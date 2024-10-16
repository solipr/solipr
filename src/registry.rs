//! A registry that can be used so store and retrieve bytes arrays of any
//! length.

use core::error::Error;
use std::io::Read;

use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

/// The hash of a content stored in the registry.
#[derive(Clone, Copy, Eq, Hash, PartialEq, BorshDeserialize, BorshSerialize)]
pub struct ContentHash([u8; 32]);

impl From<Sha256> for ContentHash {
    #[inline]
    fn from(hash: Sha256) -> Self {
        Self(hash.finalize().into())
    }
}

/// A registry that can be used to store and retrieve byte arrays of any length.
pub trait Registry {
    /// The error that can be returned when doing a registry operation.
    type Error: Error;

    /// Returns a [`Read`] to the content with the given hash.
    ///
    /// Returns `None` if the content is not found.
    ///
    /// # Errors
    ///
    /// An error will be returned if the content could not be read.
    fn read(&self, hash: ContentHash) -> Result<Option<impl Read>, Self::Error>;

    /// Writes the given data into the registry and returns the hash of the
    /// written content.
    ///
    /// If the content already exists, nothing will happen and the
    /// [`ContentHash`] will still be returned.
    ///
    /// # Errors
    ///
    /// An error will be returned if the content could not be written.
    fn write(&self, content: impl Read) -> Result<ContentHash, Self::Error>;
}
