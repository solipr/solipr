//! Defines multiple traits related to Solipr's persistent storage.

use std::error::Error as StdError;
use std::ops::Deref;

/// A database that can be used to store and retrieve bytes using transactions.
pub trait Database {
    /// A fatal error that can be returned by the database.
    ///
    /// If this error is returned by the database, the database should not be
    /// used again.
    type FatalError: StdError;

    /// The type of transaction returned by the database.
    type Tx<'db>: DatabaseTx<'db, FatalError = Self::FatalError>
    where
        Self: 'db;

    /// Open a new read-only transaction.
    ///
    /// There can be multiple read-only transactions open at the same time.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    fn read_tx(&self) -> Result<Self::Tx<'_>, Self::FatalError>;

    /// Open a new transaction that can both read and write data to the
    /// database.
    ///
    /// There can be only one write transaction open at a time. If a write
    /// transaction is already open, then this function will block until it
    /// has been closed before opening a new one.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    fn write_tx(&self) -> Result<Self::Tx<'_>, Self::FatalError>;
}

/// A transaction on a [Database].
///
/// The tranaction can be read-only or read-write dpending on whether it is
/// opened using [`Database::read_tx`] or [`Database::write_tx`].
pub trait DatabaseTx<'db> {
    /// A fatal error that can be returned by the transaction.
    ///
    /// If this error is returned by the transaction, the transaction should not
    /// be used again.
    type FatalError: StdError;

    /// The [`Slice`] used when retrieving data from the database.
    type Slice<'tx>: Slice<'tx>
    where
        Self: 'tx;

    /// Returns an [Iterator] over the keys starting by the given prefix with
    /// their values.
    ///
    /// # Errors
    ///
    /// The iterator should return an error if there is an fatal error that
    /// can't be recovered.
    fn keys(
        &self,
        prefix: impl AsRef<[u8]>,
    ) -> impl Iterator<Item = Result<(Self::Slice<'_>, Self::Slice<'_>), Self::FatalError>>;

    /// Get a value from the database.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Self::Slice<'_>>, Self::FatalError>;

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
    fn put(
        &mut self,
        key: impl AsRef<[u8]>,
        value: Option<impl AsRef<[u8]>>,
    ) -> Result<(), Self::FatalError>;

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
    fn commit(self) -> Result<(), Self::FatalError>;
}

/// A trait for a slice of bytes given by a database.
///
/// This trait enables the [Database] implementation to perform additional
/// actions when a retrieved value is dropped. It is also useful for avoiding
/// the need to clone the data from the [Database].
pub trait Slice<'tx>: Deref<Target = [u8]> {}
