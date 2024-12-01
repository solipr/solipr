//! An implementation of the [`RepositoryManager`] and [Repository] traits that
//! stores data in persistent storage (on disk).

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use base64::prelude::*;
use fjall::{
    Config, Error, PartitionCreateOptions, ReadTransaction, Slice, TransactionalKeyspace,
    TransactionalPartitionHandle, WriteTransaction,
};
use sha2::{Digest, Sha256};
use solipr_core::change::{Change, ChangeContent, ChangeHash, FileId, LineId, SingleId};
use solipr_core::repository::{ContentHash, Repository, RepositoryId, RepositoryManager};

/// An implementation of the [`RepositoryManager`] that stores data in
/// persistent storage (on disk).
pub struct PersistentRepositoryManager {
    /// The folder where the data of the registry is stored.
    registry_folder: PathBuf,

    /// The keyspace used to store data.
    ///
    /// It is an handle to the database.
    keyspace: TransactionalKeyspace,

    /// A handle to the change partition of the database.
    ///
    /// This partition stores all the changes made to the repository.
    changes: TransactionalPartitionHandle,

    /// A handle to the reverse heads partition of the database.
    ///
    /// This partition stores all the parent changes of all changes.
    reverse_heads: TransactionalPartitionHandle,

    /// A handle to the head partition of the database.
    ///
    /// This partition stores an index to find rapidly the heads of a single.
    heads: TransactionalPartitionHandle,

    /// A handle to the existing lines partition of the database.
    ///
    /// This partition stores all the existing lines of the repository.
    lines: TransactionalPartitionHandle,
}

impl PersistentRepositoryManager {
    /// Opens the specified folder as a [`PersistentRepositoryManager`].
    ///
    /// # Errors
    ///
    /// An error will be returned if the folder could not be opened.
    pub fn create(folder: impl AsRef<Path>) -> Result<Self, Error> {
        let folder = folder.as_ref();
        let keyspace = Config::new(folder.join("repositories")).open_transactional()?;
        let changes = keyspace.open_partition("changes", PartitionCreateOptions::default())?;
        let reverse_heads =
            keyspace.open_partition("reverse_heads", PartitionCreateOptions::default())?;
        let heads = keyspace.open_partition("heads", PartitionCreateOptions::default())?;
        let lines = keyspace.open_partition("lines", PartitionCreateOptions::default())?;
        Ok(Self {
            registry_folder: folder.join("registry"),
            keyspace,
            changes,
            reverse_heads,
            heads,
            lines,
        })
    }
}

impl RepositoryManager for PersistentRepositoryManager {
    type Error = Error;

    type Repository<'manager>
        = PersistentRepository<'manager>
    where
        Self: 'manager;

