//! The plugin system of Solipr.

#[cfg(feature = "host")]
pub mod host;
#[cfg(feature = "host")]
pub use host::*;

#[cfg(feature = "guest")]
mod guest;
#[cfg(feature = "guest")]
pub use guest::*;
