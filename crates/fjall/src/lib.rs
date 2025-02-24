//! An implementation of Solipr Database using the redb library.

use std::io;
use std::marker::PhantomData;
use std::ops::Deref;
use std::path::Path;
use std::result::Result;

use fjall::{
    Config, PartitionCreateOptions, ReadTransaction, TransactionalKeyspace,
    TransactionalPartitionHandle, WriteTransaction,
};
use solipr_core::storage::{Database, DatabaseTx, Slice};

/// A [Database] implementation using the [fjall] library.
pub struct FjallDatabase {
    /// The underlying [fjall] keyspace.
    keyspace: TransactionalKeyspace,

    /// The underlying [fjall] partition where all data is stored.
    partition: TransactionalPartitionHandle,
}

impl FjallDatabase {
    /// Open a [`FjallDatabase`] in the given folder.
    ///
    /// # Errors
    ///
    /// An error is returned if there is an IO error while opening the folder or
    /// if the database is in an invalid state.
    pub fn open(folder: impl AsRef<Path>) -> Result<Self, fjall::Error> {
        let keyspace = Config::new(folder).open_transactional()?;
        let partition = keyspace.open_partition("data", PartitionCreateOptions::default())?;
        Ok(Self {
            keyspace,
            partition,
        })
    }
}

impl Database for FjallDatabase {
    type FatalError = fjall::Error;

    type Tx<'db> = FjallTx<'db>;

    fn read_tx(&self) -> Result<Self::Tx<'_>, Self::FatalError> {
        Ok(FjallTx {
            partition: &self.partition,
            tx: InnerTx::Read(self.keyspace.read_tx()),
        })
    }

    fn write_tx(&self) -> Result<Self::Tx<'_>, Self::FatalError> {
        Ok(FjallTx {
            partition: &self.partition,
            tx: InnerTx::Write(self.keyspace.write_tx()),
        })
    }
}

/// A [`DatabaseTx`] implementation using the [fjall] library.
pub struct FjallTx<'db> {
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

impl<'db> DatabaseTx<'db> for FjallTx<'db> {
    type FatalError = fjall::Error;

    type Slice<'tx>
        = FjallSlice<'tx>
    where
        Self: 'tx;

    fn keys(
        &self,
        prefix: impl AsRef<[u8]>,
    ) -> impl Iterator<Item = Result<(Self::Slice<'_>, Self::Slice<'_>), Self::FatalError>> {
        let prefix = prefix.as_ref().to_vec();
        let iter: Box<dyn Iterator<Item = _>> = match &self.tx {
            InnerTx::Read(tx) => Box::new(tx.prefix(self.partition, prefix)),
            InnerTx::Write(tx) => Box::new(tx.prefix(self.partition, prefix)),
        };
        iter.map(|item| {
            item.map(|(key, value)| (FjallSlice(key, PhantomData), FjallSlice(value, PhantomData)))
        })
    }

    fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Self::Slice<'_>>, Self::FatalError> {
        let slice = match &self.tx {
            InnerTx::Read(tx) => tx.get(self.partition, key.as_ref())?,
            InnerTx::Write(tx) => tx.get(self.partition, key.as_ref())?,
        };
        Ok(slice.map(|slice| FjallSlice(slice, PhantomData)))
    }

    fn put(
        &mut self,
        key: impl AsRef<[u8]>,
        value: Option<impl AsRef<[u8]>>,
    ) -> Result<(), Self::FatalError> {
        match &mut self.tx {
            InnerTx::Read(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::ReadOnlyFilesystem,
                    "put not allowed in read-only tx",
                )
                .into());
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

    fn commit(self) -> Result<(), Self::FatalError> {
        match self.tx {
            InnerTx::Read(_) => Err(io::Error::new(
                io::ErrorKind::ReadOnlyFilesystem,
                "commit not allowed in read-only tx",
            )
            .into()),
            InnerTx::Write(tx) => tx.commit(),
        }
    }
}

/// A [Slice] implementation using the [fjall] library.
pub struct FjallSlice<'tx>(fjall::Slice, PhantomData<&'tx ()>);

impl Deref for FjallSlice<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl<'tx> Slice<'tx> for FjallSlice<'tx> {}
