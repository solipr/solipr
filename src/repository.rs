//! Defines a [`RepositoryManager`] and [Repository] traits.
//!
//! These traits are used to open repositories, apply changes to them and
//! retrieve information from them.

use std::collections::HashSet;
use std::error::Error;
use std::fmt::{self, Display};

use borsh::{BorshDeserialize, BorshSerialize};
use uuid::Uuid;

use crate::change::{Change, ChangeContent, ChangeHash, FileId, LineId, SingleId};
use crate::registry::ContentHash;

/// The identifier of a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize)]
pub struct RepositoryId(Uuid);

impl RepositoryId {
    /// Creates a new [`RepositoryId`] that is guaranteed to be unique.
    #[must_use]
    pub fn create_new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Display for RepositoryId {
    #[expect(clippy::min_ident_chars, reason = "The trait is made that way")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "repo:{}", self.0)
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
    fn changes(&self)
    -> Result<impl Iterator<Item = Result<ChangeHash, Self::Error>>, Self::Error>;

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
    ///
    /// # Note
    ///
    /// The default implementation is very inefficient and should be overridden
    /// if possible.
    fn heads(&self, single_id: SingleId) -> Result<HashSet<ChangeHash>, Self::Error> {
        let single_changes = self
            .changes()?
            .map(|change_hash| change_hash.and_then(|change_hash| self.change(change_hash)))
            .filter_map(|change| match change {
                Ok(Some(change)) if change.single_id() == single_id => Some(Ok(change)),
                Err(error) => Some(Err(error)),
                _ => None,
            })
            .collect::<Result<HashSet<_>, _>>()?;
        let mut heads = HashSet::new();
        'outer: for change in &single_changes {
            let hash = change.calculate_hash();
            for other in &single_changes {
                if other.replace.contains(&hash) {
                    continue 'outer;
                }
            }
            heads.insert(hash);
        }
        Ok(heads)
    }

    /// Returns the existence of the given [`LineId`].
    ///
    /// If there is an existence conflict, `None` will be returned. If there
    /// is no conflict, `Some(true)` or `Some(false)` will be returned if the
    /// line exists or not respectively.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn line_existence(
        &self,
        file_id: FileId,
        line_id: LineId,
    ) -> Result<Option<bool>, Self::Error> {
        let heads = self.heads(SingleId::LineExistence(file_id, line_id))?;
        let mut current_value = None;
        for head in heads {
            if let Some(Change {
                content: ChangeContent::LineExistence { existence, .. },
                ..
            }) = self.change(head)?
            {
                if current_value.is_none() {
                    current_value = Some(existence);
                } else if current_value != Some(existence) {
                    return Ok(None);
                }
            }
        }
        Ok(Some(current_value.unwrap_or(false)))
    }

    /// Returns the content of the given [`LineId`].
    ///
    /// If there is a conflict, multiple values will be returned. If no content
    /// has been set, an empty set will be returned.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn line_content(
        &self,
        file_id: FileId,
        line_id: LineId,
    ) -> Result<HashSet<ContentHash>, Self::Error> {
        let heads = self.heads(SingleId::LineContent(file_id, line_id))?;
        let mut result = HashSet::with_capacity(heads.len());
        for head in heads {
            if let Some(Change {
                content: ChangeContent::LineContent { content, .. },
                ..
            }) = self.change(head)?
            {
                result.insert(content);
            }
        }
        Ok(result)
    }

    /// Returns the parent of the given [`LineId`].
    ///
    /// If there is a conflict, multiple values will be returned. If no parent
    /// has been set, [`LineId::FIRST`] will be returned.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn line_parent(
        &self,
        file_id: FileId,
        line_id: LineId,
    ) -> Result<HashSet<LineId>, Self::Error> {
        let heads = self.heads(SingleId::LineParent(file_id, line_id))?;
        let mut result = HashSet::with_capacity(heads.len());
        for head in heads {
            if let Some(Change {
                content: ChangeContent::LineParent { parent, .. },
                ..
            }) = self.change(head)?
            {
                result.insert(parent);
            }
        }
        if result.is_empty() {
            result.insert(LineId::FIRST);
        }
        Ok(result)
    }

    /// Returns the child of the given [`LineId`].
    ///
    /// If there is a conflict, multiple values will be returned. If no child
    /// has been set, [`LineId::LAST`] will be returned.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn line_child(&self, file_id: FileId, line_id: LineId) -> Result<HashSet<LineId>, Self::Error> {
        let heads = self.heads(SingleId::LineChild(file_id, line_id))?;
        let mut result = HashSet::with_capacity(heads.len());
        for head in heads {
            if let Some(Change {
                content: ChangeContent::LineChild { child, .. },
                ..
            }) = self.change(head)?
            {
                result.insert(child);
            }
        }
        if result.is_empty() {
            result.insert(LineId::LAST);
        }
        Ok(result)
    }

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
    fn apply(&self, change: Change) -> Result<(), Self::Error>;

    /// Unapplies the change with the given [`ChangeHash`].
    ///
    /// If the change is not applied, `Ok(())` will be returned and nothing
    /// will be done.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn unapply(&self, change_hash: ChangeHash) -> Result<(), Self::Error>;

    /// Commit the changes made to the repository.
    ///
    /// # Errors
    ///
    /// An error will be returned if there was an error while doing the
    /// operation.
    fn commit(self) -> Result<(), Self::Error>;
}
