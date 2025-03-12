//! The storage system for Solipr.

use std::fs::{self, File};
use std::io::{self, Read, Seek, Write};
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::identifier::ContentHash;

#[derive(Clone)]
/// A registry that can be used to store and retrieve byte arrays of any length.
pub struct Registry {
    /// The path to the folder where the contents are stored.
    folder: PathBuf,
}

impl Registry {
    /// Opens a folder as a [Registry].
    pub fn open(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
        }
    }

    /// Returns a [Read] to the content with the given hash.
    ///
    /// Returns `None` if the content is not found.
    ///
    /// # Errors
    ///
    /// An error will be returned if the content could not be read.
    pub fn read(&self, hash: ContentHash) -> anyhow::Result<Option<impl Read + Seek>> {
        let encoded_hash = bs58::encode(hash.as_bytes()).into_string();
        match File::open(
            self.folder
                .join(&encoded_hash[..2])
                .join(&encoded_hash[2..]),
        ) {
            Ok(file) => Ok(Some(file)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Returns the size of the content with the given hash.
    ///
    /// Returns `None` if the content is not found.
    ///
    /// # Errors
    ///
    /// An error will be returned if the size could not be read.
    pub fn size(&self, hash: ContentHash) -> anyhow::Result<Option<u64>> {
        let encoded_hash = bs58::encode(hash.as_bytes()).into_string();
        let folder = self
            .folder
            .join(&encoded_hash[..2])
            .join(&encoded_hash[2..]);
        match folder.metadata() {
            Ok(metadata) => Ok(Some(metadata.len())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Writes the given data into the registry and returns the hash of the
    /// written content.
    ///
    /// If the content already exists, nothing will happen and the
    /// [`ContentHash`] will still be returned.
    ///
    /// # Errors
    ///
    /// An error will be returned if the content could not be written.
    pub fn write(&self, mut content: impl Read) -> anyhow::Result<ContentHash> {
        // Create the folder if it doesn't exist
        if !self.folder.exists() {
            fs::create_dir_all(&self.folder)?;
        }

        // Create a temporary file to store the content in
        let temp_file_path = self.folder.join(Uuid::now_v7().to_string());
        let mut temp_file = File::create(&temp_file_path)?;

        // Loop 32 bytes at a time and update the hasher
        // until we reach the end of the content
        let mut hasher = Sha256::new();
        let mut buffer = [0; 32];
        loop {
            let byte_count = match content.read(&mut buffer) {
                Ok(0) => break,
                Ok(byte_count) => byte_count,
                Err(ref error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            };
            #[expect(
                clippy::indexing_slicing,
                reason = "a call to read can't return a length bigger than the buffer size"
            )]
            {
                hasher.update(&buffer[..byte_count]);
                temp_file.write_all(&buffer[..byte_count])?;
            }
        }
        temp_file.flush()?;
        drop(temp_file);

        // Create a unique hash for the content
        let hash = ContentHash(hasher.finalize().into());
        let encoded_hash = bs58::encode(hash.as_bytes()).into_string();

        // Move the temporary file into the correct location
        let (subfolder, file) = encoded_hash.split_at(2);
        let path_dir = self.folder.join(subfolder);
        if !path_dir.exists() {
            fs::create_dir(&path_dir)?;
        }
        fs::rename(temp_file_path, path_dir.join(file))?;

        // Return the hash of the content
        Ok(hash)
    }
}
