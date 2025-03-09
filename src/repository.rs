//! Utilities to interact with a Solipr repository.

use std::collections::HashSet;
use std::marker::PhantomData;

use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

use crate::config::CONFIG;
use crate::identifier::{ChangeHash, ContentHash, DocumentId, RepositoryId};
use crate::storage::{Database, ReadTransaction, Registry, Slice, WriteTransaction};

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

    /// Returns the value associated with the given key in the document store.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn store_read(&self, key: impl AsRef<[u8]>) -> anyhow::Result<Option<Slice>> {
        let mut final_key = format!("store/{}/", self.id).into_bytes();
        final_key.extend_from_slice(key.as_ref());
        self.repository.transaction.get(final_key)
    }

    /// Retrieves all keys with the given prefix in the document store.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn store_keys(
        &self,
        prefix: impl AsRef<[u8]>,
    ) -> impl Iterator<Item = Result<(Slice, Slice), anyhow::Error>> {
        let mut final_prefix = format!("store/{}/", self.id).into_bytes();
        let base_len = final_prefix.len();
        final_prefix.extend_from_slice(prefix.as_ref());
        self.repository
            .transaction
            .keys(final_prefix)
            .map(move |entry| match entry {
                Ok((key, value)) => Ok((Slice(key.0.slice(base_len..), PhantomData), value)),
                Err(e) => Err(e),
            })
    }

    /// Returns the [Change] with the given [`ChangeHash`] applied to this
    /// document.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn change(self, hash: ChangeHash) -> anyhow::Result<Option<Change>> {
        match self
            .repository
            .transaction
            .get(format!("changes/{}/{hash}", self.id))?
        {
            Some(value) => Ok(Some(borsh::from_slice(&value)?)),
            None => Ok(None),
        }
    }

    /// Returns the hashes of the [Change]s that depend on the given
    /// [`ChangeHash`] in this document.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn dependents(&self, change_hash: ChangeHash) -> anyhow::Result<HashSet<ChangeHash>> {
        match self
            .repository
            .transaction
            .get(format!("dependents/{}/{change_hash}", self.id))?
        {
            Some(value) => Ok(borsh::from_slice(&value)?),
            None => Ok(HashSet::new()),
        }
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

    /// Returns the value associated with the given key in the document store.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn store_read(&self, key: impl AsRef<[u8]>) -> anyhow::Result<Option<Slice>> {
        let mut final_key = format!("store/{}/", self.id).into_bytes();
        final_key.extend_from_slice(key.as_ref());
        self.repository.transaction.get(final_key)
    }

    /// Retrieves all keys with the given prefix in the document store.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn store_keys(
        &self,
        prefix: impl AsRef<[u8]>,
    ) -> impl Iterator<Item = Result<(Slice, Slice), anyhow::Error>> {
        let mut final_prefix = format!("store/{}/", self.id).into_bytes();
        let base_len = final_prefix.len();
        final_prefix.extend_from_slice(prefix.as_ref());
        self.repository
            .transaction
            .keys(final_prefix)
            .map(move |entry| match entry {
                Ok((key, value)) => Ok((Slice(key.0.slice(base_len..), PhantomData), value)),
                Err(e) => Err(e),
            })
    }

    /// Returns the [Change] with the given [`ChangeHash`] applied to this
    /// document.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn change(self, hash: ChangeHash) -> anyhow::Result<Option<Change>> {
        match self
            .repository
            .transaction
            .get(format!("changes/{}/{hash}", self.id))?
        {
            Some(value) => Ok(Some(borsh::from_slice(&value)?)),
            None => Ok(None),
        }
    }

    /// Returns the hashes of the [Change]s that depend on the given
    /// [`ChangeHash`] in this document.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn dependents(&self, change_hash: ChangeHash) -> anyhow::Result<HashSet<ChangeHash>> {
        match self
            .repository
            .transaction
            .get(format!("dependents/{}/{change_hash}", self.id))?
        {
            Some(value) => Ok(borsh::from_slice(&value)?),
            None => Ok(HashSet::new()),
        }
    }
}

/// A change made to a document in a [Repository].
#[derive(BorshDeserialize, BorshSerialize)]
pub struct Change {
    /// The dependencies of this [Change].
    ///
    /// This [Change] will not be able to be applied until all its dependencies
    /// have been applied.
    pub dependencies: HashSet<ChangeHash>,

    /// The hashes of the contents used by this [Change].
    ///
    /// This [Change] will not be able to be applied until all these contents
    /// are present in the registry.
    pub used_contents: HashSet<ContentHash>,

    /// Plugin-specific data associated with this [Change].
    pub plugin_data: Vec<u8>,
}

impl Change {
    /// Calculates the [`ChangeHash`] corresponding to this [Change].
    #[must_use]
    pub fn hash(&self) -> ChangeHash {
        let mut hasher = Sha256::new();
        let _ = borsh::to_writer(&mut hasher, &self);
        ChangeHash(hasher.finalize().into())
    }
}
