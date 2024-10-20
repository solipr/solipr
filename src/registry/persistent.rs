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

impl PersistentRegistry {
    /// Creates a new [`PersistentRegistry`] from the specified folder.
    #[must_use]
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
        }
    }
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
        // Create the folder if it doesn't exist
        if !self.folder.exists() {
            fs::create_dir_all(&self.folder)?;
        }

        // Create a temporary file to store the content in.
        let temp_file_path = self.folder.join(uuid::Uuid::now_v7().to_string());
        let mut temp_file = File::create(&temp_file_path)?;

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
            #[expect(
                clippy::indexing_slicing,
                reason = "byte_count is always smaller or equal than 32"
            )]
            temp_file.write_all(&buffer[..byte_count])?;
        }
        temp_file.flush()?;
        drop(temp_file);

        // Create a unique hash for the content
        let hash = ContentHash(hasher.finalize().into());
        let encoded_hash = BASE64_URL_SAFE_NO_PAD.encode(hash.0);

        // Move the temporary file into the correct location
        let (subfolder, file) = encoded_hash.split_at(2);
        let path_dir = self.folder.join(subfolder);
        if !path_dir.exists() {
            fs::create_dir_all(&path_dir)?;
        }
        fs::rename(temp_file_path, path_dir.join(file))?;

        // Return the hash of the content
        Ok(hash)
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "the tests do not need error handling")]
mod tests {
    use tempfile;

    use super::PersistentRegistry;
    use crate::registry::tests::*;

    #[test]
    fn read_a_written_value_from_persistent() {
        let temp_dir = tempfile::tempdir().unwrap().path().to_owned();
        read_a_written_value(&PersistentRegistry::new(temp_dir));
    }

    #[test]
    fn read_a_non_written_value_from_persistent() {
        let temp_dir = tempfile::tempdir().unwrap().path().to_owned();
        read_a_non_written_value(&PersistentRegistry::new(temp_dir));
    }
}
