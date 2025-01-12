//! This module implements the systems needed to interact with the solipr
//! network.
//!
//! This make it possible to host a [`SoliprPeer`] that can connect to other
//! peers on the network and for other peers to connect to the hosted peer.
//!
//! This is implemented using [libp2p](https://libp2p.io/).

use std::collections::{HashSet, VecDeque};
use std::io::ErrorKind;
use std::time::Duration;

use address::MultiaddrExt;
use anyhow::{Context, bail};
pub use libp2p::PeerId;
use libp2p::autonat::NatStatus;
use libp2p::core::ConnectedPoint;
use libp2p::core::transport::ListenerId;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::identity::ed25519::Keypair as Ed25519Keypair;
use libp2p::kad::store::MemoryStore;
use libp2p::kad::{GetProvidersOk, Mode, ProgressStep, QueryId, QueryResult, RecordKey};
use libp2p::multiaddr::Protocol;
use libp2p::relay::client as relay_client;
use libp2p::swarm::{DialError, NetworkBehaviour, SwarmEvent};
use libp2p::upnp::tokio as upnp_tokio;
use libp2p::{
    Multiaddr, Swarm, SwarmBuilder, autonat, dcutr, identify, kad, noise, relay, request_response,
    yamux,
};
use rand::seq::IteratorRandom;
use solipr_config::{CONFIG, PEER_CONFIG};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::select;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::Instant;

mod address;

/// A command that can be sent to the peer system and return a result.
trait PeerCommand: Sized + Send {
    /// The type of the result of the command.
    type Output: Send;

    /// Initializes the command before sending it to the peer loop.
    ///
    /// This function use used to give to the command a reference to the
    /// [`SoliprPeer`] and a [Sender] to send the result of the command.
    ///
    /// Returns the command to send to the peer loop.
    fn initialize(self, sender: oneshot::Sender<Self::Output>) -> Box<dyn RawPeerCommand>;
}

/// A command that can be be processed by the peer loop.
trait RawPeerCommand: Send {
    /// Starts the execution of the command in the peer loop.
    ///
    /// Returns `None` if the execution of the command is finished.
    /// Returns the command to wait for an event from the peer loop,
    /// the next events will be passed to the [`RawPeerCommand::on_event`]
    /// function.
    ///
    /// This function should not block or execute other commands
    /// because it will block the execution of the peer loop.
    fn start(self: Box<Self>, swarm: &mut Swarm<Behaviour>) -> Option<Box<dyn RawPeerCommand>>;

    /// Executed for each peer event in the peer loop when the command is
    /// running.
    ///
    /// Returns `None` if the execution of the command is finished.
    ///
    /// This function should not block or execute other commands
    /// because it will block the execution of the peer loop.
    fn on_event(
        self: Box<Self>,
        _swarm: &mut Swarm<Behaviour>,
        _event: &SwarmEvent<BehaviourEvent>,
    ) -> Option<Box<dyn RawPeerCommand>> {
        unreachable!();
    }
}

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

    /// A behaviour used to make direct connection using relays.
    dcutr: dcutr::Behaviour,

    /// A behaviour used to open ports on the router.
    upnp: upnp_tokio::Behaviour,

    /// A behaviour that implement the Kademlia protocol.
    kad: kad::Behaviour<MemoryStore>,
}

/// The local peer that connect to the solipr network.
pub struct SoliprPeer {
    /// The [PeerId] of the [SoliprPeer].
    peer_id: PeerId,

    /// A [Sender] to send a stop signal to the internal loop.
    stop_sender: Sender<()>,

    /// The [`JoinHandle`] of the internal loop task.
    ///
    /// This is used to wait for the internal loop to finish in the
    /// [`Self::stop`] function.
    loop_handle: JoinHandle<()>,

    /// The [Sender] used to send [`PeerCommand`] to the peer loop.
    command_sender: Sender<Box<dyn RawPeerCommand>>,

    /// A [HashSet] of the keys that are provided by the [SoliprPeer].
    provided_keys: HashSet<Vec<u8>>,
}

