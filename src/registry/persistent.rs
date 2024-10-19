//! Defines a persistent implementation of [Registry].

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::PathBuf;

use base64::prelude::*;
use sha2::{Digest, Sha256};

use super::{ContentHash, Registry};

/// A persistent implementation of [Registry].
pub struct PersistentRegistry {
    /// The path to the folder where the contents are stored.
    folder: PathBuf,
}

impl Registry for PersistentRegistry {
    type Error = io::Error;

    fn read(&self, hash: ContentHash) -> Result<Option<impl Read>, Self::Error> {
        let encoded_hash = BASE64_URL_SAFE_NO_PAD.encode(hash.0);
        #[expect(
            clippy::string_slice,
            reason = "there is always more than 2 characters in a ContentHash"
        )]
        match File::open(
            self.folder
                .join(&encoded_hash[..2])
                .join(&encoded_hash[2..]),
        ) {
            Ok(file) => Ok(Some(file)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn write(&self, mut content: impl Read) -> Result<ContentHash, Self::Error> {
        // Create a temporary file to store the content in.
        let temp_file_path = self.folder.join(uuid::Uuid::now_v7().to_string());
        let mut temp_file = File::create(self.folder.join(&temp_file_path))?;

        // Loop 32 bytes at a time and update the hasher
        // until we reach the end of the content
        let mut hasher = Sha256::new();
        let mut buffer = [0; 32];
        loop {
            let byte_count = match content.read(&mut buffer) {
                Ok(0) => break,
                Ok(byte_count) => byte_count,
                Err(ref err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err),
            };
            #[expect(
                clippy::indexing_slicing,
                reason = "byte_count is always smaller or equal than 32"
            )]
            hasher.update(&buffer[..byte_count]);
            temp_file.write_all(&buffer)?;
        }
        temp_file.flush()?;

        // Create a unique hash for the content
        let hash = ContentHash(hasher.finalize().into());
        let encoded_hash = BASE64_URL_SAFE_NO_PAD.encode(hash.0);

        // Move the temporary file into the correct location
        fs::rename(
            temp_file_path,
            #[expect(
                clippy::string_slice,
                reason = "there is always more than 2 characters in 32 bytes encoded in base64"
            )]
            self.folder
                .join(&encoded_hash[..2])
                .join(&encoded_hash[2..]),
        )?;

        // Return the hash of the content
        Ok(hash)
    }
}
