//! The plugin system of Solipr.

#[cfg(feature = "host")]
pub mod host;

#[cfg(feature = "guest")]
pub mod guest;
