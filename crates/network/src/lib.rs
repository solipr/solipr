//! This module implements the systems needed to interact with the solipr
//! network.
//!
//! This make it possible to host a [`SoliprPeer`] that can connect to other
//! peers on the network and for other peers to connect to the hosted peer.
//!
//! This is implemented using [libp2p](https://libp2p.io/).

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::Duration;

use address::MultiaddrExt;
use anyhow::Context;
use libp2p::autonat::NatStatus;
use libp2p::core::transport::ListenerId;
use libp2p::core::{ConnectedPoint, Endpoint};
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::identity::ed25519::Keypair as Ed25519Keypair;
use libp2p::kad::Mode;
use libp2p::kad::store::MemoryStore;
use libp2p::multiaddr::Protocol;
use libp2p::relay::client as relay_client;
use libp2p::swarm::{DialError, NetworkBehaviour, SwarmEvent};
use libp2p::upnp::tokio as upnp_tokio;
use libp2p::{Multiaddr, PeerId, Swarm, SwarmBuilder, autonat, identify, kad, noise, relay, yamux};
use rand::seq::IteratorRandom;
use solipr_config::{CONFIG, PEER_CONFIG};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::select;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::task::JoinHandle;
use tokio::time::Instant;

mod address;

/// The behaviour used by the [Swarm] in [`SoliprPeer`].
#[derive(NetworkBehaviour)]
struct Behaviour {
    /// A behaviour used to get information about other peers.
    identify: identify::Behaviour,

    /// A behaviour used to know if the peer has a public address.
    autonat: autonat::Behaviour,

    /// A behaviour used to be a relay server for other peers.
    relay_server: relay::Behaviour,

    /// A behaviour used to use an other peer as a relay server.
    relay_client: relay_client::Behaviour,

    /// A behaviour used to open ports on the router.
    upnp: upnp_tokio::Behaviour,

    /// A behaviour that implement the Kademlia protocol.
    kad: kad::Behaviour<MemoryStore>,
}

/// The local peer that connect to the solipr network.
pub struct SoliprPeer {
    /// A [Sender] to send a stop signal to the internal loop.
    stop_sender: Sender<()>,

    /// The [`JoinHandle`] of the internal loop task.
    ///
    /// This is used to wait for the internal loop to finish in the
    /// [`Self::stop`] function.
    loop_handle: JoinHandle<()>,
}

