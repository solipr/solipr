//! Defines a persistent memory based [Registry].

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::PathBuf;

use base64::prelude::*;
use sha2::{Digest, Sha256};

use super::{ContentHash, Registry};

/// A persistent memory based [Registry].
pub struct PersistentRegistry {
    /// The path to the folder where the [Registry] is stored.
    path: PathBuf,
}

impl Registry for PersistentRegistry {
    type Error = io::Error;

    fn read(&self, hash: ContentHash) -> Result<Option<impl Read>, Self::Error> {
        let encore_hash = BASE64_URL_SAFE_NO_PAD.encode(hash.0);

        if encore_hash.len() < 2 {
            return Err(io::Error::other("invalid content hash".to_owned()));
        }
        #[expect(clippy::string_slice, reason = "The length is checked above")]
        match File::open(self.path.join(&encore_hash[0..2]).join(&encore_hash[2..])) {
            Ok(file) => Ok(Some(file)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn write(&self, mut content: impl Read) -> Result<ContentHash, Self::Error> {
        let mut hasher = Sha256::new();

        // Create a temporary file to store the content in.
        let temp_file_path = self.path.join(uuid::Uuid::now_v7().to_string());
        let mut temp_file = File::create(self.path.join(temp_file_path.clone()))?;

        // Loop 32 bytes at a time.
        loop {
            let mut buffer = [0; 32];

            match content.read(&mut buffer) {
                Ok(0) => break,
                Err(ref err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) => {
                    return Err(err);
                }
                _ => {}
            }
            hasher.update(buffer);
            temp_file.write_all(&buffer)?;
        }

        let hash = ContentHash(hasher.finalize().into());
        let encoded_hash = BASE64_URL_SAFE_NO_PAD.encode(hash.0);

        fs::rename(
            temp_file_path,
            #[expect(
                clippy::string_slice,
                reason = "There is always more than 2 characters in 32 bytes in base 64"
            )]
            self.path.join(&encoded_hash[0..2]).join(&encoded_hash[2..]),
        )?;

        Ok(hash)
    }
}
