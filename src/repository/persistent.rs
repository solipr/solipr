//! An implementation of the [`RepositoryManager`] and [Repository] traits that
//! stores data in persistent storage (on disk).

use std::collections::HashSet;
use std::io;
use std::path::Path;

use fjall::{
    Config, Error, PartitionCreateOptions, ReadTransaction, Slice, TransactionalKeyspace,
    TransactionalPartitionHandle, WriteTransaction,
};

use super::{Repository, RepositoryId, RepositoryManager};
use crate::change::{Change, ChangeHash, SingleId};

/// An implementation of the [`RepositoryManager`] that stores data in
/// persistent storage (on disk).
pub struct PersistentRepositoryManager {
    /// The keyspace used to store data.
    ///
    /// It is an handle to the database.
    keyspace: TransactionalKeyspace,

    /// A handle to the change partition of the database.
    ///
    /// This partition stores all the changes made to the repository.
    changes: TransactionalPartitionHandle,

    /// A handle to the head partition of the database.
    ///
    /// This partition stores an index to find rapidly the heads of a single.
    heads: TransactionalPartitionHandle,
}

impl PersistentRepositoryManager {
    /// Opens the specified folder as a [`PersistentRepositoryManager`].
    ///
    /// # Errors
    ///
    /// An error will be returned if the folder could not be opened.
    #[inline]
    pub fn create(folder: impl AsRef<Path>) -> Result<Self, Error> {
        let keyspace = Config::new(folder).open_transactional()?;
        let changes = keyspace.open_partition("changes", PartitionCreateOptions::default())?;
        let heads = keyspace.open_partition("heads", PartitionCreateOptions::default())?;
        Ok(Self {
            keyspace,
            changes,
            heads,
        })
    }
}

impl RepositoryManager for PersistentRepositoryManager {
    type Error = Error;

    type Repository<'manager>
        = PersistentRepository<'manager>
    where
        Self: 'manager;

    #[inline]
    fn open_read(
        &self,
        repository_id: super::RepositoryId,
    ) -> Result<Self::Repository<'_>, Self::Error> {
        Ok(PersistentRepository {
            id: repository_id,
            manager: self,
            transaction: RepositoryTransaction::Read(self.keyspace.read_tx()),
        })
    }

    #[inline]
    fn open_write(
        &self,
        repository_id: super::RepositoryId,
    ) -> Result<Self::Repository<'_>, Self::Error> {
        Ok(PersistentRepository {
            id: repository_id,
            manager: self,
            transaction: RepositoryTransaction::Write(self.keyspace.write_tx()),
        })
    }
}

/// An enum that represents a read or a write transaction.
enum RepositoryTransaction<'manager> {
    /// A read-only transaction.
    Read(ReadTransaction),

    /// A read-write transaction.
    Write(WriteTransaction<'manager>),
}

/// An implementation of the [Repository] trait that stores data in persistent
/// storage (on disk).
pub struct PersistentRepository<'manager> {
    /// The identifier of the repository.
    id: RepositoryId,

    /// The manager from which this repository was opened.
    manager: &'manager PersistentRepositoryManager,

    /// The transaction on the [`RepositoryManager`] database.
    transaction: RepositoryTransaction<'manager>,
}

