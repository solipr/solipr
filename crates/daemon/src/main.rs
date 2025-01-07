//! This crate implements the Solipr daemon.
//!
//! This daemon should run in the background and is responsible for managing
//! repositories and connecting to peers in the Solipr network.

use axum::Router;
use axum::routing::get;
use config::CONFIG;
use tokio::net::TcpListener;
use tokio::{io, select, signal};

mod config;

/// Waits for a shutdown signal.
#[expect(
    clippy::redundant_pub_crate,
    reason = "the select! macro generate this error"
)]
async fn shutdown_signal() {
    let ctrl_c = async {
        #[expect(clippy::expect_used, reason = "we want to crash if this fails")]
        signal::ctrl_c()
            .await
            .expect("failed to install ctrl+c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        #[expect(clippy::expect_used, reason = "we want to crash if this fails")]
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    select! {
        () = ctrl_c => (),
        () = terminate => (),
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    println!("Starting Solipr daemon");
    let app = Router::new().route("/", get(hello_world));
    let listener = TcpListener::bind(CONFIG.listen_address).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    println!("Solipr daemon stopped");
    Ok(())
}

/// Serves a simple "Hello, World!" message.
async fn hello_world() -> String {
    format!(
        "Hello, World!\nData folder: {}",
        CONFIG.data_folder.display()
    )
}
