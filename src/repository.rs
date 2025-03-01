//! Utilities to interact with a Solipr repository.

use crate::config::CONFIG;
use crate::identifier::RepositoryId;
use crate::storage::{Database, Registry, Transaction};

/// A Solipr repository.
pub struct Repository {
    /// The [Registry] of the repository.
    #[expect(dead_code, reason = "will be used in the future")]
    registry: Registry,

    /// The [Database] of the repository.
    database: Database,
}

impl Repository {
    /// Opens the [Repository] with the given [`RepositoryId`].
    ///
    /// # Errors
    ///
    /// An error will be returned if the repository could not be opened.
    pub fn open(repository_id: RepositoryId) -> anyhow::Result<Self> {
        let registry = Registry::open(CONFIG.data_folder.join("registry"));
        let database = Database::open(
            CONFIG
                .data_folder
                .join(format!("repositories/{repository_id}")),
        )?;
        Ok(Self { registry, database })
    }

    /// Opens a read-only view on the [Repository].
    ///
    /// # Errors
    ///
    /// See [`Database::read_tx`].
    pub fn read(&self) -> anyhow::Result<RepositoryView> {
        Ok(RepositoryView {
            repository: self,
            transaction: self.database.read_tx()?,
        })
    }

    /// Opens a read-write view on the [Repository].
    ///
    /// # Errors
    ///
    /// See [`Database::write_tx`].
    pub fn edit(&self) -> anyhow::Result<RepositoryView> {
        Ok(RepositoryView {
            repository: self,
            transaction: self.database.write_tx()?,
        })
    }
}

/// A view on a [Repository] that can be used to read or write data.
///
/// This is the main interface for interacting with a [Repository].
pub struct RepositoryView<'repo> {
    /// The underlying [Repository].
    #[expect(dead_code, reason = "will be used in the future")]
    repository: &'repo Repository,

    /// The [Transaction] being used by this view.
    #[expect(dead_code, reason = "will be used in the future")]
    transaction: Transaction<'repo>,
}
