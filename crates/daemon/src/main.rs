//! This crate implements the Solipr daemon.
//!
//! This daemon should run in the background and is responsible for managing
//! repositories and connecting to peers in the Solipr network.

use axum::Router;
use axum::routing::get;
use config::CONFIG;
use tokio::io;
use tokio::net::TcpListener;

mod config;

#[tokio::main]
async fn main() -> io::Result<()> {
    let app = Router::new().route("/", get(hello_world));
    let listener = TcpListener::bind(CONFIG.listen_address).await?;
    axum::serve(listener, app).await
}

/// Serves a simple "Hello, World!" message.
async fn hello_world() -> String {
    format!(
        "Hello, World!\nData folder: {}",
        CONFIG.data_folder.display()
    )
}