    fn open_read(&self, repository_id: RepositoryId) -> Result<Self::Repository<'_>, Self::Error> {
        Ok(PersistentRepository {
            id: repository_id,
            manager: self,
            transaction: RepositoryTransaction::Read(self.keyspace.read_tx()),
        })
    }

    fn open_write(&self, repository_id: RepositoryId) -> Result<Self::Repository<'_>, Self::Error> {
        Ok(PersistentRepository {
            id: repository_id,
            manager: self,
            transaction: RepositoryTransaction::Write(self.keyspace.write_tx()),
        })
    }

    fn read_content(&self, hash: ContentHash) -> Result<Option<impl Read>, Self::Error> {
        let encoded_hash = BASE64_URL_SAFE_NO_PAD.encode(hash.as_bytes());
        #[expect(
            clippy::string_slice,
            reason = "there is always more than 2 characters in a ContentHash"
        )]
        match File::open(
            self.registry_folder
                .join(&encoded_hash[..2])
                .join(&encoded_hash[2..]),
        ) {
            Ok(file) => Ok(Some(file)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    fn write_content(&self, mut content: impl Read) -> Result<ContentHash, Self::Error> {
        // Create the folder if it doesn't exist
        if !self.registry_folder.exists() {
            fs::create_dir_all(&self.registry_folder)?;
        }

        // Create a temporary file to store the content in
        let temp_file_path = self.registry_folder.join(uuid::Uuid::now_v7().to_string());
        let mut temp_file = File::create(&temp_file_path)?;

        // Loop 32 bytes at a time and update the hasher
        // until we reach the end of the content
        let mut hasher = Sha256::new();
        let mut buffer = [0; 32];
        loop {
            let byte_count = match content.read(&mut buffer) {
                Ok(0) => break,
                Ok(byte_count) => byte_count,
                Err(ref err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err.into()),
            };
            #[expect(
                clippy::indexing_slicing,
                reason = "byte_count is always smaller or equal than 32"
            )]
            hasher.update(&buffer[..byte_count]);
            #[expect(
                clippy::indexing_slicing,
                reason = "byte_count is always smaller or equal than 32"
            )]
            temp_file.write_all(&buffer[..byte_count])?;
        }
        temp_file.flush()?;
        drop(temp_file);

        // Create a unique hash for the content
        let hash = ContentHash::new(hasher.finalize().into());
        let encoded_hash = BASE64_URL_SAFE_NO_PAD.encode(hash.as_bytes());

        // Move the temporary file into the correct location
        let (subfolder, file) = encoded_hash.split_at(2);
        let path_dir = self.registry_folder.join(subfolder);
        if !path_dir.exists() {
            fs::create_dir_all(&path_dir)?;
        }
        fs::rename(temp_file_path, path_dir.join(file))?;

        // Return the hash of the content
        Ok(hash)
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

impl PersistentRepository<'_> {
    /// Updates the existence of a line in the repository.
    fn update_line(&mut self, file_id: FileId, line_id: LineId) -> Result<(), Error> {
        let existence = (self.line_existence(file_id, line_id)?).unwrap_or(true);
        let key = borsh::to_vec(&(self.id, file_id, line_id))?;
        let RepositoryTransaction::Write(ref mut tx) = self.transaction else {
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::ReadOnlyFilesystem,
                "cannot update line in read-only transaction",
            )));
        };
        if existence {
            tx.insert(&self.manager.lines, key, b"");
        } else {
            tx.remove(&self.manager.lines, key);
        }
        Ok(())
    }
}

