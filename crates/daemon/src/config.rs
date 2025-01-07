//! This module is responsible for loading the daemon configuration from the
//! environment and the file system and make it accessible to the rest of the
//! program.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use config::{Config, Environment, File};
use directories::ProjectDirs;
use lazy_static::lazy_static;
use libp2p::Multiaddr;
use libp2p::multiaddr::Protocol;
use serde::Deserialize;
use serde_inline_default::serde_inline_default;

lazy_static! {
    /// The project directories.
    ///
    /// This is only used to load the configuration file and find the default data folder.
    static ref PROJECT_DIRS: ProjectDirs = load_project_dirs();

    /// The daemon configuration.
    pub static ref CONFIG: DaemonConfig = load_daemon_config();
}

/// Load the project directories.
fn load_project_dirs() -> ProjectDirs {
    #[expect(clippy::expect_used, reason = "we want to crash if this fails")]
    ProjectDirs::from("fr", "", "Solipr").expect("cannot find home directory")
}

/// Load the daemon configuration.
fn load_daemon_config() -> DaemonConfig {
    #[expect(clippy::expect_used, reason = "we want to crash if this fails")]
    Config::builder()
        .add_source(File::from(PROJECT_DIRS.config_dir().join("config")).required(false))
        .add_source(Environment::with_prefix("SOLIPR"))
        .build()
        .expect("cannot load config")
        .try_deserialize()
        .expect("cannot deserialize config")
}

/// The configuration of the Solipr daemon.
#[serde_inline_default]
#[derive(Deserialize)]
pub struct DaemonConfig {
    /// The path of the folder in which Solipr stores its data.
    #[serde_inline_default(PROJECT_DIRS.data_dir().to_owned())]
    pub data_folder: PathBuf,

    /// The address on which the daemon http api should listen.
    #[serde_inline_default(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 2718))]
    pub http_address: SocketAddr,

    /// The address on which the peer should listen.
    #[serde_inline_default(
        Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Udp(2729))
            .with(Protocol::QuicV1)
    )]
    pub peer_address: Multiaddr,
}
