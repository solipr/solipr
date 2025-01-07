//! This module handle all the networking needed to communicate with other nodes
//! in the Solipr network.

use libp2p::futures::StreamExt;
use libp2p::swarm::dummy;
use libp2p::{Swarm, SwarmBuilder};
use tokio::select;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::task::JoinHandle;

/// This struct represents the Solipr network system.
pub struct SoliprNetwork {
    /// The channel used to send the stop signal to the network loop.
    stop_sender: Sender<()>,

    /// The handle to the network loop.
    ///
    /// It is stored in a mutex to make it usable using only a shared reference.
    loop_handle: Mutex<Option<JoinHandle<anyhow::Result<()>>>>,
}

impl SoliprNetwork {
    /// Starts a new network system and returns it.
    pub fn start() -> anyhow::Result<Self> {
        let mut swarm = SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_quic()
            .with_behaviour(|_| dummy::Behaviour)?
            .build();
        swarm.listen_on("/ip4/0.0.0.0/udp/27918/quic-v1".parse()?)?;
        let (stop_sender, stop_receiver) = channel(1);
        let loop_handle = Mutex::new(Some(tokio::spawn(network_loop(swarm, stop_receiver))));
        Ok(Self {
            stop_sender,
            loop_handle,
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
}

/// This is the main loop of the network system.
async fn network_loop(
    mut swarm: Swarm<dummy::Behaviour>,
    mut stop_receiver: Receiver<()>,
) -> anyhow::Result<()> {
    loop {
        select! {
            event = swarm.select_next_some() => println!("Network event: {event:?}"),
            _ = stop_receiver.recv() => break,
        }
    }
    Ok(())
}
