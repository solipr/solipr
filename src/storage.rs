//! The storage system for Solipr.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use fjall::{Config, PartitionCreateOptions, TransactionalKeyspace, TransactionalPartitionHandle};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wasmtime::component::Resource;

use crate::identifier::ContentHash;
use crate::plugin::{HostReadRegistry, HostWriteRegistry, ReadRegistry, WriteRegistry};

/// A database that can be used to store and retrieve bytes using transactions.
pub struct Database {
    /// The underlying [fjall] keyspace.
    keyspace: TransactionalKeyspace,

    /// The underlying [fjall] partition where all data is stored.
    partition: TransactionalPartitionHandle,
}

impl Database {
    /// Open a [Database] in the given folder.
    ///
    /// # Errors
    ///
    /// An error is returned if there is an IO error while opening the folder or
    /// if the database is in an invalid state.
    pub fn open(folder: impl AsRef<Path>) -> anyhow::Result<Self> {
        let keyspace = Config::new(folder).open_transactional()?;
        let partition = keyspace.open_partition("data", PartitionCreateOptions::default())?;
        Ok(Self {
            keyspace,
            partition,
        })
    }

    /// Open a new read-only transaction.
    ///
    /// There can be multiple read-only transactions open at the same time.
    ///
    /// # Errors
    ///
    /// This method return an error if there is an fatal error that can't be
    /// recovered.
    pub fn read_tx(&self) -> anyhow::Result<ReadTransaction> {
        Ok(ReadTransaction {
            partition: &self.partition,
            tx: self.keyspace.read_tx(),
        })
    }

    /// Open a new transaction that can both read and write data to the
    /// database.
    ///
    /// There can be only one write transaction open at a time. If a write
    /// transaction is already open, then this function will block until it
    /// has been closed before opening a new one.
    ///
    /// # Errors
    ///
    /// This method return an error if there is an fatal error that can't be
    /// recovered.
    pub fn write_tx(&self) -> anyhow::Result<WriteTransaction> {
        Ok(WriteTransaction {
            partition: &self.partition,
            tx: self.keyspace.write_tx(),
        })
    }
}

/// A read-only transaction on a [Database].
pub struct ReadTransaction<'db> {
    /// The partition that the transaction is operating on.
    partition: &'db TransactionalPartitionHandle,

    /// The underlying [fjall] transaction.
    tx: fjall::ReadTransaction,
}

impl ReadTransaction<'_> {
    /// Returns an [Iterator] over the keys starting by the given prefix with
    /// their values.
    ///
    /// # Errors
    ///
    /// The iterator should return an error if there is an fatal error that
    /// can't be recovered.
    pub fn keys(
        &self,
        prefix: impl AsRef<[u8]>,
    ) -> impl Iterator<Item = anyhow::Result<(Slice, Slice)>> {
        let prefix = prefix.as_ref().to_vec();
        self.tx.prefix(self.partition, prefix).map(|item| {
            item.map(|(key, value)| (Slice(key, PhantomData), Slice(value, PhantomData)))
                .map_err(|error| anyhow::anyhow!(error))
        })
    }

    /// Get a value from the database.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn get(&self, key: impl AsRef<[u8]>) -> anyhow::Result<Option<Slice<'_>>> {
        Ok(self
            .tx
            .get(self.partition, key.as_ref())?
            .map(|slice| Slice(slice, PhantomData)))
    }
}

/// A read-write transaction on a [Database].
pub struct WriteTransaction<'db> {
    /// The partition that the transaction is operating on.
    partition: &'db TransactionalPartitionHandle,

    /// The underlying [fjall] transaction.
    tx: fjall::WriteTransaction<'db>,
}

impl WriteTransaction<'_> {
    /// Returns an [Iterator] over the keys starting by the given prefix with
    /// their values.
    ///
    /// # Errors
    ///
    /// The iterator should return an error if there is an fatal error that
    /// can't be recovered.
    pub fn keys(
        &self,
        prefix: impl AsRef<[u8]>,
    ) -> impl Iterator<Item = anyhow::Result<(Slice, Slice)>> {
        let prefix = prefix.as_ref().to_vec();
        self.tx.prefix(self.partition, prefix).map(|item| {
            item.map(|(key, value)| (Slice(key, PhantomData), Slice(value, PhantomData)))
                .map_err(|error| anyhow::anyhow!(error))
        })
    }

    /// Get a value from the database.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn get(&self, key: impl AsRef<[u8]>) -> anyhow::Result<Option<Slice<'_>>> {
        Ok(self
            .tx
            .get(self.partition, key.as_ref())?
            .map(|slice| Slice(slice, PhantomData)))
    }

    /// Put a value in the database. If there is already a value for this key,
    /// it will be overwritten.
    ///
    /// If the `value` is `None`, this will remove the existing value for the
    /// key.
    ///
    /// This method will return an error if the transaction is read-only.
    ///
    /// # Errors
    ///
    /// This method should return an error if the transaction is read-only or if
    /// there is an fatal error that can't be recovered.
    pub fn put(
        &mut self,
        key: impl AsRef<[u8]>,
        value: Option<impl AsRef<[u8]>>,
    ) -> anyhow::Result<()> {
        if let Some(value) = value {
            self.tx.insert(self.partition, key.as_ref(), value.as_ref());
        } else {
            self.tx.remove(self.partition, key.as_ref());
        }
        Ok(())
    }

    /// Commit the transaction to the database.
    ///
    /// This method will apply all changes made in this transaction to the
    /// database in a single operation.
    ///
    /// This method will return an error if the transaction is read-only.
    ///
    /// # Errors
    ///
    /// This method should return an error if the transaction is read-only or if
    /// there is an fatal error that can't be recovered.
    pub fn commit(self) -> anyhow::Result<()> {
        self.tx.commit()?;
        Ok(())
    }
}

