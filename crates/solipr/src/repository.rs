//! Utilities to interact with a Solipr repository.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use borsh::{BorshDeserialize, BorshSerialize};
use redb::{
    AccessGuard, Database, Key, MultimapTable, MultimapTableDefinition, ReadOnlyMultimapTable,
    ReadOnlyTable, ReadTransaction, ReadableMultimapTable, ReadableTable, Table, TableDefinition,
    TypeName, Value, WriteTransaction,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::identifier::{ChangeHash, ContentHash, DocumentId, PluginHash};

/// The definition of the table used to store the plugin data of documents.
static STORE_TABLE: TableDefinition<(DocumentId, &[u8]), &[u8]> = TableDefinition::new("store");

/// The definition of the table used to store changes applied to documents.
static CHANGES_TABLE: TableDefinition<(DocumentId, ChangeHash), Change> =
    TableDefinition::new("changes");

/// The definition of the table used to store an index of dependents changes.
static DEPENDENTS_TABLE: MultimapTableDefinition<(DocumentId, ChangeHash), ChangeHash> =
    MultimapTableDefinition::new("dependents");

/// The type of the item returned by the iterator returned by the
/// [`ReadDocument::store_keys`] and [`WriteDocument::store_keys`] function.
type KeysIteratorItem = Result<(Vec<u8>, Vec<u8>), anyhow::Error>;

/// A Solipr raw repository that interacts directly with a repository database.
pub struct RawRepository {
    /// The underlying [redb] database used to store data.
    database: Database,
}

impl RawRepository {
    /// Open the given database file as a [`RawRepository`].
    ///
    /// # Errors
    ///
    /// An error will be returned if the repository could not be opened.
    pub fn open(file: impl AsRef<Path>) -> anyhow::Result<Self> {
        let database = Database::create(file)?;
        let tx = database.begin_write()?;
        tx.open_table(STORE_TABLE)?;
        tx.open_table(CHANGES_TABLE)?;
        tx.open_multimap_table(DEPENDENTS_TABLE)?;
        tx.commit()?;
        Ok(Self { database })
    }

    /// Opens a read-only transaction on the [`RawRepository`].
    ///
    /// # Errors
    ///
    /// An error will be returned if the transaction could not be opened.
    pub fn read(&self) -> anyhow::Result<RawReadRepository> {
        Ok(RawReadRepository {
            tx: self.database.begin_read()?,
        })
    }

    /// Opens a read-write transaction on the [`RawRepository`].
    ///
    /// # Errors
    ///
    /// An error will be returned if the transaction could not be opened.
    pub fn write(&self) -> anyhow::Result<RawWriteRepository> {
        Ok(RawWriteRepository {
            tx: self.database.begin_write()?,
        })
    }
}

/// A read-only transaction on a [`RawRepository`].
///
/// This is the main interface to read data from a [`RawRepository`].
pub struct RawReadRepository {
    /// The underlying [redb] transaction.
    tx: ReadTransaction,
}

impl RawReadRepository {
    /// Opens a document from this [`RawReadRepository`].
    ///
    /// # Errors
    ///
    /// An error will be returned if the database is corrupted or if there is an
    /// IO error.
    pub fn open(&self, id: DocumentId) -> anyhow::Result<RawReadDocument> {
        let store = self.tx.open_table(STORE_TABLE)?;
        let changes = self.tx.open_table(CHANGES_TABLE)?;
        let dependents = self.tx.open_multimap_table(DEPENDENTS_TABLE)?;
        Ok(RawReadDocument {
            id,
            store,
            changes,
            dependents,
        })
    }
}

/// A read-only document from a [`RawReadRepository`].
pub struct RawReadDocument {
    /// The identifier of the document.
    id: DocumentId,

    /// The table used to store the plugin data of documents.
    store: ReadOnlyTable<(DocumentId, &'static [u8]), &'static [u8]>,

    /// The table used to store changes applied to documents.
    changes: ReadOnlyTable<(DocumentId, ChangeHash), Change>,

    /// The table used to store an index of dependents changes.
    dependents: ReadOnlyMultimapTable<(DocumentId, ChangeHash), ChangeHash>,
}

impl RawReadDocument {
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
    pub fn store_read(
        &self,
        key: impl AsRef<[u8]>,
    ) -> anyhow::Result<Option<AccessGuard<&'static [u8]>>> {
        Ok(self.store.get((self.id, key.as_ref()))?)
    }

    /// Retrieves all keys with the given prefix in the document store.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn store_keys(
        &self,
        prefix: impl Into<Vec<u8>>,
    ) -> anyhow::Result<impl Iterator<Item = KeysIteratorItem>> {
        let prefix = prefix.into();
        Ok(self
            .store
            .range((self.id, &prefix[..])..)?
            .map_while(move |item| match item {
                Ok((key, value)) => {
                    let (document, key) = key.value();
                    if document == self.id && key.starts_with(prefix.as_ref()) {
                        Some(Ok::<_, anyhow::Error>((
                            key.to_vec(),
                            value.value().to_vec(),
                        )))
                    } else {
                        None
                    }
                }
                Err(error) => Some(Err(error.into())),
            }))
    }

    /// Returns the [Change] with the given [`ChangeHash`] applied to this
    /// document.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn change(&self, change_hash: ChangeHash) -> anyhow::Result<Option<Change>> {
        Ok(self
            .changes
            .get((self.id, change_hash))?
            .map(|value| value.value()))
    }

    /// Returns the hashes of the [Change]s that depend on the given
    /// [`ChangeHash`] in this document.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn dependents(&self, change_hash: ChangeHash) -> anyhow::Result<BTreeSet<ChangeHash>> {
        let iter = self
            .dependents
            .get((self.id, change_hash))?
            .map(|value| match value {
                Ok(value) => Ok::<_, anyhow::Error>(value.value()),
                Err(error) => Err(error.into()),
            });
        iter.collect::<Result<_, _>>()
    }
}

