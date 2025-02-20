//! An implementation of Solipr Database using the fjall library.

use std::future::Future;
use std::path::Path;

use fjall::{Config, Keyspace, TransactionalKeyspace, TransactionalPartitionHandle};
use futures_core::Stream;
use solipr_core::{Database, ReadTx, WriteTx};

/// A [Database] that uses the [fjall] library.
pub struct FjallDatabase {
    keyspace: TransactionalKeyspace,
    partition: TransactionalPartitionHandle,
}

impl FjallDatabase {
    async fn open(path: impl AsRef<Path>) -> Result<Self, fjall::Error> {
        let keyspace = Config::new(path).open_transactional()?;
        let partition = keyspace.open_partition("store", Default::default())?;
        Ok(Self {
            keyspace,
            partition,
        })
    }
}

impl Database for FjallDatabase {
    type FatalError = fjall::Error;
    type ReadTx = FjallReadTx;
    type WriteTx = FjallWriteTx;

    fn read_tx(&self) -> impl Future<Output = Result<Self::ReadTx, Self::FatalError>> + Send {
        todo!()
    }

    fn write_tx(&self) -> impl Future<Output = Result<Self::WriteTx, Self::FatalError>> + Send {
        todo!()
    }
}

/// A [ReadTx] that uses the [fjall] library.
pub struct FjallReadTx {}

impl ReadTx for FjallReadTx {
    type FatalError = fjall::Error;

    fn get(
        &self,
        key: impl AsRef<[u8]>,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, Self::FatalError>> + Send {
        todo!()
    }

    fn keys<'tx>(
        &'tx self,
        prefix: impl AsRef<[u8]> + 'tx,
    ) -> impl Stream<Item = Result<Vec<u8>, Self::FatalError>> + Send + 'tx {
        todo!()
    }
}

/// A [WriteTx] that uses the [fjall] library.
pub struct FjallWriteTx {}

impl ReadTx for FjallWriteTx {
    type FatalError = fjall::Error;

    fn get(
        &self,
        key: impl AsRef<[u8]>,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, Self::FatalError>> + Send {
        todo!()
    }

    fn keys<'tx>(
        &'tx self,
        prefix: impl AsRef<[u8]> + 'tx,
    ) -> impl Stream<Item = Result<Vec<u8>, Self::FatalError>> + Send + 'tx {
        todo!()
    }
}

impl WriteTx for FjallWriteTx {
    fn put(
        &self,
        key: impl AsRef<[u8]>,
        value: Option<impl AsRef<[u8]>>,
    ) -> impl Future<Output = Result<(), Self::FatalError>> + Send {
        todo!()
    }
}
