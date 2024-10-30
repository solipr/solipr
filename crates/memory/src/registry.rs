//! Defines a memory based [Registry].

use std::collections::HashMap;
use std::io::{self, Cursor, Read};
use std::sync::{Arc, RwLock};

use sha2::{Digest, Sha256};
use solipr_core::registry::{ContentHash, Registry};

/// A memory based [Registry].
#[derive(Default)]
pub struct MemoryRegistry {
    /// The contents stored in the registry.
    contents: RwLock<HashMap<ContentHash, Arc<[u8]>>>,
}

impl MemoryRegistry {
    /// Creates a new [`MemoryRegistry`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Registry for MemoryRegistry {
    type Error = io::Error;

    fn read(&self, hash: ContentHash) -> Result<Option<impl Read>, Self::Error> {
        let Ok(data) = self.contents.read() else {
            return Err(io::Error::other("failed to read content".to_owned()));
        };
        let Some(content) = data.get(&hash) else {
            return Ok(None);
        };
        Ok(Some(Cursor::new(Arc::clone(content))))
    }

    fn write(&self, mut content: impl Read) -> Result<ContentHash, Self::Error> {
        // Read the content into memory
        let mut buffer = Vec::new();
        content.read_to_end(&mut buffer)?;

        // Create a unique hash for the content
        let mut hasher = Sha256::new();
        hasher.update(&buffer);
        let hash = hasher.finalize().into();

        // Write the content into the registry
        let Ok(mut data) = self.contents.write() else {
            return Err(io::Error::other("failed to write content".to_owned()));
        };
        data.insert(ContentHash::new(hash), buffer.into());

        // Return the hash of the content
        Ok(ContentHash::new(hash))
    }
}

#[cfg(test)]
mod tests {
    use solipr_core::registry::tests::*;

    use super::MemoryRegistry;

    #[test]
    fn read_a_written_value_from_memory() {
        read_a_written_value(&MemoryRegistry::new());
    }

    #[test]
    fn read_a_non_written_value_from_memory() {
        read_a_non_written_value(&MemoryRegistry::new());
    }
}
