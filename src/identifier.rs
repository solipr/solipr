//! This module contains all the identifier types and utilities used in Solipr.

use std::fmt::{self, Debug, Display};
use std::hash::Hash;
use std::str::FromStr;

use anyhow::Context;

/// The hash of a content stored in a [Registry](crate::storage::Registry).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash(pub(crate) [u8; 32]);

impl ContentHash {
    /// Returns the raw bytes of the hash.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C{}", bs58::encode(self.0).into_string())
    }
}

impl Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C{}", bs58::encode(self.0).into_string())
    }
}

impl FromStr for ContentHash {
    type Err = anyhow::Error;

    fn from_str(mut value: &str) -> Result<Self, Self::Err> {
        value = value.trim().strip_prefix("C").context("missing prefix")?;
        Ok(Self(bs58::decode(value.as_bytes()).into_array_const()?))
    }
}
