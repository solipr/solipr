//! Utilities to interact with a Solipr repository.

use crate::config::CONFIG;
use crate::identifier::RepositoryId;
use crate::storage::{Database, ReadTransaction, Registry, WriteTransaction};

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

    /// Opens a read-only transaction on the [Repository].
    ///
    /// # Errors
    ///
    /// See [`Database::read_tx`].
    pub fn read(&self) -> anyhow::Result<ReadRepository> {
        Ok(ReadRepository {
            repository: self,
            transaction: self.database.read_tx()?,
        })
    }

    /// Opens a read-write transaction on the [Repository].
    ///
    /// # Errors
    ///
    /// See [`Database::write_tx`].
    pub fn write(&self) -> anyhow::Result<WriteRepository> {
        Ok(WriteRepository {
            repository: self,
            transaction: self.database.write_tx()?,
        })
    }
}

/// A read-only transaction on a [Repository].
///
/// This is the main interface to read data from a [Repository].
pub struct ReadRepository<'repo> {
    /// The underlying [Repository].
    #[expect(dead_code, reason = "will be used in the future")]
    repository: &'repo Repository,

    /// The [`ReadTransaction`] being used by this [`ReadRepository`].
    #[expect(dead_code, reason = "will be used in the future")]
    transaction: ReadTransaction<'repo>,
}

/// A read-write transaction on a [Repository].
///
/// This is the main interface to write data to a [Repository].
pub struct WriteRepository<'repo> {
    /// The underlying [Repository].
    #[expect(dead_code, reason = "will be used in the future")]
    repository: &'repo Repository,

    /// The [`WriteTransaction`] being used by this [`WriteRepository`].
    #[expect(dead_code, reason = "will be used in the future")]
    transaction: WriteTransaction<'repo>,
}
