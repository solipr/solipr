//! Utilities to interact with a Solipr repository.

use wasmtime::component::Resource;

use crate::config::CONFIG;
use crate::identifier::{DocumentId, RepositoryId};
use crate::plugin::{Change, Document, HostDocument};
use crate::storage::{Database, ReadTransaction, Registry, WriteTransaction};

/// A Solipr repository.
pub struct Repository {
    /// The identifier of the repository.
    id: RepositoryId,

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
        Ok(Self {
            id: repository_id,
            registry,
            database,
        })
    }

    /// Returns the identifier of the repository.
    #[must_use]
    pub const fn id(&self) -> RepositoryId {
        self.id
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
    transaction: ReadTransaction<'repo>,
}

impl ReadRepository<'_> {
    /// Opens a document from this [`ReadRepository`].
    #[must_use]
    pub const fn open(&self, id: DocumentId) -> ReadDocument<'_> {
        ReadDocument {
            id,
            repository: self,
        }
    }
}

/// A read-only document from a [`ReadRepository`].
pub struct ReadDocument<'tx> {
    /// The identifier of the document.
    id: DocumentId,

    /// The underlying [`ReadRepository`].
    repository: &'tx ReadRepository<'tx>,
}

impl ReadDocument<'_> {
    /// Returns the identifier of the document.
    #[must_use]
    pub const fn id(&self) -> DocumentId {
        self.id
    }
}

impl HostDocument for ReadDocument<'_> {
    fn get_change(
        &mut self,
        _: Resource<Document>,
        change_hash: String,
    ) -> wasmtime::Result<Option<Change>> {
        let change_key = format!("{}/changes/{}", self.id, change_hash);
        match self.repository.transaction.get(change_key)? {
            Some(value) => Ok(borsh::from_slice(&value)?),
            None => Ok(None),
        }
    }

    fn dependent_changes(
        &mut self,
        _: Resource<Document>,
        change_hash: String,
    ) -> wasmtime::Result<Vec<String>> {
        let change_key = format!("{}/dependents/{}", self.id, change_hash);
        match self.repository.transaction.get(change_key)? {
            Some(value) => Ok(borsh::from_slice(&value)?),
            None => Ok(Vec::new()),
        }
    }

    fn drop(&mut self, _: Resource<Document>) -> wasmtime::Result<()> {
        Ok(())
    }
}

/// A read-write transaction on a [Repository].
///
/// This is the main interface to write data to a [Repository].
pub struct WriteRepository<'repo> {
    /// The underlying [Repository].
    #[expect(dead_code, reason = "will be used in the future")]
    repository: &'repo Repository,

    /// The [`WriteTransaction`] being used by this [`WriteRepository`].
    transaction: WriteTransaction<'repo>,
}

impl WriteRepository<'_> {
    /// Opens a document from this [`WriteRepository`].
    #[must_use]
    pub const fn open(&self, id: DocumentId) -> WriteDocument<'_> {
        WriteDocument {
            id,
            repository: self,
        }
    }

    /// Commits the transaction.
    ///
    /// # Errors
    ///
    /// See [`WriteTransaction::commit`].
    pub fn commit(self) -> anyhow::Result<()> {
        self.transaction.commit()
    }
}

/// A read-write document from a [`WriteRepository`].
pub struct WriteDocument<'tx> {
    /// The identifier of the document.
    id: DocumentId,

    /// The underlying [`WriteRepository`].
    repository: &'tx WriteRepository<'tx>,
}

impl WriteDocument<'_> {
    /// Returns the identifier of the document.
    #[must_use]
    pub const fn id(&self) -> DocumentId {
        self.id
    }
}

impl HostDocument for WriteDocument<'_> {
    fn get_change(
        &mut self,
        _: Resource<Document>,
        change_hash: String,
    ) -> wasmtime::Result<Option<Change>> {
        let change_key = format!("{}/changes/{}", self.id, change_hash);
        match self.repository.transaction.get(change_key)? {
            Some(value) => Ok(borsh::from_slice(&value)?),
            None => Ok(None),
        }
    }

    fn dependent_changes(
        &mut self,
        _: Resource<Document>,
        change_hash: String,
    ) -> wasmtime::Result<Vec<String>> {
        let change_key = format!("{}/dependents/{}", self.id, change_hash);
        match self.repository.transaction.get(change_key)? {
            Some(value) => Ok(borsh::from_slice(&value)?),
            None => Ok(Vec::new()),
        }
    }

    fn drop(&mut self, _: Resource<Document>) -> wasmtime::Result<()> {
        Ok(())
    }
}
