//! A registry that can be used so store and retrieve bytes arrays of any
//! length.

use core::error::Error;

use async_trait::async_trait;
use futures::{AsyncRead, Stream};

/// The hash of a content stored in the registry.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct ContentHash([u8; 32]);

/// The tag of a content stored in the registry.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct ContentTag([u8; 32]);

impl<T: ToString> From<T> for ContentTag {
    #[must_use]
    #[inline]
    fn from(value: T) -> Self {
        let mut bytes = [0_u8; 32];
        #[expect(clippy::indexing_slicing, reason = "we only take the first 32 bytes")]
        for (i, byte) in value.to_string().bytes().take(32).enumerate() {
            bytes[i] = byte;
        }
        Self(bytes)
    }
}

/// A registry that can be used to store simple byte arrays of any length
/// locally.
#[async_trait]
pub trait Registry {
    /// The error type of the registry.
    type Error: Error;

    /// Returns a [Stream] of all known tags in the registry.
    async fn known_tags(&self) -> Result<impl Stream<Item = ContentTag>, Self::Error>;

    /// Returns a [Stream] of all known hashes in the registry that match the
    /// given tags.
    async fn list(
        &self,
        tags: impl IntoIterator<Item = ContentTag>,
    ) -> Result<impl Stream<Item = ContentHash>, Self::Error>;

    /// Returns whether a content with the given hash has the given tags in the
    /// registry.
    async fn has(
        &self,
        hash: ContentHash,
        tags: impl IntoIterator<Item = ContentTag>,
    ) -> Result<bool, Self::Error>;

    /// Returns the tags of a content with the given hash in the registry.
    async fn tags(&self, hash: ContentHash) -> Result<impl Stream<Item = ContentTag>, Self::Error>;

    /// Returns an [`AsyncRead`] of the content of the content with the given
    /// hash in the registry.
    async fn read(&self, hash: ContentHash) -> Result<Option<impl AsyncRead>, Self::Error>;

    /// Writes the given data into the registry with the given tags.
    ///
    /// Returns the hash of the written content.
    async fn write(
        &self,
        data: impl AsyncRead,
        tags: impl IntoIterator<Item = ContentTag>,
    ) -> Result<ContentHash, Self::Error>;

    /// Adds the given tags to the content with the given hash in the registry.
    async fn add_tags(
        &self,
        hash: ContentHash,
        tags: impl IntoIterator<Item = ContentTag>,
    ) -> Result<ContentHash, Self::Error>;

    /// Removes the given tags from the content with the given hash in the
    /// registry.
    ///
    /// If the content does not have the given tags or if it does not exist,
    /// this function does nothing.
    async fn remove_tags(
        &self,
        hash: ContentHash,
        tags: impl IntoIterator<Item = ContentTag>,
    ) -> Result<(), Self::Error>;
}
