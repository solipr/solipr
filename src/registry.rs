//! A registry that can be used so store and retrieve bytes arrays of any
//! length.

use std::error::Error;
use std::fmt::{self, Debug, Display};
use std::io::Read;

use base64::prelude::*;
use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

pub mod memory;
pub mod persistent;

/// The hash of a content stored in the registry.
#[derive(Clone, Copy, Eq, Hash, PartialEq, BorshDeserialize, BorshSerialize)]
pub struct ContentHash([u8; 32]);

impl<T: AsRef<[u8]>> From<T> for ContentHash {
    fn from(value: T) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(value);
        Self(hasher.finalize().into())
    }
}

impl Debug for ContentHash {
    #[expect(clippy::min_ident_chars, reason = "The trait is made that way")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ContentHash")
            .field(&format_args!("{}", BASE64_URL_SAFE_NO_PAD.encode(self.0)))
            .finish()
    }
}

impl Display for ContentHash {
    #[expect(clippy::min_ident_chars, reason = "The trait is made that way")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "content:{}", BASE64_URL_SAFE_NO_PAD.encode(self.0))
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

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "the tests do not need error handling")]
mod tests {
    use std::io::{Cursor, Read};

    use super::*;

    pub fn read_a_written_value(registry: &impl Registry) {
        let content_1 = b"hello";
        let content_2 = b"world";

        let hash_1 = registry.write(Cursor::new(content_1)).unwrap();
        let hash_2 = registry.write(Cursor::new(content_2)).unwrap();

        assert_eq!(
            hash_1.to_string(),
            "content:LPJNul-wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ"
        );
        assert_eq!(
            hash_2.to_string(),
            "content:SG6kYiTRu0-2gPNPfJrZao8k7Ii-c-qOWmxlJg6cuKc"
        );

        let mut read_content_1 = registry.read(hash_1).unwrap().unwrap();
        let mut buffer_1 = Vec::new();
        read_content_1.read_to_end(&mut buffer_1).unwrap();

        let mut read_content_2 = registry.read(hash_2).unwrap().unwrap();
        let mut buffer_2 = Vec::new();
        read_content_2.read_to_end(&mut buffer_2).unwrap();

        assert_eq!(buffer_1, content_1);
        assert_eq!(buffer_2, content_2);
    }

    pub fn read_a_non_written_value(registry: &impl Registry) {
        let random_hash = ContentHash([0; 32]);

        let read_content = registry.read(random_hash).unwrap();

        assert!(read_content.is_none());
    }
}
