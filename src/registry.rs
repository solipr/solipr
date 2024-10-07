//! A registry that can be used so store and retrieve bytes arrays of any
//! length.

use std::io::{self, Read};

use crate::stack;

/// A byte array that is stored directly if it is small enough to fit, otherwise
/// it is stored in a registry and only the data hash is stored.
pub enum Content {
    /// If the content is small enough it is stored in a [`stack::Vec`].
    Direct(stack::Vec<u8, 127>),

    /// If the content is too large it is stored in a registry and we store the
    /// hash of the data.
    Registry([u8; 32]),
}

/// A registry that can be used to store simple byte arrays of any length.
pub trait Registry {
    /// Returns a [Read] over a content stored in the registry.
    ///
    /// Returns `None` if the content is not stored in the registry.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if the content can't be opened.
    fn read(&self, hash: [u8; 32]) -> io::Result<Option<impl Read>>;

    /// Write a new content to the registry.
    ///
    /// If the content is already in the registry, this function does nothing.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if there is an error while writing to the
    /// content to the registry.
    fn write(&self, content: impl Read) -> io::Result<()>;
}
