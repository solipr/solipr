//! The core systems of Solipr.

use std::error::Error as StdError;
use std::future::Future;

use futures_core::Stream;

/// A database that can be used to store and retrieve data using transactions.
pub trait Database {
    /// A fatal error that can be returned by the database.
    ///
    /// If this error is returned by the database, the database should not be
    /// used again.
    type FatalError: StdError;

    /// A transaction that can only be used to read data from the database.
    type ReadTx: ReadTx;

    /// A transaction that can read and write data to the database.
    type WriteTx: WriteTx;

    /// Open a new read-only transaction.
    ///
    /// There can be multiple read-only transactions open at the same time.
    fn read_tx(&self) -> impl Future<Output = Result<Self::ReadTx, Self::FatalError>> + Send;

    /// Open a new transaction that can both read and write data to the
    /// database.
    ///
    /// There can be only one write transaction open at a time.
    fn write_tx(&self) -> impl Future<Output = Result<Self::WriteTx, Self::FatalError>> + Send;
}

/// A read-only transaction that can be used to retrieve data from the database.
///
/// A read-only transaction is a snapshot of the database at some point in time.
/// There can be multiple read-only transactions open at the same time.
pub trait ReadTx {
    /// A fatal error that can be returned by the transaction.
    ///
    /// If this error is returned by the transaction, the transaction should not
    /// be used again.
    type FatalError: StdError;

    /// Get a value from the database.
    fn get(
        &self,
        key: impl AsRef<[u8]>,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, Self::FatalError>> + Send;

    /// Get a [Stream] of all values in the database that start with the given
    /// prefix.
    ///
    /// If the prefix is empty, this will return all values in the database.
    fn keys<'tx>(
        &'tx self,
        prefix: impl AsRef<[u8]> + 'tx,
    ) -> impl Stream<Item = Result<Vec<u8>, Self::FatalError>> + Send + 'tx;
}

/// A write transaction that can both read and write data to the database.
///
/// There can only be one write transaction open at a time.
pub trait WriteTx: ReadTx {
    /// Put a value in the database. If there is already a value for this key,
    /// it will be overwritten.
    ///
    /// If the `value` is `None`, this will remove the existing value for the
    /// key.
    fn put(
        &self,
        key: impl AsRef<[u8]>,
        value: Option<impl AsRef<[u8]>>,
    ) -> impl Future<Output = Result<(), Self::FatalError>> + Send;
}
