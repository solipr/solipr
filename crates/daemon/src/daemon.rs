//! This module contains the daemon logic, which is used by the axum server to
//! respond to requests. It is the main component of this program.
//!
//! It runs in parallel with the axum server.

use std::collections::HashSet;
use std::ops::Deref;
use std::sync::Arc;

use libp2p::Multiaddr;
use network::SoliprNetwork;
use tokio::select;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::task::JoinHandle;

mod network;

/// A handle to the Solipr daemon.
pub struct DaemonHandle {
    /// The channel used to send the stop signal to the daemon loop.
    stop_sender: Sender<()>,
    /// The handle to the daemon loop.
    ///
    /// It is stored in a mutex to make it usable using only a shared reference.
    loop_handle: Mutex<Option<JoinHandle<anyhow::Result<()>>>>,

    /// The real daemon.
    daemon: Arc<SoliprDaemon>,
}

impl DaemonHandle {
    /// Send a stop signal to the daemon loop and wait for it to finish.
    ///
    /// If the daemon loop is not running, this function does nothing.
    pub async fn stop(&self) -> anyhow::Result<()> {
        let loop_handle = self.loop_handle.lock().await.take();
        if let Some(handle) = loop_handle {
            let _ = self.stop_sender.send(()).await;
            handle.await??;
        }
        Ok(())
    }
}

impl Deref for DaemonHandle {
    type Target = SoliprDaemon;

    fn deref(&self) -> &Self::Target {
        &self.daemon
    }
}

/// This struct represents the Solipr daemon.
pub struct SoliprDaemon {
    /// The network system of the daemon.
    network: SoliprNetwork,
}

impl SoliprDaemon {
    /// Starts a new daemon and returns it.
    pub fn start() -> anyhow::Result<DaemonHandle> {
        let daemon = Arc::new(Self {
            network: SoliprNetwork::start()?,
        });
        let (stop_sender, stop_receiver) = channel(1);
        let daemon_clone = Arc::clone(&daemon);
        let loop_handle = Mutex::new(Some(tokio::spawn(async move {
            daemon_clone.main_loop(stop_receiver).await
        })));
        Ok(DaemonHandle {
            stop_sender,
            loop_handle,
            daemon,
        })
    }

    /// Returns the external addresses of the daemon.
    ///
    /// These addresses can be used by other peers to connect to the daemon.
    pub async fn external_addresses(&self) -> anyhow::Result<HashSet<Multiaddr>> {
        self.network.external_addresses().await
    }

    /// The main loop of the daemon.
    async fn main_loop(&self, mut stop_receiver: Receiver<()>) -> anyhow::Result<()> {
        loop {
            select! {
                _ = stop_receiver.recv() => break,
                event = self.network.next_event() => {
                    println!("Network event: {event:?}");
                }
            }
        }
        self.network.stop().await
    }
}