impl SoliprPeer {
    /// Load the keypair from the os using [keyring].
    fn load_keypair() -> anyhow::Result<Keypair> {
        if let Some(mut bytes) = PEER_CONFIG.keypair {
            return Ok(Ed25519Keypair::try_from_bytes(&mut bytes)?.into());
        }
        let entry = keyring::Entry::new("solipr-peer", &whoami::username())?;
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
                    dcutr: dcutr::Behaviour::new(key.public().to_peer_id()),
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
        let peer_id = *swarm.local_peer_id();
        for address in &PEER_CONFIG.listen_addresses {
            swarm.listen_on(address.clone())?;
        }
        let known_addresses = Self::load_known_addresses().await?;
        for address in known_addresses
            .iter()
            .chain(PEER_CONFIG.bootstrap_addresses.iter())
        {
            if let Some(peer_id) = address.peer_id() {
                if peer_id == *swarm.local_peer_id() {
                    continue;
                }
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
        let (command_sender, command_receiver) = channel(1);
        let solipr_loop = SoliprPeerLoop {
            swarm,
            stop_receiver,
            known_addresses,
            relay_listener: None,
            relay_start_at: Instant::now(),
            command_receiver,
        };
        let loop_handle = tokio::spawn(async move {
            if let Err(error) = solipr_loop.internal_loop().await {
                println!("Internal loop error: {error}");
            }
        });
        Ok(Self {
            peer_id,
            stop_sender,
            loop_handle,
            command_sender,
            provided_keys: HashSet::new(),
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
            .context("peer loop is not running")?;
        self.loop_handle.await?;
        Ok(())
    }

    /// Execute a command in the peer system and return the result.
    async fn command<C: PeerCommand>(&self, command: C) -> anyhow::Result<C::Output> {
        let (sender, receiver) = oneshot::channel();
        let command = command.initialize(sender);
        if self.command_sender.send(command).await.is_err() {
            bail!("peer loop is not running");
        }
        receiver
            .await
            .context("peer command was dropped before completion")
    }

    /// Returns the [PeerId] of the [`SoliprPeer`].
    pub fn id(&self) -> PeerId {
        self.peer_id
    }

    /// Advertise the solipr network that this node provide the given key.
    pub async fn provide_key(&mut self, key: impl AsRef<[u8]>) -> anyhow::Result<bool> {
        /// The command used for this function.
        pub struct Command(RecordKey);

        /// The raw command generated by [Command].
        struct RawCommand(oneshot::Sender<bool>, RecordKey);

        /// The command that wait the query to be completed.
        struct WaitForQuery(oneshot::Sender<bool>, QueryId);

        impl PeerCommand for Command {
            type Output = bool;

            fn initialize(self, sender: oneshot::Sender<Self::Output>) -> Box<dyn RawPeerCommand> {
                Box::new(RawCommand(sender, self.0))
            }
        }

        impl RawPeerCommand for RawCommand {
            fn start(
                self: Box<Self>,
                swarm: &mut Swarm<Behaviour>,
            ) -> Option<Box<dyn RawPeerCommand>> {
                let Ok(query) = swarm.behaviour_mut().kad.start_providing(self.1) else {
                    let _ = self.0.send(false);
                    return None;
                };
                Some(Box::new(WaitForQuery(self.0, query)))
            }
        }

        impl RawPeerCommand for WaitForQuery {
            fn start(
                self: Box<Self>,
                _swarm: &mut Swarm<Behaviour>,
            ) -> Option<Box<dyn RawPeerCommand>> {
                unreachable!();
            }

            fn on_event(
                self: Box<Self>,
                _swarm: &mut Swarm<Behaviour>,
                event: &SwarmEvent<BehaviourEvent>,
            ) -> Option<Box<dyn RawPeerCommand>> {
                if let SwarmEvent::Behaviour(BehaviourEvent::Kad(
                    kad::Event::OutboundQueryProgressed {
                        id,
                        result: QueryResult::StartProviding(result),
                        step: ProgressStep { last: true, .. },
                        ..
                    },
                )) = event
                {
                    if *id == self.1 {
                        match result {
                            Ok(_) => {
                                let _ = self.0.send(true);
                            }
                            Err(_) => {
                                let _ = self.0.send(false);
                            }
                        }
                        return None;
                    }
                }
                Some(self)
            }
        }

        let result = self.command(Command(RecordKey::new(&key.as_ref()))).await?;
        if result {
            self.provided_keys.insert(key.as_ref().to_vec());
        }
        Ok(result)
    }

    /// Stop providing the given key.
    pub async fn stop_providing_key(&mut self, key: impl AsRef<[u8]>) -> anyhow::Result<()> {
        /// The command used for this function.
        pub struct Command(RecordKey);

        /// The raw command generated by [Command].
        struct RawCommand(oneshot::Sender<()>, RecordKey);

        impl PeerCommand for Command {
            type Output = ();

            fn initialize(self, sender: oneshot::Sender<Self::Output>) -> Box<dyn RawPeerCommand> {
                Box::new(RawCommand(sender, self.0))
            }
        }

        impl RawPeerCommand for RawCommand {
            fn start(
                self: Box<Self>,
                swarm: &mut Swarm<Behaviour>,
            ) -> Option<Box<dyn RawPeerCommand>> {
                swarm.behaviour_mut().kad.stop_providing(&self.1);
                let _ = self.0.send(());
                None
            }
        }

        self.command(Command(RecordKey::new(&key.as_ref()))).await
    }

    /// Returns the keys that are currently provided.
    pub fn provided_keys(&mut self) -> &HashSet<Vec<u8>> {
        &self.provided_keys
    }

    /// Returns the [PeerId] of the [`SoliprPeer`] that currently provides the
    /// given key.
    pub async fn find_providers(&self, key: impl AsRef<[u8]>) -> anyhow::Result<HashSet<PeerId>> {
        /// The command used for this function.
        pub struct Command(RecordKey);

        /// The raw command generated by [Command].
        struct RawCommand(oneshot::Sender<HashSet<PeerId>>, RecordKey);

        /// The command that wait the query to be completed.
        struct WaitForQuery(oneshot::Sender<HashSet<PeerId>>, QueryId, HashSet<PeerId>);

        impl PeerCommand for Command {
            type Output = HashSet<PeerId>;

            fn initialize(self, sender: oneshot::Sender<Self::Output>) -> Box<dyn RawPeerCommand> {
                Box::new(RawCommand(sender, self.0))
            }
        }

        impl RawPeerCommand for RawCommand {
            fn start(
                self: Box<Self>,
                swarm: &mut Swarm<Behaviour>,
            ) -> Option<Box<dyn RawPeerCommand>> {
                let Ok(query) = swarm.behaviour_mut().kad.start_providing(self.1) else {
                    let _ = self.0.send(HashSet::new());
                    return None;
                };
                Some(Box::new(WaitForQuery(self.0, query, HashSet::new())))
            }
        }

        impl RawPeerCommand for WaitForQuery {
            fn start(
                self: Box<Self>,
                _swarm: &mut Swarm<Behaviour>,
            ) -> Option<Box<dyn RawPeerCommand>> {
                unreachable!();
            }

            fn on_event(
                mut self: Box<Self>,
                _swarm: &mut Swarm<Behaviour>,
                event: &SwarmEvent<BehaviourEvent>,
            ) -> Option<Box<dyn RawPeerCommand>> {
                if let SwarmEvent::Behaviour(BehaviourEvent::Kad(
                    kad::Event::OutboundQueryProgressed {
                        id,
                        result: QueryResult::GetProviders(result),
                        ..
                    },
                )) = event
                {
                    if *id == self.1 {
                        match result {
                            Ok(GetProvidersOk::FoundProviders { providers, .. }) => {
                                self.2.extend(providers);
                            }
                            Err(_) | Ok(GetProvidersOk::FinishedWithNoAdditionalRecord { .. }) => {
                                let _ = self.0.send(self.2);
                                return None;
                            }
                        }
                    }
                }
                Some(self)
            }
        }

        self.command(Command(RecordKey::new(&key.as_ref()))).await
    }
}

struct SoliprPeerLoop {
    swarm: Swarm<Behaviour>,
    stop_receiver: Receiver<()>,
    known_addresses: HashSet<Multiaddr>,
    relay_listener: Option<ListenerId>,
    relay_start_at: Instant,
    command_receiver: Receiver<Box<dyn RawPeerCommand>>,
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
        let mut current_commands: VecDeque<Box<dyn RawPeerCommand>> = VecDeque::new();
        let mut global_update_timer = tokio::time::interval(Duration::from_secs(1));
        loop {
            select! {
                _ = self.stop_receiver.recv() => break,
                event = self.swarm.select_next_some() => {
                    for _ in 0..current_commands.len() {
                        if let Some(command) = current_commands.pop_front() {
                            if let Some(command) = command.on_event(&mut self.swarm, &event) {
                                current_commands.push_back(command);
                            }
                        }
                    }
                    self.update_known_addresses(&event).await?;
                    self.update_behaviours_addresses(&event).await?;
                }
                command = self.command_receiver.recv() => {
                    let Some(command) = command else { break; };
                    if let Some(command) = command.start(&mut self.swarm) {
                        current_commands.push_back(command);
                    }
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
            SwarmEvent::OutgoingConnectionError {
                peer_id: Some(peer_id),
                error:
                    DialError::WrongPeerId {
                        endpoint: ConnectedPoint::Dialer { address, .. },
                        ..
                    },
                ..
            } => {
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