impl<'manager> Repository<'manager> for PersistentRepository<'manager> {
    type Error = Error;

    fn changes(&self) -> impl Iterator<Item = Result<(ChangeHash, Change), Self::Error>> {
        let iter: Box<dyn Iterator<Item = Result<(Slice, Slice), _>>> = match self.transaction {
            RepositoryTransaction::Read(ref tx) => {
                Box::new(tx.prefix(&self.manager.changes, self.id.0.as_bytes()))
            }
            RepositoryTransaction::Write(ref tx) => {
                Box::new(tx.prefix(&self.manager.changes, self.id.0.as_bytes()))
            }
        };

        iter.map(|result| match result {
            Ok((key, value)) => {
                let (_, hash) = borsh::from_slice::<(RepositoryId, ChangeHash)>(&key)?;
                let change = borsh::from_slice::<Change>(&value)?;
                Ok((hash, change))
            }
            Err(err) => Err(err),
        })
    }

    #[inline]
    fn change(&self, change_hash: ChangeHash) -> Result<Option<Change>, Self::Error> {
        let key = borsh::to_vec(&(self.id, change_hash))?;
        let value = match self.transaction {
            RepositoryTransaction::Read(ref tx) => tx.get(&self.manager.changes, key)?,
            RepositoryTransaction::Write(ref tx) => tx.get(&self.manager.changes, key)?,
        };
        match value {
            Some(value) => Ok(Some(borsh::from_slice(&value)?)),
            None => Ok(None),
        }
    }

    #[inline]
    fn heads(&self, single_id: SingleId) -> Result<HashSet<ChangeHash>, Self::Error> {
        let key = borsh::to_vec(&(self.id, single_id))?;
        let value = match self.transaction {
            RepositoryTransaction::Read(ref tx) => tx.get(&self.manager.heads, key)?,
            RepositoryTransaction::Write(ref tx) => tx.get(&self.manager.heads, key)?,
        };
        match value {
            Some(value) => Ok(borsh::from_slice(&value)?),
            None => Ok(HashSet::new()),
        }
    }

    #[inline]
    fn apply(&mut self, change: Change) -> Result<ChangeHash, Self::Error> {
        let RepositoryTransaction::Write(ref mut tx) = self.transaction else {
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::ReadOnlyFilesystem,
                "cannot apply changes to read-only transaction",
            )));
        };

        // Insert the change.
        let change_hash = change.calculate_hash();
        tx.insert(
            &self.manager.changes,
            borsh::to_vec(&(self.id, change_hash))?,
            borsh::to_vec(&change)?,
        );

        // Update the heads.
        let single_key = borsh::to_vec(&(self.id, change.single_id()))?;
        let heads = tx.get(&self.manager.heads, &single_key)?;
        let mut heads = match heads {
            Some(heads) => borsh::from_slice(&heads)?,
            None => HashSet::new(),
        };
        for change_hash in change.replace {
            heads.remove(&change_hash);
        }
        heads.insert(change_hash);
        tx.insert(&self.manager.heads, single_key, borsh::to_vec(&heads)?);

        // Return the change hash.
        Ok(change_hash)
    }

    #[inline]
    fn unapply(&mut self, change_hash: ChangeHash) -> Result<(), Self::Error> {
        let RepositoryTransaction::Write(ref mut tx) = self.transaction else {
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::ReadOnlyFilesystem,
                "cannot unapply changes to read-only transaction",
            )));
        };

        // Remove the change.
        let Some(change) = tx.take(
            &self.manager.changes,
            borsh::to_vec(&(self.id, change_hash))?,
        )?
        else {
            return Ok(());
        };
        let change: Change = borsh::from_slice(&change)?;

        // Update the heads.
        let single_key = borsh::to_vec(&(self.id, change.single_id()))?;
        let heads = tx.get(&self.manager.heads, &single_key)?;
        let mut heads = match heads {
            Some(heads) => borsh::from_slice(&heads)?,
            None => HashSet::new(),
        };
        heads.remove(&change_hash);
        'outer: for change_hash in change.replace {
            // TODO: store the dependant changes for each change to make this faster (for
            // @CoCoSol007)
            for entry in tx.prefix(&self.manager.changes, self.id.0.as_bytes()) {
                let other: Change = borsh::from_slice(&entry?.1)?;
                if other.replace.contains(&change_hash) {
                    continue 'outer;
                }
            }
            heads.insert(change_hash);
        }
        tx.insert(&self.manager.heads, single_key, borsh::to_vec(&heads)?);

        // Return success.
        Ok(())
    }

    #[inline]
    fn commit(self) -> Result<(), Self::Error> {
        match self.transaction {
            RepositoryTransaction::Read(_) => Err(Error::Io(io::Error::new(
                io::ErrorKind::ReadOnlyFilesystem,
                "cannot commit read-only transaction",
            ))),
            RepositoryTransaction::Write(tx) => Ok(tx.commit()?),
        }
    }
}
