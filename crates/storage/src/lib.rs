//! The storage system for Solipr.

use std::marker::PhantomData;
use std::ops::Deref;
use std::path::Path;
use std::result::Result as StdResult;

use fjall::{
    Config, PartitionCreateOptions, ReadTransaction, TransactionalKeyspace,
    TransactionalPartitionHandle, WriteTransaction,
};

/// The error type for [`Database`] operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The underlying [fjall] database returned an error.
    #[error("fjall error: {0}")]
    Fjall(#[from] fjall::Error),

    /// The user tried to write to a read-only transaction.
    #[error("write on a read-only transaction")]
    ReadOnlyTransaction,
}

/// The result type for [`Database`] operations.
type Result<T> = StdResult<T, Error>;

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
    pub fn open(folder: impl AsRef<Path>) -> Result<Self> {
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
    pub fn read_tx(&self) -> Result<Transaction> {
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
    pub fn write_tx(&self) -> Result<Transaction> {
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
    pub fn keys(&self, prefix: impl AsRef<[u8]>) -> impl Iterator<Item = Result<(Slice, Slice)>> {
        let prefix = prefix.as_ref().to_vec();
        let iter: Box<dyn Iterator<Item = _>> = match &self.tx {
            InnerTx::Read(tx) => Box::new(tx.prefix(self.partition, prefix)),
            InnerTx::Write(tx) => Box::new(tx.prefix(self.partition, prefix)),
        };
        iter.map(|item| {
            item.map(|(key, value)| (Slice(key, PhantomData), Slice(value, PhantomData)))
                .map_err(Error::Fjall)
        })
    }

    /// Get a value from the database.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Slice<'_>>> {
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
    pub fn put(&mut self, key: impl AsRef<[u8]>, value: Option<impl AsRef<[u8]>>) -> Result<()> {
        match &mut self.tx {
            InnerTx::Read(_) => {
                return Err(Error::ReadOnlyTransaction);
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
    pub fn commit(self) -> Result<()> {
        match self.tx {
            InnerTx::Read(_) => Err(Error::ReadOnlyTransaction),
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
