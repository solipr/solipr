//! This crate implements the Solipr daemon.
//!
//! This daemon should run in the background and is responsible for managing
//! repositories and connecting to peers in the Solipr network.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::net::UnixListener;
use tokio::{fs, io};

/// The configuration of the Solipr daemon.
#[derive(Deserialize, Serialize)]
struct Config {
    /// The path of the folder in which Solipr stores its data.
    data_folder: PathBuf,

    /// The path of the daemon's socket.
    ///
    /// This path is relative to the Solipr data folder.
    socket_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let home = home::home_dir().unwrap_or_default();
        Self {
            data_folder: home.join(".solipr"),
            socket_path: PathBuf::from("solipr.sock"),
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let config: Config = confy::load("solipr", None).map_err(io::Error::other)?;
    fs::create_dir_all(&config.data_folder).await?;
    {
        let _ = UnixListener::bind(config.data_folder.join(&config.socket_path))?;
    }
    fs::remove_file(config.data_folder.join(&config.socket_path)).await?;
    Ok(())
}