/// A read-write transaction on a [`RawRepository`].
///
/// This is the main interface to write data to a [`RawRepository`].
pub struct RawWriteRepository {
    /// The underlying [fjall] transaction.
    tx: WriteTransaction,
}

impl RawWriteRepository {
    /// Opens a document from this [`RawWriteRepository`].
    ///
    /// # Errors
    ///
    /// An error will be returned if the database is corrupted or if there is an
    /// IO error.
    pub fn open(&self, id: DocumentId) -> anyhow::Result<RawWriteDocument> {
        let store = self.tx.open_table(STORE_TABLE)?;
        let changes = self.tx.open_table(CHANGES_TABLE)?;
        let dependents = self.tx.open_multimap_table(DEPENDENTS_TABLE)?;
        Ok(RawWriteDocument {
            id,
            store,
            changes,
            dependents,
        })
    }

    /// Commits the transaction.
    ///
    /// # Errors
    ///
    /// If there is an error during committing, it will be returned.
    pub fn commit(self) -> anyhow::Result<()> {
        Ok(self.tx.commit()?)
    }
}

/// A read-write document from a [`RawWriteRepository`].
pub struct RawWriteDocument<'tx> {
    /// The identifier of the document.
    id: DocumentId,

    /// The table used to store the plugin data of documents.
    store: Table<'tx, (DocumentId, &'static [u8]), &'static [u8]>,

    /// The table used to store changes applied to documents.
    changes: Table<'tx, (DocumentId, ChangeHash), Change>,

    /// The table used to store an index of dependents changes.
    dependents: MultimapTable<'tx, (DocumentId, ChangeHash), ChangeHash>,
}

