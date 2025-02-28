//! The storage system for Solipr.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::marker::PhantomData;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use anyhow::bail;
use fjall::{
    Config, PartitionCreateOptions, ReadTransaction, TransactionalKeyspace,
    TransactionalPartitionHandle, WriteTransaction,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// A database that can be used to store and retrieve bytes using transactions.
pub struct Database {
    /// The underlying [fjall] keyspace.
    keyspace: TransactionalKeyspace,

    /// The underlying [fjall] partition where all data is stored.
    partition: TransactionalPartitionHandle,
}

impl Database {
    /// Open a [`SoliprDb`] in the given folder.
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
    pub fn read_tx(&self) -> anyhow::Result<Transaction> {
        Ok(Transaction {
            partition: &self.partition,
            tx: InnerTx::Read(self.keyspace.read_tx()),
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
    pub fn write_tx(&self) -> anyhow::Result<Transaction> {
        Ok(Transaction {
            partition: &self.partition,
            tx: InnerTx::Write(self.keyspace.write_tx()),
        })
    }
}

/// A transaction on a [Database].
///
/// The tranaction can be read-only or read-write dpending on whether it is
/// opened using [`Database::read_tx`] or [`Database::write_tx`].
pub struct Transaction<'db> {
    /// The partition that the transaction is operating on.
    partition: &'db TransactionalPartitionHandle,

    /// The underlying [fjall] transaction.
    tx: InnerTx<'db>,
}

/// Since [fjall] use two different types of transactions, we need to use an
/// enum to represent the different types of transactions. This enum serves that
/// purpose.
enum InnerTx<'db> {
    /// The read-only version of the transaction.
    Read(ReadTransaction),

    /// The write version of the transaction.
    Write(WriteTransaction<'db>),
}

impl Transaction<'_> {
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
        let iter: Box<dyn Iterator<Item = _>> = match &self.tx {
            InnerTx::Read(tx) => Box::new(tx.prefix(self.partition, prefix)),
            InnerTx::Write(tx) => Box::new(tx.prefix(self.partition, prefix)),
        };
        iter.map(|item| {
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
        let slice = match &self.tx {
            InnerTx::Read(tx) => tx.get(self.partition, key.as_ref())?,
            InnerTx::Write(tx) => tx.get(self.partition, key.as_ref())?,
        };
        Ok(slice.map(|slice| Slice(slice, PhantomData)))
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
        match &mut self.tx {
            InnerTx::Read(_) => {
                bail!("cannot put into a read only transaction")
            }
            InnerTx::Write(tx) => {
                if let Some(value) = value {
                    tx.insert(self.partition, key.as_ref(), value.as_ref());
                } else {
                    tx.remove(self.partition, key.as_ref());
                }
            }
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
        match self.tx {
            InnerTx::Read(_) => bail!("cannot commit a read only transaction"),
            InnerTx::Write(tx) => Ok(tx.commit()?),
        }
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

/// The hash of a content stored in the registry.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

/// A registry that can be used to store and retrieve byte arrays of any length.
pub struct Registry {
    /// The path to the folder where the contents are stored.
    folder: PathBuf,
}

impl Registry {
    /// Creates a new [Registry] from the specified folder.
    pub fn new(folder: impl Into<PathBuf>) -> Self {
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
    pub fn read(&self, hash: ContentHash) -> anyhow::Result<Option<impl Read>> {
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
        let hash = ContentHash::new(hasher.finalize().into());
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