impl<'manager> Repository<'manager> for PersistentRepository<'manager> {
    type Error = Error;

    type Manager = PersistentRepositoryManager;

    fn manager(&self) -> &'manager Self::Manager {
        self.manager
    }

    fn changes(&self) -> impl Iterator<Item = Result<(ChangeHash, Change), Self::Error>> {
        let iter: Box<dyn Iterator<Item = Result<(Slice, Slice), _>>> = match self.transaction {
            RepositoryTransaction::Read(ref tx) => {
                Box::new(tx.prefix(&self.manager.changes, self.id.as_bytes()))
            }
            RepositoryTransaction::Write(ref tx) => {
                Box::new(tx.prefix(&self.manager.changes, self.id.as_bytes()))
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

    fn existing_lines(&self, file_id: FileId) -> Result<HashSet<LineId>, Self::Error> {
        let prefix = borsh::to_vec(&(self.id, file_id))?;
        let iter: Box<dyn Iterator<Item = Result<(Slice, Slice), _>>> = match self.transaction {
            RepositoryTransaction::Read(ref tx) => Box::new(tx.prefix(&self.manager.lines, prefix)),
            RepositoryTransaction::Write(ref tx) => {
                Box::new(tx.prefix(&self.manager.lines, prefix))
            }
        };

        iter.map(|result| match result {
            Ok((key, _)) => {
                let (_, _, line_id) = borsh::from_slice::<(RepositoryId, FileId, LineId)>(&key)?;
                Ok(line_id)
            }
            Err(err) => Err(err),
        })
        .collect()
    }

    fn apply(&mut self, change: Change) -> Result<ChangeHash, Self::Error> {
        let RepositoryTransaction::Write(ref mut tx) = self.transaction else {
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::ReadOnlyFilesystem,
                "cannot apply changes to read-only transaction",
            )));
        };

        // Insert the change
        let change_hash = change.calculate_hash();
        tx.insert(
            &self.manager.changes,
            borsh::to_vec(&(self.id, change_hash))?,
            borsh::to_vec(&change)?,
        );

        // Update the reversed heads
        for replaced_hash in change.replace {
            let serialized_key = borsh::to_vec(&(self.id, replaced_hash))?;

            // Get all changes that replace this change
            let reverse_heads = tx.get(&self.manager.reverse_heads, &serialized_key)?;
            let mut reverse_heads = match reverse_heads {
                Some(reverse_heads) => borsh::from_slice(&reverse_heads)?,
                None => HashSet::new(),
            };

            // Update the reversed heads by adding this change
            reverse_heads.insert(change_hash);
            tx.insert(
                &self.manager.reverse_heads,
                serialized_key,
                borsh::to_vec(&reverse_heads)?,
            );
        }

        // Update the heads
        let single_key = borsh::to_vec(&(self.id, change.single_id()))?;
        let heads = tx.get(&self.manager.heads, &single_key)?;
        let mut heads = match heads {
            Some(heads) => borsh::from_slice(&heads)?,
            None => HashSet::new(),
        };
        for change_hash in change.replace {
            heads.remove(&change_hash);
        }
        let reverse_heads = tx.get(
            &self.manager.reverse_heads,
            borsh::to_vec(&(self.id, change_hash))?,
        )?;
        let reverse_heads: HashSet<ChangeHash> = match reverse_heads {
            Some(reverse_heads) => borsh::from_slice(&reverse_heads)?,
            None => HashSet::new(),
        };
        if reverse_heads.is_empty() {
            heads.insert(change_hash);
        }
        tx.insert(&self.manager.heads, single_key, borsh::to_vec(&heads)?);

        // Update the line existence if needed
        if let ChangeContent::LineExistence {
            file_id, line_id, ..
        } = change.content
        {
            self.update_line(file_id, line_id)?;
        }

        // Return the change hash
        Ok(change_hash)
    }

    fn unapply(&mut self, change_hash: ChangeHash) -> Result<(), Self::Error> {
        let RepositoryTransaction::Write(ref mut tx) = self.transaction else {
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::ReadOnlyFilesystem,
                "cannot unapply changes to read-only transaction",
            )));
        };

        // Remove the change
        let Some(change) = tx.take(
            &self.manager.changes,
            borsh::to_vec(&(self.id, change_hash))?,
        )?
        else {
            return Ok(());
        };
        let change: Change = borsh::from_slice(&change)?;

        // Update the heads
        let single_key = borsh::to_vec(&(self.id, change.single_id()))?;
        let heads = tx.get(&self.manager.heads, &single_key)?;
        let mut heads = match heads {
            Some(heads) => borsh::from_slice(&heads)?,
            None => HashSet::new(),
        };
        heads.remove(&change_hash);
        for replaced_hash in change.replace {
            let serialized_key = borsh::to_vec(&(self.id, replaced_hash))?;

            // Verify that the replaced change is replaced ONLY by this change
            let Some(reverse_heads) = tx.get(&self.manager.reverse_heads, &serialized_key)? else {
                continue;
            };
            let mut reverse_heads: HashSet<ChangeHash> = borsh::from_slice(&reverse_heads)?;
            if reverse_heads.len() == 1 && reverse_heads.contains(&change_hash) {
                // Add the replaced change to the heads
                heads.insert(replaced_hash);
            }

            // Update the replaced change by removing this change
            reverse_heads.remove(&change_hash);
            if reverse_heads.is_empty() {
                tx.remove(&self.manager.reverse_heads, &serialized_key);
            }
            tx.insert(
                &self.manager.reverse_heads,
                serialized_key,
                borsh::to_vec(&reverse_heads)?,
            );
        }
        tx.insert(&self.manager.heads, single_key, borsh::to_vec(&heads)?);

        // Update the line existence if needed
        if let ChangeContent::LineExistence {
            file_id, line_id, ..
        } = change.content
        {
            self.update_line(file_id, line_id)?;
        }

        // Return success
        Ok(())
    }

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
