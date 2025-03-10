//! Utilities to interact with a Solipr repository.

use std::collections::HashSet;
use std::marker::PhantomData;
use std::path::Path;

use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

use crate::identifier::{ChangeHash, ContentHash, DocumentId};
use crate::storage::{Database, ReadTransaction, Slice, WriteTransaction};

/// A Solipr repository.
pub struct Repository {
    /// The [Database] of the repository.
    database: Database,
}

impl Repository {
    /// Opens the [Repository] at the given path.
    ///
    /// # Errors
    ///
    /// An error will be returned if the repository could not be opened.
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        Ok(Self {
            database: Database::open(path)?,
        })
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
    pub fn change(&self, hash: ChangeHash) -> anyhow::Result<Option<Change>> {
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

impl<'repo> WriteRepository<'repo> {
    /// Opens a document from this [`WriteRepository`].
    #[must_use]
    pub const fn open(&'repo mut self, id: DocumentId) -> WriteDocument<'repo> {
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
    repository: &'tx mut WriteRepository<'tx>,
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
    pub fn change(&self, hash: ChangeHash) -> anyhow::Result<Option<Change>> {
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

    /// Apply a [Change] to the document.
    ///
    /// If the [Change] is already applied, this function does nothing.
    ///
    /// # Note
    ///
    /// This method does not call the plugin hooks. It is up to the caller to
    /// call it before calling this function.
    ///
    /// # Errors
    ///
    /// If there is a fatal error that can't be recovered, this method should
    /// return an [anyhow] error.
    ///
    /// If the [Change] can't be applied because
    /// some of its dependencies are not applied, this method returns an error
    /// with a set of the [`ChangeHash`] of the dependencies that need to be
    /// applied first.
    pub fn apply(&mut self, change: &Change) -> anyhow::Result<Result<(), HashSet<ChangeHash>>> {
        // Check if all dependencies are already applied.
        let mut needed_dependencies = HashSet::new();
        for dependency in &change.dependencies {
            if self.change(*dependency)?.is_none() {
                needed_dependencies.insert(*dependency);
            }
        }
        if !needed_dependencies.is_empty() {
            return Ok(Err(needed_dependencies));
        }

        // Add the change to the database.
        let change_hash = change.hash();
        let change_bytes = borsh::to_vec(&change)?;
        self.repository.transaction.put(
            format!("changes/{}/{change_hash}", self.id),
            Some(change_bytes),
        )?;

        // Update dependents.
        for dependency in &change.dependencies {
            let dependents_key = format!("dependents/{}/{dependency}", self.id);
            let mut dependents = match self.repository.transaction.get(&dependents_key)? {
                Some(value) => borsh::from_slice(&value)?,
                None => HashSet::new(),
            };
            dependents.insert(change_hash);
            self.repository
                .transaction
                .put(&dependents_key, Some(borsh::to_vec(&dependents)?))?;
        }

        // Returns success.
        Ok(Ok(()))
    }

    /// Unapply a [Change] from the document.
    ///
    /// If the [Change] is not applied, this function does nothing.
    ///
    /// # Note
    ///
    /// This method does not call the plugin hooks. It is up to the caller to
    /// call it before calling this function.
    ///
    /// # Errors
    ///
    /// If there is a fatal error that can't be recovered, this method should
    /// return an [anyhow] error.
    ///
    /// If there is other [Change] that depends on this one, it returns an error
    /// with a set of the [`ChangeHash`] of those changes.
    pub fn unapply(
        &mut self,
        change_hash: ChangeHash,
    ) -> anyhow::Result<Result<(), HashSet<ChangeHash>>> {
        // Get the change from the database.
        let Some(change) = self.change(change_hash)? else {
            return Ok(Ok(()));
        };

        // Check dependents changes.
        let dependents_key = format!("dependents/{}/{change_hash}", self.id);
        let dependents = match self.repository.transaction.get(&dependents_key)? {
            Some(value) => borsh::from_slice(&value)?,
            None => HashSet::new(),
        };
        if !dependents.is_empty() {
            return Ok(Err(dependents));
        }

        // Remove the change from the database.
        self.repository.transaction.put(
            format!("changes/{}/{change_hash}", self.id),
            None::<Vec<u8>>,
        )?;

        // Update dependents changes.
        for dependency in change.dependencies {
            let dependents_key = format!("dependents/{}/{dependency}", self.id);
            let mut dependents: HashSet<ChangeHash> =
                match self.repository.transaction.get(&dependents_key)? {
                    Some(value) => borsh::from_slice(&value)?,
                    None => HashSet::new(),
                };
            dependents.remove(&change_hash);
            if dependents.is_empty() {
                self.repository
                    .transaction
                    .put(&dependents_key, None::<Vec<u8>>)?;
            } else {
                self.repository
                    .transaction
                    .put(&dependents_key, Some(borsh::to_vec(&dependents)?))?;
            }
        }

        // Return success.
        Ok(Ok(()))
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
