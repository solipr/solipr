//! Defines a persistent memory based [Registry].

use std::fs::File;
use std::io::{self, Read, Write};

use sha2::{Digest, Sha256};

use super::{ContentHash, Registry};

/// A persistent memory based [Registry].
pub struct PersistentRegistry {
    /// The path to the folder where the [Registry] is stored.
    path: String,
}

impl Registry for PersistentRegistry {
    type Error = io::Error;

    fn read(&self, hash: ContentHash) -> Result<Option<impl Read>, Self::Error> {
        match File::open(format!("{}/{}", self.path, hash)) {
            Ok(file) => Ok(Some(file)),
            Err(err) => Err(err),
        }
    }

    fn write(&self, mut content: impl Read) -> Result<ContentHash, Self::Error> {
        let mut hasher = Sha256::new();

        // Create a temporary file to store the content in.
        let mut temp_file = File::create(format!("{}/{}", self.path, "temp"))?;

        // Loop 32 bytes at a time.
        loop {
            let mut buffer = [0; 32];

            match content.read(&mut buffer) {
                Ok(0) => break,
                Err(err) => {
                    return Err(err);
                }
                _ => {}
            }
            hasher.update(buffer);
            temp_file.write_all(&buffer)?;
        }

        let hash = ContentHash(hasher.finalize().into());

        let mut file = File::create(format!("{}/{}", self.path, hash))?;
        io::copy(&mut temp_file, &mut file)?;

        Ok(hash)
    }
}
