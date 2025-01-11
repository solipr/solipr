//! This module is responsible for loading the main configuration from the
//! environment and the file system and make it accessible to the rest of the
//! program.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

use config::{Config, Environment, File};
use directories::ProjectDirs;
use libp2p::{Multiaddr};
use libp2p::multiaddr::Protocol;
use serde::Deserialize;
use serde_inline_default::serde_inline_default;

/// The project directories.
///
/// This is only used to load the configuration file and find the default data
/// folder.
static PROJECT_DIRS: LazyLock<ProjectDirs> = LazyLock::new(load_project_dirs);

/// The main configuration.
pub static CONFIG: LazyLock<SoliprConfig> = LazyLock::new(load_main_config);

/// The daemon configuration.
pub static DAEMON_CONFIG: LazyLock<DaemonConfig> = LazyLock::new(load_daemon_config);

/// The peer configuration.
pub static PEER_CONFIG: LazyLock<PeerConfig> = LazyLock::new(load_peer_config);

/// Load the project directories.
fn load_project_dirs() -> ProjectDirs {
    #[expect(clippy::expect_used, reason = "we want to crash if this fails")]
    ProjectDirs::from("fr", "", "Solipr").expect("cannot find home directory")
}

/// Load the main configuration.
fn load_main_config() -> SoliprConfig {
    #[expect(clippy::expect_used, reason = "we want to crash if this fails")]
    Config::builder()
        .add_source(File::from(PROJECT_DIRS.config_dir().join("global")).required(false))
        .add_source(Environment::with_prefix("SOLIPR"))
        .build()
        .expect("cannot load config")
        .try_deserialize()
        .expect("cannot deserialize config")
}

/// Load the daemon configuration.
fn load_daemon_config() -> DaemonConfig {
    #[expect(clippy::expect_used, reason = "we want to crash if this fails")]
    Config::builder()
        .add_source(File::from(PROJECT_DIRS.config_dir().join("daemon")).required(false))
        .add_source(Environment::with_prefix("SOLIPR_DAEMON"))
        .build()
        .expect("cannot load config")
        .try_deserialize()
        .expect("cannot deserialize config")
}

/// Load the peer configuration.
fn load_peer_config() -> PeerConfig {
    #[expect(clippy::expect_used, reason = "we want to crash if this fails")]
    Config::builder()
        .add_source(File::from(PROJECT_DIRS.config_dir().join("peer")).required(false))
        .add_source(Environment::with_prefix("SOLIPR_PEER"))
        .build()
        .expect("cannot load config")
        .try_deserialize()
        .expect("cannot deserialize config")
}

/// The main configuration of Solipr.
#[serde_inline_default]
#[derive(Deserialize)]
pub struct SoliprConfig {
    /// The path of the folder in which Solipr stores its data.
    #[serde_inline_default(PROJECT_DIRS.data_dir().to_owned())]
    pub data_folder: PathBuf,
}

/// The configuration of the Solipr daemon.
#[serde_inline_default]
#[derive(Deserialize)]
pub struct DaemonConfig {
    /// The listening address of the daemon for the http api.
    #[serde_inline_default(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 2718))]
    pub listen_address: SocketAddr,
}

/// The configuration of the Solipr peer.
#[serde_inline_default]
#[derive(Deserialize)]
pub struct PeerConfig {
    /// The addresses on which the peer should listen for incoming connections
    /// of other peers.
    #[serde_inline_default(
        HashSet::from_iter([Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Udp(2729))
            .with(Protocol::QuicV1)])
    )]
    pub listen_addresses: HashSet<Multiaddr>,

    /// The keypair to use as the private key of the peer.
    ///
    /// If this is not set, it will generated or loaded from the os using the
    /// keyring library.
    #[serde_inline_default(None)]
    #[serde(with = "serde_bytes")]
    pub keypair: Option<[u8; 64]>,

    /// The addresses of the peer used to join the network for the first time.
    #[serde_inline_default(HashSet::from_iter([
        "/ip4/79.90.77.127/udp/2729/quic-v1/p2p/12D3KooWRuA21w8ZPw8berCXTPAHu4Fsk2kvGPQ4b8BbFyY4MfbV".parse().unwrap()
    ]))]
    pub bootstrap_addresses: HashSet<Multiaddr>,

    /// The maximum number of other peers address to store.
    #[serde_inline_default(256)]
    pub max_stored_addresses: usize,

    /// The time to wait before marking a relay as dead.
    #[serde_inline_default(Duration::from_secs(10))]
    pub relay_timeout: Duration,
}