/// A slice of bytes given by a [Database].
///
/// This trait enables the [Database] implementation to perform additional
/// actions when a retrieved value is dropped. It is also useful for avoiding
/// the need to clone the data from the [Database].
pub struct Slice<'tx>(fjall::Slice, PhantomData<&'tx ()>);

impl Deref for Slice<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

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

impl HostReadRegistry for Registry {
    fn read(
        &mut self,
        _: Resource<ReadRegistry>,
        content_hash: String,
        start: u64,
        length: Option<u64>,
    ) -> wasmtime::Result<Option<Vec<u8>>> {
        let encoded_hash =
            bs58::encode(ContentHash::from_str(&content_hash)?.as_bytes()).into_string();
        let file = match File::open(
            self.folder
                .join(&encoded_hash[..2])
                .join(&encoded_hash[2..]),
        ) {
            Ok(file) => Ok::<_, anyhow::Error>(Some(file)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }?;
        match file {
            Some(mut file) => {
                file.seek(SeekFrom::Start(start))?;
                if let Some(length) = length {
                    let mut buffer = Vec::with_capacity(length.try_into()?);
                    file.take(length).read_to_end(&mut buffer)?;
                    Ok(Some(buffer))
                } else {
                    let mut buffer = Vec::with_capacity(
                        file.metadata()?.len().saturating_sub(start).try_into()?,
                    );
                    file.read_to_end(&mut buffer)?;
                    Ok(Some(buffer))
                }
            }
            None => Ok(None),
        }
    }

    fn size(
        &mut self,
        _: Resource<ReadRegistry>,
        content_hash: String,
    ) -> wasmtime::Result<Option<u64>> {
        Self::size(self, ContentHash::from_str(&content_hash)?)
    }

    fn drop(&mut self, _: Resource<ReadRegistry>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl HostWriteRegistry for Registry {
    fn read(
        &mut self,
        _: Resource<WriteRegistry>,
        content_hash: String,
        start: u64,
        length: Option<u64>,
    ) -> wasmtime::Result<Option<Vec<u8>>> {
        let encoded_hash =
            bs58::encode(ContentHash::from_str(&content_hash)?.as_bytes()).into_string();
        let file = match File::open(
            self.folder
                .join(&encoded_hash[..2])
                .join(&encoded_hash[2..]),
        ) {
            Ok(file) => Ok::<_, anyhow::Error>(Some(file)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }?;
        match file {
            Some(mut file) => {
                file.seek(SeekFrom::Start(start))?;
                if let Some(length) = length {
                    let mut buffer = Vec::with_capacity(length.try_into()?);
                    file.take(length).read_to_end(&mut buffer)?;
                    Ok(Some(buffer))
                } else {
                    let mut buffer = Vec::with_capacity(
                        file.metadata()?.len().saturating_sub(start).try_into()?,
                    );
                    file.read_to_end(&mut buffer)?;
                    Ok(Some(buffer))
                }
            }
            None => Ok(None),
        }
    }

    fn size(
        &mut self,
        _: Resource<WriteRegistry>,
        content_hash: String,
    ) -> wasmtime::Result<Option<u64>> {
        Self::size(self, ContentHash::from_str(&content_hash)?)
    }

    fn write(&mut self, _: Resource<WriteRegistry>, data: Vec<u8>) -> wasmtime::Result<String> {
        Self::write(self, &data[..]).map(|content_hash| content_hash.to_string())
    }

    fn cut(
        &mut self,
        _: Resource<WriteRegistry>,
        content_hash: String,
        start: u64,
        length: Option<u64>,
    ) -> wasmtime::Result<Option<String>> {
        let data = Self::read(self, ContentHash::from_str(&content_hash)?)?;
        match data {
            Some(mut data) => {
                data.seek(SeekFrom::Start(start))?;
                let content_hash = if let Some(length) = length {
                    Self::write(self, data.take(length))
                } else {
                    Self::write(self, data)
                }?;
                Ok(Some(content_hash.to_string()))
            }
            None => Ok(None),
        }
    }

    fn drop(&mut self, _: Resource<WriteRegistry>) -> wasmtime::Result<()> {
        Ok(())
    }
}