impl RawWriteDocument<'_> {
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
    pub fn store_read(
        &self,
        key: impl AsRef<[u8]>,
    ) -> anyhow::Result<Option<AccessGuard<&'static [u8]>>> {
        Ok(self.store.get((self.id, key.as_ref()))?)
    }

    /// Retrieves all keys (and their values) with the given prefix in the
    /// document store.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn store_keys(
        &self,
        prefix: impl Into<Vec<u8>>,
    ) -> anyhow::Result<impl Iterator<Item = KeysIteratorItem>> {
        let prefix = prefix.into();
        Ok(self
            .store
            .range((self.id, &prefix[..])..)?
            .map_while(move |item| match item {
                Ok((key, value)) => {
                    let (document, key) = key.value();
                    if document == self.id && key.starts_with(prefix.as_ref()) {
                        Some(Ok::<_, anyhow::Error>((
                            key.to_vec(),
                            value.value().to_vec(),
                        )))
                    } else {
                        None
                    }
                }
                Err(error) => Some(Err(error.into())),
            }))
    }

    /// Writes a value to the document store.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn store_write(
        &mut self,
        key: impl AsRef<[u8]>,
        value: Option<impl AsRef<[u8]>>,
    ) -> anyhow::Result<()> {
        if let Some(value) = value {
            self.store.insert((self.id, key.as_ref()), value.as_ref())?;
        } else {
            self.store.remove((self.id, key.as_ref()))?;
        }
        Ok(())
    }

    /// Returns the [Change] with the given [`ChangeHash`] applied to this
    /// document.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn change(&self, change_hash: ChangeHash) -> anyhow::Result<Option<Change>> {
        Ok(self
            .changes
            .get((self.id, change_hash))?
            .map(|value| value.value()))
    }

    /// Returns the hashes of the [Change]s that depend on the given
    /// [`ChangeHash`] in this document.
    ///
    /// # Errors
    ///
    /// This method should return an error if there is an fatal error that can't
    /// be recovered.
    pub fn dependents(&self, change_hash: ChangeHash) -> anyhow::Result<BTreeSet<ChangeHash>> {
        let iter = self
            .dependents
            .get((self.id, change_hash))?
            .map(|value| match value {
                Ok(value) => Ok::<_, anyhow::Error>(value.value()),
                Err(error) => Err(error.into()),
            });
        iter.collect::<Result<_, _>>()
    }

    /// Insert a [Change] into the document.
    ///
    /// If the [Change] is already in the document, this function does nothing.
    ///
    /// # Note
    ///
    /// This method does not call the plugin hooks. It is up to the caller to
    /// call it after calling this function.
    ///
    /// # Errors
    ///
    /// If there is a fatal error that can't be recovered, this method should
    /// return an [anyhow] error.
    ///
    /// If the [Change] can't be applied because some of its dependencies are
    /// not applied, this method returns an error with a set of the
    /// [`ChangeHash`] of the dependencies that need to be applied first.
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
        self.changes.insert((self.id, change_hash), change)?;

        // Update dependents.
        for dependency in &change.dependencies {
            self.dependents
                .insert((self.id, *dependency), change_hash)?;
        }

        // Returns success.
        Ok(Ok(()))
    }

    /// Remove a [Change] from the document.
    ///
    /// If the [Change] is not in the document, this function does nothing.
    ///
    /// # Note
    ///
    /// This method does not call the plugin hooks. It is up to the caller to
    /// call it after calling this function.
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
    ) -> anyhow::Result<Result<Option<Change>, BTreeSet<ChangeHash>>> {
        // Get the change from the database.
        let Some(change) = self.change(change_hash)? else {
            return Ok(Ok(None));
        };

        // Check dependents changes.
        let dependents = self.dependents(change_hash)?;
        if !dependents.is_empty() {
            return Ok(Err(dependents));
        }

        // Remove the change from the database.
        self.changes.remove((self.id, change_hash))?;

        // Update dependents changes.
        for dependency in &change.dependencies {
            self.dependents
                .remove((self.id, *dependency), change_hash)?;
        }

        // Return success.
        Ok(Ok(Some(change)))
    }
}

/// A change made to a document in a [`RawRepository`].
#[derive(Debug, BorshDeserialize, BorshSerialize)]
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

impl Value for Change {
    type SelfType<'a>
        = Self
    where
        Self: 'a;

    type AsBytes<'a>
        = Vec<u8>
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        #[expect(clippy::unwrap_used, reason = "can't do anything else with redb")]
        borsh::from_slice(data).unwrap()
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'b,
    {
        #[expect(clippy::unwrap_used, reason = "can't do anything else with redb")]
        borsh::to_vec(value).unwrap()
    }

    fn type_name() -> redb::TypeName {
        TypeName::new("solipr::Change")
    }
}

impl Value for ChangeHash {
    type SelfType<'a>
        = Self
    where
        Self: 'a;

    type AsBytes<'a>
        = &'a [u8; 32]
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        Some(32)
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        #[expect(clippy::unwrap_used, reason = "can't do anything else with redb")]
        Self(data.try_into().unwrap())
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'b,
    {
        value.as_bytes()
    }

    fn type_name() -> TypeName {
        TypeName::new("solipr::ChangeHash")
    }
}

impl Key for ChangeHash {
    fn compare(a: &[u8], b: &[u8]) -> Ordering {
        a.cmp(b)
    }
}

impl Value for DocumentId {
    type SelfType<'a>
        = Self
    where
        Self: 'a;

    type AsBytes<'a>
        = [u8; 48]
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        Some(48)
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        #[expect(clippy::unwrap_used, reason = "can't do anything else with redb")]
        #[expect(clippy::indexing_slicing, reason = "there is already unwrap, so...")]
        Self(
            PluginHash(data[0..32].try_into().unwrap()),
            Uuid::from_bytes(data[32..48].try_into().unwrap()),
        )
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'b,
    {
        let mut buffer = [0; 48];
        buffer[0..32].copy_from_slice(value.0.as_bytes());
        buffer[32..48].copy_from_slice(value.1.as_bytes());
        buffer
    }

    fn type_name() -> TypeName {
        TypeName::new("solipr::DocumentId")
    }
}

impl Key for DocumentId {
    fn compare(a: &[u8], b: &[u8]) -> Ordering {
        a.cmp(b)
    }
}
