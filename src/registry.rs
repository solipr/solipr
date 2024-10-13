//! A registry that can be used so store and retrieve bytes arrays of any
//! length.

use core::error::Error;

use async_trait::async_trait;
use futures::{AsyncRead, Stream};

/// The hash of a content stored in the registry.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct ContentHash([u8; 32]);

/// A registry that can be used to store simple byte arrays of any length
/// locally.
#[async_trait]
pub trait Registry {
    /// The error that can occur when opening a transaction to the registry.
    type Error: Error;

    /// The type of a transaction that can be opened on the registry.
    type Transaction<'registry>: RegistryTransaction<'registry>
    where
        Self: 'registry;

    /// Opens a read-only transaction to the registry.
    ///
    /// There can be any number of read-only transactions open at a time. Even
    /// if there is a write transaction.
    ///
    /// If there is a write transaction open, this transaction will not see the
    /// changes made in that write transaction. To see the changes made in the
    /// write transaction, you need to open a new read-only transaction after
    /// the write transaction has been committed.
    async fn read(&self) -> Result<Self::Transaction<'_>, Self::Error>;

    /// Opens a read-write transaction to the registry.
    ///
    /// Only one write transaction can be open at a time.
    async fn write(&self) -> Result<Self::Transaction<'_>, Self::Error>;
}

/// A transaction on a [Registry] that can be used to store and retrieve byte
/// arrays of any length.
#[async_trait]
pub trait RegistryTransaction<'registry> {
    /// The error that can occur when doing operations in the transaction.
    type Error: Error;

    /// The type of a transaction savepoint.
    type Savepoint<'transaction>: RegistrySavepoint<'transaction>
    where
        Self: 'transaction;

    /// The type of a handle to a content in the registry.
    type ContentHandle<'transaction>: ContentHandle<'transaction>
    where
        Self: 'transaction;

    /// Returns a [Stream] of all known tags in the registry.
    async fn known_tags(&self) -> Result<impl Stream<Item = String>, Self::Error>;

    /// Returns a [Stream] of all known content in the registry that match the
    /// given tags.
    async fn list(
        &self,
        tags: impl IntoIterator<Item = String>,
    ) -> Result<impl Stream<Item = Self::ContentHandle<'_>>, Self::Error>;

    /// Creates a snapshot of the current registry state, which can be used to
    /// rollback the database.
    ///
    /// This savepoint will be freed as soon as the returned [Self::Savepoint]
    /// is dropped.
    async fn savepoint(&self) -> Result<Self::Savepoint<'_>, Self::Error>;

    /// Restore the state of the registry to the given [Self::Savepoint].
    ///
    /// Calling this method invalidates all [Self::Savepoint]s created after
    /// the given savepoint.
    async fn restore(&self, savepoint: Self::Savepoint<'_>) -> Result<(), Self::Error>;

    /// Returns a [`ContentHandle`] of the content with the given hash in the
    /// registry.
    ///
    /// Returns `None` if the content is not found.
    async fn read(&self, hash: ContentHash)
    -> Result<Option<Self::ContentHandle<'_>>, Self::Error>;

    /// Writes the given data into the registry with the given tags.
    ///
    /// Returns a [`ContentHandle`] to the written content.
    async fn write(&self, data: impl AsyncRead) -> Result<Self::ContentHandle<'_>, Self::Error>;

    /// Abort the transaction.
    ///
    /// Any changes made during the transaction will be discarded.
    async fn abort(self) -> Result<(), Self::Error>;

    /// Commit the transaction.
    ///
    /// Any changes made during the transaction will be applied to the
    /// registry and new transactions will be able to see them.
    async fn commit(self) -> Result<(), Self::Error>;
}

/// A savepoint on a [RegistryTransaction] that can be used to rollback a
/// transaction to a previous state.
#[async_trait]
pub trait RegistrySavepoint<'transaction> {}

/// A handle to a content in the registry.
#[async_trait]
pub trait ContentHandle<'transaction>: AsyncRead {
    /// The error that can occur when doing operations on the content.
    type Error: Error;

    /// Returns the hash of the content.
    fn hash(&self) -> ContentHash;

    /// Returns the tags of the content.
    async fn tags(&self) -> Result<impl Stream<Item = String>, Self::Error>;

    /// Returns whether the content has the given tag.
    async fn has(&self, tag: String) -> Result<bool, Self::Error>;

    /// Adds the given tags to the content.
    async fn add_tag(&self, tag: String) -> Result<(), Self::Error>;

    /// Removes the given tags from the content.
    async fn remove_tag(&self, tag: String) -> Result<(), Self::Error>;
}
