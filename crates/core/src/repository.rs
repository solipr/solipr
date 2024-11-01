//! Defines a [`RepositoryManager`] and [Repository] traits.
//!
//! These traits are used to open repositories, apply changes to them and
//! retrieve information from them.

use std::collections::HashSet;
use std::error::Error;
use std::fmt::{self, Display};
use std::ops::Deref;
use std::str::FromStr;

use borsh::{BorshDeserialize, BorshSerialize};
use uuid::Uuid;

use crate::change::{Change, ChangeHash, FileId, LineId, SingleId};

pub mod diff;
pub mod graph;
pub mod head;
pub mod linear;

/// The identifier of a repository.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, BorshDeserialize, BorshSerialize,
)]
pub struct RepositoryId(Uuid);

impl RepositoryId {
    /// Creates a new [`RepositoryId`] that is guaranteed to be unique.
    #[must_use]
    pub fn create_new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Display for RepositoryId {
    #[expect(clippy::min_ident_chars, reason = "the trait is made that way")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "repo:{}", self.0)
    }
}

impl FromStr for RepositoryId {
    type Err = uuid::Error;

    fn from_str(mut value: &str) -> Result<Self, Self::Err> {
        value = value.trim();
        value = value.strip_prefix("repo:").unwrap_or(value);
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl Deref for RepositoryId {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A [Repository] manager, used to open repositories.
pub trait RepositoryManager {
    /// The error that can be returned when opening a repository.
    type Error: Error;

    /// The type of [Repository] returned when opening a repository.
    type Repository<'manager>: Repository<'manager>
    where
        Self: 'manager;

    /// Opens a repository with a read-only access.
    ///
    /// If the repository does not exist, an empty repository will be returned.
    ///
    /// # Errors
    ///
    /// An error will be returned if the repository could not be opened.
    fn open_read(&self, repository_id: RepositoryId) -> Result<Self::Repository<'_>, Self::Error>;

    /// Opens a repository with a read-write access.
    ///
    /// If the repository does not exist, it will be created.
    ///
    /// # Errors
    ///
    /// An error will be returned if the repository could not be opened.
    fn open_write(&self, repository_id: RepositoryId) -> Result<Self::Repository<'_>, Self::Error>;
}

/// A repository.
pub trait Repository<'manager> {
    /// The error that can be returned when doing a repository operation.
    type Error: Error;

    /// Returns an [Iterator] over the [Change]s applied to the repository.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn changes(&self) -> impl Iterator<Item = Result<(ChangeHash, Change), Self::Error>>;

    /// Returns a [Change] with the given [`ChangeHash`].
    ///
    /// If the change does not exist, `None` will be returned.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn change(&self, change_hash: ChangeHash) -> Result<Option<Change>, Self::Error>;

    /// Returns the heads of the given [`SingleId`].
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn heads(&self, single_id: SingleId) -> Result<HashSet<ChangeHash>, Self::Error>;

    /// Returns the the existing [`LineId`]s in a file of the [Repository].
    ///
    /// If the existence of a line was not defined, it is considered to not
    /// exist.
    ///
    /// If the existence of a line is in a conflict state, this function will
    /// return it.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn existing_lines(&self, file_id: FileId) -> Result<HashSet<LineId>, Self::Error>;

    /// Applies the given [`Change`] to the repository and returns the hash of
    /// the applied change.
    ///
    /// If the [Change] is already applied, `Ok(())` will be returned and
    /// nothing will be done.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn apply(&mut self, change: Change) -> Result<ChangeHash, Self::Error>;

    /// Unapplies the change with the given [`ChangeHash`].
    ///
    /// If the change is not applied, `Ok(())` will be returned and nothing
    /// will be done.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn unapply(&mut self, change_hash: ChangeHash) -> Result<(), Self::Error>;

    /// Commit the changes made to the repository.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn commit(self) -> Result<(), Self::Error>;
}
