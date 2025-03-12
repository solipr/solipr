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

impl From<ChangeHash> for ContentHash {
    fn from(value: ChangeHash) -> Self {
        Self(value.0)
    }
}

impl From<PluginHash> for ContentHash {
    fn from(value: PluginHash) -> Self {
        Self(value.0)
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

/// The hash of a document plugin.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, BorshDeserialize, BorshSerialize)]
pub struct PluginHash(pub(crate) [u8; 32]);

impl PluginHash {
    /// Returns the raw bytes of the hash.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Debug for PluginHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "P{}", bs58::encode(self.0).into_string())
    }
}

impl Display for PluginHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "P{}", bs58::encode(self.0).into_string())
    }
}

impl FromStr for PluginHash {
    type Err = anyhow::Error;

    fn from_str(mut value: &str) -> Result<Self, Self::Err> {
        value = value.trim().strip_prefix("P").context("missing prefix")?;
        Ok(Self(bs58::decode(value.as_bytes()).into_array_const()?))
    }
}

impl From<ContentHash> for PluginHash {
    fn from(hash: ContentHash) -> Self {
        Self(hash.0)
    }
}

/// The identifier of a document in a
/// [Repository](crate::repository::Repository).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, BorshDeserialize, BorshSerialize)]
pub struct DocumentId(pub(crate) PluginHash, pub(crate) Uuid);

impl DocumentId {
    /// Creates a new [`DocumentId`] from a [`PluginHash`].
    #[must_use]
    pub fn new(plugin_hash: PluginHash) -> Self {
        Self(plugin_hash, Uuid::now_v7())
    }

    /// Returns the [`PluginHash`] of the plugin that created the document.
    #[must_use]
    pub const fn plugin_hash(&self) -> PluginHash {
        self.0
    }
}

impl Debug for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut bytes = [0; 48];
        bytes[0..32].copy_from_slice(self.0.as_bytes());
        bytes[32..48].copy_from_slice(self.1.as_bytes());
        write!(f, "D{}", bs58::encode(bytes).into_string())
    }
}

impl Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut bytes = [0; 48];
        bytes[0..32].copy_from_slice(self.0.as_bytes());
        bytes[32..48].copy_from_slice(self.1.as_bytes());
        write!(f, "D{}", bs58::encode(bytes).into_string())
    }
}

impl FromStr for DocumentId {
    type Err = anyhow::Error;

    fn from_str(mut value: &str) -> Result<Self, Self::Err> {
        value = value.trim().strip_prefix("D").context("missing prefix")?;
        let bytes: [u8; 48] = bs58::decode(value.as_bytes()).into_array_const()?;
        Ok(Self(
            PluginHash(bytes[0..32].try_into()?),
            Uuid::from_bytes(bytes[32..48].try_into()?),
        ))
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

impl From<ContentHash> for ChangeHash {
    fn from(hash: ContentHash) -> Self {
        Self(hash.0)
    }
}
