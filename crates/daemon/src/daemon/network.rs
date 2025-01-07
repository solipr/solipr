//! This module handle all the networking needed to communicate with other nodes
//! in the Solipr network.

use std::collections::{HashSet, VecDeque};

use anyhow::{Context, bail};
use libp2p::futures::StreamExt;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::upnp::tokio::Behaviour as UpnpBehaviour;
use libp2p::{Multiaddr, Swarm, SwarmBuilder};
use tokio::select;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;

use crate::config::CONFIG;

/// A command that can be sent to the network system and return a result.
trait NetworkCommand: Sized + Send {
    /// The type of the result of the command.
    type Output: Send;

    /// Initializes the command before sending it to the network loop.
    ///
    /// This function use used to give to the command a reference to the
    /// [`SoliprNetwork`] and a [Sender] to send the result of the command.
    ///
    /// Returns the command to send to the network loop.
    fn initialize(self, sender: oneshot::Sender<Self::Output>) -> Box<dyn RawNetworkCommand>;
}

/// A command that can be be processed by the network loop.
trait RawNetworkCommand: Send {
    /// Starts the execution of the command in the network loop.
    ///
    /// Returns `None` if the execution of the command is finished.
    /// Returns the command to wait for an event from the network loop,
    /// the next events will be passed to the [`RawNetworkCommand::on_event`]
    /// function.
    ///
    /// This function should not block or execute other commands
    /// because it will block the execution of the network loop.
    fn start(self: Box<Self>, swarm: &mut Swarm<Behaviour>) -> Option<Box<dyn RawNetworkCommand>>;

    /// Executed for each network event in the network loop when the command is
    /// running.
    ///
    /// Returns `None` if the execution of the command is finished.
    ///
    /// This function should not block or execute other commands
    /// because it will block the execution of the network loop.
    fn on_event(
        self: Box<Self>,
        _swarm: &mut Swarm<Behaviour>,
        _event: &SwarmEvent<BehaviourEvent>,
    ) -> Option<Box<dyn RawNetworkCommand>> {
        unreachable!();
    }
}

/// The [`NetworkBehaviour`] used by the network system [Swarm].
///
/// It is a combination of multiple [`NetworkBehaviour`] joined together.
#[derive(NetworkBehaviour)]
struct Behaviour {
    /// Automatically tries to map the external port to an internal address on
    /// the gateway.
    upnp: UpnpBehaviour,
}

/// This struct represents the Solipr network system.
pub struct SoliprNetwork {
    /// The channel used to send the stop signal to the network loop.
    stop_sender: Sender<()>,

    /// The handle to the network loop.
    ///
    /// It is stored in a mutex to make it usable using only a shared reference.
    loop_handle: Mutex<Option<JoinHandle<anyhow::Result<()>>>>,

    /// The [Sender] used to send [`NetworkCommand`] to the network loop.
    command_sender: Sender<Box<dyn RawNetworkCommand>>,
}

impl SoliprNetwork {
    /// Starts a new network system and returns it.
    pub fn start() -> anyhow::Result<Self> {
        let mut swarm = SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_quic()
            .with_behaviour(|_| Behaviour {
                upnp: UpnpBehaviour::default(),
            })?
            .build();
        swarm.listen_on(CONFIG.peer_address.clone())?;
        let (stop_sender, stop_receiver) = channel(1);
        let (command_sender, command_receiver) = channel(1);
        let loop_handle = Mutex::new(Some(tokio::spawn(network_loop(
            swarm,
            stop_receiver,
            command_receiver,
        ))));
        Ok(Self {
            stop_sender,
            loop_handle,
            command_sender,
        })
    }

    /// Send a stop signal to the network loop and wait for it to finish.
    ///
    /// If the network loop is not running, this function does nothing.
    pub async fn stop(&self) -> anyhow::Result<()> {
        let loop_handle = self.loop_handle.lock().await.take();
        if let Some(handle) = loop_handle {
            let _ = self.stop_sender.send(()).await;
            handle.await??;
        }
        Ok(())
    }

    /// Execute a command in the network system and return the result.
    async fn command<C: NetworkCommand>(&self, command: C) -> anyhow::Result<C::Output> {
        let (sender, receiver) = oneshot::channel();
        let command = command.initialize(sender);
        if self.command_sender.send(command).await.is_err() {
            bail!("network loop is not running");
        }
        receiver
            .await
            .context("network command was dropped before completion")
    }

    /// Returns the external addresses of the network system.
    ///
    /// These addresses can be used by other peers to connect to the network
    /// system.
    pub async fn external_addresses(&self) -> anyhow::Result<HashSet<Multiaddr>> {
        /// The command used for this function.
        pub struct Command;

        /// The raw command generated by [Command].
        struct RawCommand(oneshot::Sender<HashSet<Multiaddr>>);

        impl NetworkCommand for Command {
            type Output = HashSet<Multiaddr>;

            fn initialize(
                self,
                sender: oneshot::Sender<Self::Output>,
            ) -> Box<dyn RawNetworkCommand> {
                Box::new(RawCommand(sender))
            }
        }

        impl RawNetworkCommand for RawCommand {
            fn start(
                self: Box<Self>,
                swarm: &mut Swarm<Behaviour>,
            ) -> Option<Box<dyn RawNetworkCommand>> {
                let _ = self
                    .0
                    .send(swarm.external_addresses().cloned().collect::<HashSet<_>>());
                None
            }
        }

        self.command(Command).await
    }
}

/// This is the main loop of the network system.
async fn network_loop(
    mut swarm: Swarm<Behaviour>,
    mut stop_receiver: Receiver<()>,
    mut command_receiver: Receiver<Box<dyn RawNetworkCommand>>,
) -> anyhow::Result<()> {
    let mut current_commands: VecDeque<Box<dyn RawNetworkCommand>> = VecDeque::new();
    loop {
        select! {
            event = swarm.select_next_some() => {
                println!("Network event: {event:?}");
                for _ in 0..current_commands.len() {
                    if let Some(command) = current_commands.pop_front() {
                        if let Some(command) = command.on_event(&mut swarm, &event) {
                            current_commands.push_back(command);
                        }
                    }
                }
            },
            _ = stop_receiver.recv() => break,
            command = command_receiver.recv() => {
                let Some(command) = command else { break; };
                if let Some(command) = command.start(&mut swarm) {
                    current_commands.push_back(command);
                }
            }
        }
    }
    Ok(())
}
