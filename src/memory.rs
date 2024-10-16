use std::{collections::HashMap, sync::RwLock};

use async_trait::async_trait;
use futures::Stream;

use crate::registry::{ContentHandle, ContentHash, Registry, RegistryTransaction};

pub struct Memory {
    current_hash: RwLock<HashMap<String, Vec<u8>>>,
    write_hash: RwLock<HashMap<String, Vec<u8>>>,
}

pub type MemoryError = Box<dyn std::error::Error + Send + Sync>;

pub struct MemoryTransaction<'registry> {
    memory: &'registry Memory,
}

pub struct MemorySavepoint<'transaction> {
    transaction: &'transaction MemoryTransaction<'transaction>,
}

pub type MemoryTransactionError = Box<dyn std::error::Error + Send + Sync>;

pub struct MemoryContentHandle<'transaction> {
    transaction: &'transaction MemoryTransaction<'transaction>,
}

pub type MemoryContentHandleError = Box<dyn std::error::Error + Send + Sync>;

#[async_trait]
impl Registry for Memory {
    type Error = MemoryError;
    type Transaction<'registry> = MemoryTransaction<'registry>;

    async fn read(&self) -> Result<Self::Transaction<'_>, Self::Error> {
        todo!()
    }

    async fn write(&self) -> Result<Self::Transaction<'_>, Self::Error> {
        todo!()
    }
}

#[async_trait]
impl<'registry> RegistryTransaction<'registry> for MemoryTransaction<'registry> {
    type Error = MemoryTransactionError;

    type Savepoint<'transaction> = MemorySavepoint<'transaction>
    where
        Self: 'transaction;

    type ContentHandle<'transaction> = MemoryContentHandle<'transaction>
    where
        Self: 'transaction;

    async fn known_tags<T: Stream<Item = String>>(&self) -> Result<T, Self::Error> {
        todo!()
    }

    async fn list(
        &self,
        tags: impl IntoIterator<Item = String>,
    ) -> Result<impl Stream<Item = Self::ContentHandle<'_>>, Self::Error> {
        todo!()
    }

    async fn savepoint(&self) -> Result<Self::Savepoint<'_>, Self::Error> {
        todo!()
    }

    async fn restore(&self, savepoint: Self::Savepoint<'_>) -> Result<(), Self::Error> {
        todo!()
    }

    async fn read(
        &self,
        hash: ContentHash,
    ) -> Result<Option<Self::ContentHandle<'_>>, Self::Error> {
        todo!()
    }

    async fn write(&self, data: impl AsyncRead) -> Result<Self::ContentHandle<'_>, Self::Error> {
        todo!()
    }

    async fn abort(self) -> Result<(), Self::Error> {
        todo!()
    }

    async fn commit(self) -> Result<(), Self::Error> {
        todo!()
    }
}

#[async_trait]
impl<'transaction> ContentHandle<'transaction> for MemoryContentHandle<'transaction> {
    type Error = MemoryContentHandleError;

    fn hash(&self) -> ContentHash {
        todo!()
    }

    async fn tags<T: Stream<Item = String>>(&self) -> Result<T>, Self::Error> {
        todo!()
    }

    async fn has(&self, tag: String) -> Result<bool, Self::Error> {
        todo!()
    }

    async fn add_tag(&self, tag: String) -> Result<(), Self::Error> {
        todo!()
    }

    async fn remove_tag(&self, tag: String) -> Result<(), Self::Error> {
        todo!()
    }
}
