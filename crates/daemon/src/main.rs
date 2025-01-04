//! This crate implements the Solipr daemon.
//!
//! This daemon should run in the background and is responsible for managing
//! repositories and connecting to peers in the Solipr network.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use axum::Router;
use axum::routing::get;
use serde::{Deserialize, Serialize};
use tokio::io;
use tokio::net::TcpListener;

/// The configuration of the Solipr daemon.
#[derive(Deserialize, Serialize)]
struct Config {
    /// The path of the folder in which Solipr stores its data.
    data_folder: PathBuf,

    /// The address on which the Solipr daemon should listen.
    listen_address: SocketAddr,
}

impl Default for Config {
    fn default() -> Self {
        let home = home::home_dir().unwrap_or_default();
        Self {
            data_folder: home.join(".solipr"),
            listen_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 2718),
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let config: Config = confy::load("solipr", None).map_err(io::Error::other)?;
    let app = Router::new().route("/", get(hello_world));
    let listener = TcpListener::bind(config.listen_address).await?;
    axum::serve(listener, app).await
}

/// Serves a simple "Hello, World!" message.
async fn hello_world() -> &'static str {
    "Hello, World!"
}
