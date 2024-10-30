//! A registry that can be used so store and retrieve bytes arrays of any
//! length.

use std::error::Error;
use std::fmt::{self, Debug, Display};
use std::io::Read;
use std::str::FromStr;

use base64::prelude::*;
use borsh::{BorshDeserialize, BorshSerialize};

/// The hash of a content stored in the registry.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, BorshDeserialize, BorshSerialize)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Creates a new content hash from raw bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the raw bytes of the hash.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Debug for ContentHash {
    #[expect(clippy::min_ident_chars, reason = "the trait is made that way")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ContentHash")
            .field(&format_args!("{}", BASE64_URL_SAFE_NO_PAD.encode(self.0)))
            .finish()
    }
}

impl Display for ContentHash {
    #[expect(clippy::min_ident_chars, reason = "the trait is made that way")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "content:{}", BASE64_URL_SAFE_NO_PAD.encode(self.0))
    }
}

impl FromStr for ContentHash {
    type Err = base64::DecodeSliceError;

    fn from_str(mut value: &str) -> Result<Self, Self::Err> {
        value = value.trim();
        value = value.strip_prefix("content:").unwrap_or(value);
        let mut buffer = [0; 32];
        BASE64_URL_SAFE_NO_PAD.decode_slice(value.as_bytes(), &mut buffer)?;
        Ok(Self(buffer))
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

/// Provides tests for the [`Registry`] trait.
#[cfg(feature = "tests")]
#[expect(
    clippy::unwrap_used,
    clippy::missing_panics_doc,
    reason = "the tests do not need error handling"
)]
pub mod tests {
    use std::io::{Cursor, Read};

    use super::{ContentHash, Registry};

    /// Tests that reading and writing a content works.
    pub fn read_a_written_value(registry: &impl Registry) {
        let content_1 = b"hello";
        let content_2 = b"world";

        let hash_1 = registry.write(Cursor::new(content_1)).unwrap();
        let hash_2 = registry.write(Cursor::new(content_2)).unwrap();

        assert_eq!(
            hash_1.to_string(),
            "content:LPJNul-wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ",
            "the hash should be a sha-256 hash of the content"
        );
        assert_eq!(
            hash_2.to_string(),
            "content:SG6kYiTRu0-2gPNPfJrZao8k7Ii-c-qOWmxlJg6cuKc",
            "the hash should be a sha-256 hash of the content"
        );

        let mut read_content_1 = registry.read(hash_1).unwrap().unwrap();
        let mut buffer_1 = Vec::new();
        read_content_1.read_to_end(&mut buffer_1).unwrap();

        let mut read_content_2 = registry.read(hash_2).unwrap().unwrap();
        let mut buffer_2 = Vec::new();
        read_content_2.read_to_end(&mut buffer_2).unwrap();

        assert_eq!(buffer_1, content_1, "the content should not change");
        assert_eq!(buffer_2, content_2, "the content should not change");
    }

    /// Tests that reading a non-written value returns `None`.
    pub fn read_a_non_written_value(registry: &impl Registry) {
        let random_hash = ContentHash([0; 32]);

        let read_content = registry.read(random_hash).unwrap();

        assert!(read_content.is_none(), "the content should not be found");
    }
}
