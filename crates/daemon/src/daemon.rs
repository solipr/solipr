//! This module contains the daemon logic, which is used by the axum server to
//! respond to requests. It is the main component of this program.
//!
//! It runs in parallel with the axum server.

use std::collections::HashSet;

use libp2p::Multiaddr;
use network::SoliprNetwork;

mod network;

/// This struct represents the Solipr daemon.
pub struct SoliprDaemon {
    /// The network system of the daemon.
    network: SoliprNetwork,
}

impl SoliprDaemon {
    /// Starts a new daemon and returns it.
    pub fn start() -> anyhow::Result<Self> {
        Ok(Self {
            network: SoliprNetwork::start()?,
        })
    }

    /// Send a stop signal to the daemon loop and wait for it to finish.
    ///
    /// If the daemon loop is not running, this function does nothing.
    pub async fn stop(&self) -> anyhow::Result<()> {
        self.network.stop().await
    }

    /// Returns the external addresses of the daemon.
    ///
    /// These addresses can be used by other peers to connect to the daemon.
    pub async fn external_addresses(&self) -> anyhow::Result<HashSet<Multiaddr>> {
        self.network.external_addresses().await
    }
}
