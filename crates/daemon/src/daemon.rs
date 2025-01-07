//! This module contains the daemon logic, which is used by the axum server to
//! respond to requests. It is the main component of this program.
//!
//! It runs in parallel with the axum server.

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
}
