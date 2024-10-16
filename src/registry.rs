//! A registry that can be used so store and retrieve bytes arrays of any
//! length.

use core::error::Error;
use core::future::Future;

use futures::AsyncRead;

/// The hash of a content stored in the registry.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct ContentHash([u8; 32]);

/// A registry that can be used to store and retrieve byte arrays of any length.
pub trait Registry {
    /// The error that can be returned when doing a registry operation.
    type Error: Error;

    /// Returns an [`AsyncRead`] to the content with the given hash.
    ///
    /// Returns `None` if the content is not found.
    ///
    /// # Errors
    ///
    /// An error will be returned if the content could not be read.
    fn read(
        &self,
        hash: ContentHash,
    ) -> impl Future<Output = Result<Option<impl AsyncRead + Send>, Self::Error>> + Send;

    /// Writes the given data into the registry and returns the hash of the
    /// written content.
    ///
    /// If the content already exists, nothing will happen and the
    /// [`ContentHash`] will still be returned.
    ///
    /// # Errors
    ///
    /// An error will be returned if the content could not be written.
    fn write(
        &self,
        content: impl AsyncRead + Send,
    ) -> impl Future<Output = Result<ContentHash, Self::Error>> + Send;
}
