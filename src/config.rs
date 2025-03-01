//! This module is responsible for loading the main configuration from the
//! environment and the file system and make it accessible to the rest of the
//! program.

use std::path::PathBuf;
use std::sync::LazyLock;

use config::{Config, Environment, File};
use directories::ProjectDirs;
use serde::Deserialize;
use serde_inline_default::serde_inline_default;

/// The project directories.
///
/// This is only used to load the configuration file and find the default data
/// folder.
static PROJECT_DIRS: LazyLock<ProjectDirs> = LazyLock::new(|| {
    #[expect(clippy::expect_used, reason = "we want to crash if this fails")]
    ProjectDirs::from("fr", "", "Solipr").expect("cannot find home directory")
});

/// The main configuration of Solipr.
pub static CONFIG: LazyLock<SoliprConfig> = LazyLock::new(|| {
    #[expect(clippy::expect_used, reason = "we want to crash if this fails")]
    Config::builder()
        .add_source(File::from(PROJECT_DIRS.config_dir().join("global")).required(false))
        .add_source(Environment::with_prefix("SOLIPR"))
        .build()
        .expect("cannot load config")
        .try_deserialize()
        .expect("cannot deserialize config")
});

/// The main configuration of Solipr.
#[serde_inline_default]
#[derive(Deserialize)]
pub struct SoliprConfig {
    /// The path of the folder in which Solipr stores its data.
    #[serde_inline_default(PROJECT_DIRS.data_dir().to_owned())]
    pub data_folder: PathBuf,
}