impl SoliprPeer {
    /// Load the keypair from the os using [keyring].
    fn load_keypair() -> anyhow::Result<Keypair> {
        if let Some(mut bytes) = PEER_CONFIG.keypair {
            return Ok(Ed25519Keypair::try_from_bytes(&mut bytes)?.into());
        }
        let entry = keyring::Entry::new("solipr", &whoami::username())?;
        match entry.get_secret() {
            Ok(mut bytes) => Ok(Ed25519Keypair::try_from_bytes(&mut bytes)?.into()),
            Err(keyring::Error::NoEntry) => {
                let keypair = Ed25519Keypair::generate();
                entry.set_secret(keypair.to_bytes().as_slice())?;
                Ok(keypair.into())
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Build the [Swarm] of the [SoliprPeer].
    fn build_swarm() -> anyhow::Result<Swarm<Behaviour>> {
        let mut swarm = SwarmBuilder::with_existing_identity(Self::load_keypair()?)
            .with_tokio()
            .with_quic()
            .with_relay_client(noise::Config::new, yamux::Config::default)?
            .with_behaviour(|key, relay_behaviour| {
                Ok(Behaviour {
                    identify: identify::Behaviour::new(
                        identify::Config::new(
                            format!("solipr/{}", env!("CARGO_PKG_VERSION")),
                            key.public(),
                        )
                        .with_hide_listen_addrs(true),
                    ),
                    autonat: autonat::Behaviour::new(key.public().to_peer_id(), autonat::Config {
                        boot_delay: Duration::from_millis(100),
                        ..Default::default()
                    }),
                    relay_server: relay::Behaviour::new(
                        key.public().to_peer_id(),
                        relay::Config::default(),
                    ),
                    relay_client: relay_behaviour,
                    upnp: upnp_tokio::Behaviour::default(),
                    kad: kad::Behaviour::new(
                        key.public().to_peer_id(),
                        MemoryStore::new(key.public().to_peer_id()),
                    ),
                })
            })?
            .with_swarm_config(|config| config.with_idle_connection_timeout(Duration::from_secs(1)))
            .build();
        swarm.behaviour_mut().kad.set_mode(Some(Mode::Client));
        Ok(swarm)
    }

    /// Load the known addresses.
    async fn load_known_addresses() -> anyhow::Result<HashSet<Multiaddr>> {
        let mut addresses = HashSet::new();
        let file = match File::open(CONFIG.data_folder.join("known_addresses")).await {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(addresses),
            Err(err) => return Err(err.into()),
        };
        let mut lines = BufReader::new(file).lines();
        while let Some(line) = lines.next_line().await? {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(address) = line.parse::<Multiaddr>() {
                if PEER_CONFIG.bootstrap_addresses.contains(&address) {
                    continue;
                }
                addresses.insert(address);
            }
        }
        Ok(addresses)
    }

    /// Start the peer and returns it.
    ///
    /// # Errors
    ///
    /// Returns an error if the peer cannot be started.
    pub async fn start() -> anyhow::Result<Self> {
        let mut swarm = Self::build_swarm()?;
        for address in &PEER_CONFIG.listen_addresses {
            swarm.listen_on(address.clone())?;
        }
        let known_addresses = Self::load_known_addresses().await?;
        for address in known_addresses
            .iter()
            .chain(PEER_CONFIG.bootstrap_addresses.iter())
        {
            if let Some(peer_id) = address.peer_id() {
                swarm.add_peer_address(peer_id, address.clone());
                swarm
                    .behaviour_mut()
                    .autonat
                    .add_server(peer_id, Some(address.clone()));
                swarm
                    .behaviour_mut()
                    .kad
                    .add_address(&peer_id, address.clone());
            }
        }
        let (stop_sender, stop_receiver) = channel(1);
        let solipr_loop = SoliprPeerLoop {
            swarm,
            stop_receiver,
            known_addresses,
            relay_listener: None,
            relay_start_at: Instant::now(),
        };
        let loop_handle = tokio::spawn(async move {
            if let Err(error) = solipr_loop.internal_loop().await {
                println!("Internal loop error: {error}");
            }
        });
        Ok(Self {
            stop_sender,
            loop_handle,
        })
    }

    /// Send a stop signal to the internal loop of the [`SoliprPeer`] and wait
    /// for it to be fully stopped.
    ///
    /// # Errors
    ///
    /// Returns an error if the internal loop is not running.
    pub async fn stop(self) -> anyhow::Result<()> {
        self.stop_sender
            .send(())
            .await
            .context("network loop is not running")?;
        self.loop_handle.await?;
        Ok(())
    }
}

struct SoliprPeerLoop {
    swarm: Swarm<Behaviour>,
    stop_receiver: Receiver<()>,
    known_addresses: HashSet<Multiaddr>,
    relay_listener: Option<ListenerId>,
    relay_start_at: Instant,
}

impl SoliprPeerLoop {
    /// Returns a random address from the known addresses or the bootstrap
    /// addresses if the known addresses are empty.
    fn get_random_address(&self) -> Option<&Multiaddr> {
        self.known_addresses
            .iter()
            .choose(&mut rand::thread_rng())
            .or_else(|| {
                PEER_CONFIG
                    .bootstrap_addresses
                    .iter()
                    .choose(&mut rand::thread_rng())
            })
    }

    /// The internal loop of the [`SoliprPeer`].
    async fn internal_loop(mut self) -> anyhow::Result<()> {
        let mut global_update_timer = tokio::time::interval(Duration::from_secs(1));
        loop {
            select! {
                _ = self.stop_receiver.recv() => break,
                event = self.swarm.select_next_some() => {
                    self.update_known_addresses(&event).await?;
                    self.update_behaviours_addresses(&event).await?;
                    println!("Network event: {event:?}");
                }
                _ = global_update_timer.tick() => {
                    self.update_relay_connection().await?;
                }
            }
        }
        Ok(())
    }

    /// Update known address list with the given event.
    async fn update_known_addresses(
        &mut self,
        event: &SwarmEvent<BehaviourEvent>,
    ) -> anyhow::Result<()> {
        let mut need_save = false;
        match event {
            SwarmEvent::ConnectionEstablished {
                concurrent_dial_errors,
                endpoint: ConnectedPoint::Dialer { address, .. },
                ..
            } => {
                if let Some(errors) = concurrent_dial_errors {
                    for (address, _) in errors {
                        if self.known_addresses.remove(address) {
                            need_save = true;
                        }
                    }
                }
                if self.known_addresses.len() < PEER_CONFIG.max_stored_addresses
                    && address.is_public()
                    && !PEER_CONFIG.bootstrap_addresses.contains(address)
                    && self.known_addresses.insert(address.clone())
                {
                    need_save = true;
                }
            }
            SwarmEvent::OutgoingConnectionError {
                error:
                    DialError::WrongPeerId {
                        endpoint: ConnectedPoint::Dialer { address, .. },
                        ..
                    },
                ..
            } => {
                if self.known_addresses.remove(address) {
                    need_save = true;
                }
            }
            SwarmEvent::OutgoingConnectionError {
                error: DialError::Transport(errors),
                ..
            } => {
                for (address, _) in errors {
                    if self.known_addresses.remove(address) {
                        need_save = true;
                    }
                }
            }
            _ => {}
        };
        if need_save {
            let mut file = File::create(CONFIG.data_folder.join("known_addresses")).await?;
            for address in self.known_addresses.iter() {
                file.write_all(format!("{address}\n").as_bytes()).await?;
            }
            file.flush().await?;
            println!("Known addresses updated");
        }
        Ok(())
    }

    /// Update the addresses used by the beaviours with the given event.
    async fn update_behaviours_addresses(
        &mut self,
        event: &SwarmEvent<BehaviourEvent>,
    ) -> anyhow::Result<()> {
        match event {
            SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
                peer_id,
                info,
                ..
            })) => {
                if !info.protocol_version.starts_with("solipr/") {
                    return Ok(());
                }
                for address in &info.listen_addrs {
                    if address.is_public() && address.peer_id() == Some(*peer_id) {
                        self.swarm
                            .behaviour_mut()
                            .autonat
                            .add_server(*peer_id, Some(address.clone()));
                        self.swarm
                            .behaviour_mut()
                            .kad
                            .add_address(peer_id, address.clone());
                    }
                }
            }
            SwarmEvent::OutgoingConnectionError {
                peer_id: Some(peer_id),
                error: DialError::Transport(errors),
                ..
            } => {
                for (address, _) in errors {
                    if address.is_public()
                        && address.peer_id() == Some(*peer_id)
                        && !PEER_CONFIG.bootstrap_addresses.contains(address)
                    {
                        self.swarm.behaviour_mut().autonat.remove_server(peer_id);
                        self.swarm
                            .behaviour_mut()
                            .kad
                            .remove_address(peer_id, address);
                    }
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Autonat(autonat::Event::StatusChanged {
                new,
                ..
            })) => {
                self.swarm.behaviour_mut().kad.set_mode(Some(match new {
                    NatStatus::Public(_) => Mode::Server,
                    _ => Mode::Client,
                }));
            }
            _ => (),
        }
        Ok(())
    }

    /// Update the connection to a relay server.
    async fn update_relay_connection(&mut self) -> anyhow::Result<()> {
        match self.swarm.behaviour().autonat.nat_status() {
            NatStatus::Private => match &self.relay_listener {
                None => {
                    let Some(random_address) = self.get_random_address() else {
                        return Ok(());
                    };
                    println!("Starting relay connection to {random_address}");
                    let random_address = random_address.clone().with(Protocol::P2pCircuit);
                    self.relay_listener = Some(self.swarm.listen_on(random_address)?);
                    self.relay_start_at = Instant::now();
                }
                Some(listener) => {
                    let is_connected = self.swarm.external_addresses().any(|address| {
                        address
                            .iter()
                            .any(|protocol| matches!(protocol, Protocol::P2pCircuit))
                    });
                    if !is_connected && self.relay_start_at.elapsed() > PEER_CONFIG.relay_timeout {
                        self.swarm.remove_listener(*listener);
                        self.relay_listener = None;
                        println!("Relay connection timed out");
                    }
                }
            },
            NatStatus::Public(_) => {
                if let Some(listener) = self.relay_listener.take() {
                    self.swarm.remove_listener(listener);
                    println!("Peer is public, closing relay listener");
                }
            }
            NatStatus::Unknown => {}
        }
        Ok(())
    }
}
