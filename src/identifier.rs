//! This module contains all the identifier types and utilities used in Solipr.

use std::fmt::{self, Debug, Display};
use std::hash::Hash;
use std::str::FromStr;

use anyhow::Context;
use borsh::{BorshDeserialize, BorshSerialize};
use uuid::Uuid;

/// The hash of a content stored in a [Registry](crate::storage::Registry).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, BorshDeserialize, BorshSerialize)]
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
        write!(f, "B{}", bs58::encode(self.0).into_string())
    }
}

impl Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "B{}", bs58::encode(self.0).into_string())
    }
}

impl FromStr for ContentHash {
    type Err = anyhow::Error;

    fn from_str(mut value: &str) -> Result<Self, Self::Err> {
        value = value.trim().strip_prefix("B").context("missing prefix")?;
        Ok(Self(bs58::decode(value.as_bytes()).into_array_const()?))
    }
}

/// The identifier of a [Repository](crate::repository::Repository).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, BorshDeserialize, BorshSerialize)]
pub struct RepositoryId(Uuid);

impl Debug for RepositoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "R{}", bs58::encode(self.0.as_bytes()).into_string())
    }
}

impl Display for RepositoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "R{}", bs58::encode(self.0.as_bytes()).into_string())
    }
}

impl FromStr for RepositoryId {
    type Err = anyhow::Error;

    fn from_str(mut value: &str) -> Result<Self, Self::Err> {
        value = value.trim().strip_prefix("R").context("missing prefix")?;
        Ok(Self(Uuid::from_bytes(
            bs58::decode(value.as_bytes()).into_array_const()?,
        )))
    }
}

/// The identifier of a document in a
/// [Repository](crate::repository::Repository).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, BorshDeserialize, BorshSerialize)]
pub struct DocumentId(Uuid);

impl Debug for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "D{}", bs58::encode(self.0.as_bytes()).into_string())
    }
}

impl Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "D{}", bs58::encode(self.0.as_bytes()).into_string())
    }
}

impl FromStr for DocumentId {
    type Err = anyhow::Error;

    fn from_str(mut value: &str) -> Result<Self, Self::Err> {
        value = value.trim().strip_prefix("D").context("missing prefix")?;
        Ok(Self(Uuid::from_bytes(
            bs58::decode(value.as_bytes()).into_array_const()?,
        )))
    }
}

/// The hash of a [Change](crate::repository::Change) stored in a document.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, BorshDeserialize, BorshSerialize)]
pub struct ChangeHash(pub(crate) [u8; 32]);

impl ChangeHash {
    /// Returns the raw bytes of the hash.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Debug for ChangeHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C{}", bs58::encode(self.0).into_string())
    }
}

impl Display for ChangeHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C{}", bs58::encode(self.0).into_string())
    }
}

impl FromStr for ChangeHash {
    type Err = anyhow::Error;

    fn from_str(mut value: &str) -> Result<Self, Self::Err> {
        value = value.trim().strip_prefix("C").context("missing prefix")?;
        Ok(Self(bs58::decode(value.as_bytes()).into_array_const()?))
    }
}
