//! This module contains the daemon logic, which is used by the axum server to
//! respond to requests. It is the main component of this program.
//!
//! It runs in parallel with the axum server.

use tokio::sync::Mutex;
use tokio::sync::mpsc::{Sender, channel};
use tokio::task::JoinHandle;

/// This struct represents the Solipr daemon.
pub struct SoliprDaemon {
    /// The channel used to send the stop signal to the daemon loop.
    stop_sender: Sender<()>,

    /// The handle to the daemon loop.
    ///
    /// It is stored in a mutex to make it usable using only a shared reference.
    loop_handle: Mutex<Option<JoinHandle<anyhow::Result<()>>>>,
}

impl SoliprDaemon {
    /// Starts a new daemon and returns it.
    #[expect(clippy::unused_async, reason = "it will be used later")]
    pub async fn start() -> anyhow::Result<Self> {
        let (stop_sender, mut stop_receiver) = channel(1);
        let loop_handle = Mutex::new(Some(tokio::spawn(async move {
            let _ = stop_receiver.recv().await;
            Ok::<(), anyhow::Error>(())
        })));
        Ok(Self {
            stop_sender,
            loop_handle,
        })
    }

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
