//! The plugin interface for document plugin in Solipr.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::mem;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;
use std::string::ToString;
use std::sync::LazyLock;

use anyhow::Context;
use inner::{
    Document, DocumentPlugin, DocumentPluginImports, HostDocument, HostReadKvStore,
    HostReadRegistry, HostRenderer, HostWriteKvStore, HostWriteRegistry, ReadKvStore, ReadRegistry,
    Renderer, WriteKvStore, WriteRegistry,
};
use wasmtime::component::{Component, Linker, Resource};
use wasmtime::{Engine, Store};

use crate::identifier::{ChangeHash, ContentHash};
use crate::repository::{Change, ReadDocument, WriteDocument};
use crate::storage::Registry;

/// A module to contains the plugin interface for document plugin in Solipr.
///
/// This module is usefull to remove clippy lints that can't be removed without
/// removing them from the entire module.
mod inner {
    wasmtime::component::bindgen!({
        path: "../../wit",
        world: "document-plugin",
        trappable_imports: true,
    });
}

/// The engine used to run all the plugins.
static ENGINE: LazyLock<Engine> = LazyLock::new(Engine::default);

/// A render block is output by a plugin. It can be either content or bytes.
pub enum RenderBlock {
    /// A content.
    Content(ContentHash),

    /// A byte sequence.
    Bytes(Vec<u8>),
}

/// A read-only document that can use the document plugin.
pub struct PluginReadDocument<'tx> {
    /// The store used to run the document plugin.
    store: Store<ReadHost<'tx>>,

    /// The instance of the document plugin.
    instance: DocumentPlugin,
}

impl<'tx> PluginReadDocument<'tx> {
    /// Load the plugin for an existing [`ReadDocument`].
    ///
    /// # Errors
    ///
    /// If the plugin is not stored in the registry or if the plugin is
    /// malformed this function will return an error.
    pub fn new(registry: Registry, document: ReadDocument<'tx>) -> anyhow::Result<Self> {
        let mut plugin_bytes = Vec::with_capacity(
            registry
                .size(document.id().plugin_hash())?
                .context("plugin not in registry")?
                .try_into()?,
        );
        registry
            .read(document.id().plugin_hash())?
            .context("plugin not in registry")?
            .read_to_end(&mut plugin_bytes)?;
        let component = Component::from_binary(&ENGINE, &plugin_bytes)?;
        let mut linker = Linker::new(&ENGINE);
        DocumentPlugin::add_to_linker(&mut linker, |state: &mut ReadHost<'tx>| state)?;
        let mut store = Store::new(
            &ENGINE,
            ReadHost {
                registry,
                document,
                rendered_blocks: Vec::new(),
            },
        );
        let instance = DocumentPlugin::instantiate(&mut store, &component, &linker)?;
        Ok(Self { store, instance })
    }

    /// Render the current state of the document.
    ///
    /// # Errors
    ///
    /// This function can return an error if the plugin send bad data or panic.
    pub fn render_document(&mut self) -> anyhow::Result<Vec<RenderBlock>> {
        self.store.data_mut().rendered_blocks.clear();
        self.instance.call_render_document(
            &mut self.store,
            Resource::new_borrow(0),
            Resource::new_borrow(0),
            Resource::new_borrow(0),
            Resource::new_borrow(0),
        )?;
        Ok(mem::take(&mut self.store.data_mut().rendered_blocks))
    }

    /// Calculates a change to the current document such that, when applied,
    /// the document will output the target content when rendered with
    /// [`Self::render_document`].
    ///
    /// # Errors
    ///
    /// This function can return an error if the plugin send bad data or panic.
    pub fn calculate_diff(
        &mut self,
        target_content: ContentHash,
    ) -> anyhow::Result<Option<Change>> {
        let result = self.instance.call_calculate_diff(
            &mut self.store,
            Resource::new_borrow(0),
            Resource::new_borrow(0),
            Resource::new_borrow(0),
            &target_content.to_string(),
        )?;
        match result {
            Some(change) => Ok(Some(Change {
                dependencies: change
                    .dependencies
                    .into_iter()
                    .map(|change_hash| ChangeHash::from_str(&change_hash))
                    .collect::<Result<HashSet<_>, _>>()?,
                used_contents: change
                    .used_contents
                    .into_iter()
                    .map(|content_hash| ContentHash::from_str(&content_hash))
                    .collect::<Result<HashSet<_>, _>>()?,
                plugin_data: change.plugin_data,
            })),
            None => Ok(None),
        }
    }
}

impl<'tx> Deref for PluginReadDocument<'tx> {
    type Target = ReadDocument<'tx>;

    fn deref(&self) -> &Self::Target {
        &self.store.data().document
    }
}

/// The data stored by the host to interact with a plugin with a read-only
/// access to the document.
struct ReadHost<'tx> {
    /// The registry used.
    registry: Registry,

    /// The read-only document used.
    document: ReadDocument<'tx>,

    /// A buffer to store block that are rendered.
    rendered_blocks: Vec<RenderBlock>,
}

impl HostReadKvStore for ReadHost<'_> {
    fn read(
        &mut self,
        _: Resource<ReadKvStore>,
        key: Vec<u8>,
    ) -> wasmtime::Result<Option<Vec<u8>>> {
        self.document
            .store_read(key)?
            .map_or_else(|| Ok(None), |value| Ok(Some(value.to_vec())))
    }

    fn keys(
        &mut self,
        _: Resource<ReadKvStore>,
        prefix: Vec<u8>,
    ) -> wasmtime::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.document
            .store_keys(prefix)
            .map(|entry| entry.map(|(key, value)| (key.to_vec(), value.to_vec())))
            .collect::<Result<Vec<_>, _>>()
    }

    fn drop(&mut self, _: Resource<ReadKvStore>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl HostWriteKvStore for ReadHost<'_> {
    fn read(
        &mut self,
        _: Resource<WriteKvStore>,
        _key: Vec<u8>,
    ) -> wasmtime::Result<Option<Vec<u8>>> {
        unreachable!()
    }

    fn keys(
        &mut self,
        _: Resource<WriteKvStore>,
        _prefix: Vec<u8>,
    ) -> wasmtime::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        unreachable!()
    }

    fn write(
        &mut self,
        _: Resource<WriteKvStore>,
        _key: Vec<u8>,
        _value: Option<Vec<u8>>,
    ) -> wasmtime::Result<()> {
        unreachable!()
    }

    fn drop(&mut self, _: Resource<WriteKvStore>) -> wasmtime::Result<()> {
        unreachable!()
    }
}

impl HostReadRegistry for ReadHost<'_> {
    fn read(
        &mut self,
        _: Resource<ReadRegistry>,
        content_hash: String,
        start: u64,
        length: Option<u64>,
    ) -> wasmtime::Result<Option<Vec<u8>>> {
        let Some(mut data) = self.registry.read(ContentHash::from_str(&content_hash)?)? else {
            return Ok(None);
        };
        data.seek(SeekFrom::Start(start))?;
        if let Some(length) = length {
            let mut buffer = Vec::with_capacity(length.try_into()?);
            data.take(length).read_to_end(&mut buffer)?;
            Ok(Some(buffer))
        } else {
            let mut buffer = Vec::new();
            data.read_to_end(&mut buffer)?;
            Ok(Some(buffer))
        }
    }

    fn size(
        &mut self,
        _: Resource<ReadRegistry>,
        content_hash: String,
    ) -> wasmtime::Result<Option<u64>> {
        self.registry.size(ContentHash::from_str(&content_hash)?)
    }

    fn drop(&mut self, _: Resource<ReadRegistry>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl HostWriteRegistry for ReadHost<'_> {
    fn read(
        &mut self,
        _: Resource<WriteRegistry>,
        content_hash: String,
        start: u64,
        length: Option<u64>,
    ) -> wasmtime::Result<Option<Vec<u8>>> {
        let Some(mut data) = self.registry.read(ContentHash::from_str(&content_hash)?)? else {
            return Ok(None);
        };
        data.seek(SeekFrom::Start(start))?;
        if let Some(length) = length {
            let mut buffer = Vec::with_capacity(length.try_into()?);
            data.take(length).read_to_end(&mut buffer)?;
            Ok(Some(buffer))
        } else {
            let mut buffer = Vec::new();
            data.read_to_end(&mut buffer)?;
            Ok(Some(buffer))
        }
    }

    fn size(
        &mut self,
        _: Resource<WriteRegistry>,
        content_hash: String,
    ) -> wasmtime::Result<Option<u64>> {
        self.registry.size(ContentHash::from_str(&content_hash)?)
    }

    fn write(&mut self, _: Resource<WriteRegistry>, data: Vec<u8>) -> wasmtime::Result<String> {
        Ok(self.registry.write(&data[..])?.to_string())
    }

    fn cut(
        &mut self,
        _: Resource<WriteRegistry>,
        content_hash: String,
        start: u64,
        length: Option<u64>,
    ) -> wasmtime::Result<Option<String>> {
        let Some(mut data) = self.registry.read(ContentHash::from_str(&content_hash)?)? else {
            return Ok(None);
        };
        data.seek(SeekFrom::Start(start))?;
        if let Some(length) = length {
            Ok(Some(self.registry.write(data.take(length))?.to_string()))
        } else {
            Ok(Some(self.registry.write(data)?.to_string()))
        }
    }

    fn drop(&mut self, _: Resource<WriteRegistry>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl HostDocument for ReadHost<'_> {
    fn get_change(
        &mut self,
        _: Resource<Document>,
        change_hash: String,
    ) -> wasmtime::Result<Option<inner::Change>> {
        let Some(change) = self.document.change(ChangeHash::from_str(&change_hash)?)? else {
            return Ok(None);
        };
        Ok(Some(inner::Change {
            dependencies: change
                .dependencies
                .into_iter()
                .map(|change_hash| change_hash.to_string())
                .collect(),
            used_contents: change
                .used_contents
                .into_iter()
                .map(|content_hash| content_hash.to_string())
                .collect(),
            plugin_data: change.plugin_data,
        }))
    }

    fn dependent_changes(
        &mut self,
        _: Resource<Document>,
        change_hash: String,
    ) -> wasmtime::Result<Vec<String>> {
        Ok(self
            .document
            .dependents(ChangeHash::from_str(&change_hash)?)?
            .into_iter()
            .map(|change_hash| change_hash.to_string())
            .collect())
    }

    fn drop(&mut self, _: Resource<Document>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl HostRenderer for ReadHost<'_> {
    fn render_bytes(&mut self, _: Resource<Renderer>, bytes: Vec<u8>) -> wasmtime::Result<()> {
        self.rendered_blocks.push(RenderBlock::Bytes(bytes));
        Ok(())
    }

    fn render_content(
        &mut self,
        _: Resource<Renderer>,
        content_hash: String,
    ) -> wasmtime::Result<()> {
        self.rendered_blocks
            .push(RenderBlock::Content(ContentHash::from_str(&content_hash)?));
        Ok(())
    }

    fn drop(&mut self, _: Resource<Renderer>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl DocumentPluginImports for ReadHost<'_> {}

/// A read-write document that can use the document plugin.
pub struct PluginWriteDocument<'tx> {
    /// The store used to run the document plugin.
    store: Store<WriteHost<'tx>>,

    /// The instance of the document plugin.
    instance: DocumentPlugin,
}

impl<'tx> PluginWriteDocument<'tx> {
    /// Load the plugin for an existing [`WriteDocument`].
    ///
    /// # Errors
    ///
    /// If the plugin is not stored in the registry or if the plugin is
    /// malformed this function will return an error.
    pub fn new(registry: Registry, document: WriteDocument<'tx>) -> anyhow::Result<Self> {
        let mut plugin_bytes = Vec::with_capacity(
            registry
                .size(document.id().plugin_hash())?
                .context("plugin not in registry")?
                .try_into()?,
        );
        registry
            .read(document.id().plugin_hash())?
            .context("plugin not in registry")?
            .read_to_end(&mut plugin_bytes)?;
        let component = Component::from_binary(&ENGINE, &plugin_bytes)?;
        let mut linker = Linker::new(&ENGINE);
        DocumentPlugin::add_to_linker(&mut linker, |state: &mut WriteHost<'tx>| state)?;
        let mut store = Store::new(
            &ENGINE,
            WriteHost {
                registry,
                document,
                rendered_blocks: Vec::new(),
            },
        );
        let instance = DocumentPlugin::instantiate(&mut store, &component, &linker)?;
        Ok(Self { store, instance })
    }

    /// Render the current state of the document.
    ///
    /// # Errors
    ///
    /// This function can return an error if the plugin send bad data or panic.
    pub fn render_document(&mut self) -> anyhow::Result<Vec<RenderBlock>> {
        self.store.data_mut().rendered_blocks.clear();
        self.instance.call_render_document(
            &mut self.store,
            Resource::new_borrow(0),
            Resource::new_borrow(0),
            Resource::new_borrow(0),
            Resource::new_borrow(0),
        )?;
        Ok(mem::take(&mut self.store.data_mut().rendered_blocks))
    }

    /// Calculates a change to the current document such that, when applied,
    /// the document will output the target content when rendered with
    /// [`Self::render_document`].
    ///
    /// # Errors
    ///
    /// This function can return an error if the plugin send bad data or panic.
    pub fn calculate_diff(
        &mut self,
        target_content: ContentHash,
    ) -> anyhow::Result<Option<Change>> {
        let result = self.instance.call_calculate_diff(
            &mut self.store,
            Resource::new_borrow(0),
            Resource::new_borrow(0),
            Resource::new_borrow(0),
            &target_content.to_string(),
        )?;
        match result {
            Some(change) => Ok(Some(Change {
                dependencies: change
                    .dependencies
                    .into_iter()
                    .map(|change_hash| ChangeHash::from_str(&change_hash))
                    .collect::<Result<HashSet<_>, _>>()?,
                used_contents: change
                    .used_contents
                    .into_iter()
                    .map(|content_hash| ContentHash::from_str(&content_hash))
                    .collect::<Result<HashSet<_>, _>>()?,
                plugin_data: change.plugin_data,
            })),
            None => Ok(None),
        }
    }

    /// Apply a [Change] to the document, updating its state accordingly.
    ///
    /// # Errors
    ///
    /// This function can return an [anyhow] error if the plugin send bad data
    /// or panic or if there is an unrecoverable error.
    ///
    /// If the [Change] can't be applied because some of its dependencies are
    /// not applied, this method returns an error with a set of the
    /// [`ChangeHash`] of the dependencies that need to be applied first.
    pub fn apply(&mut self, change: &Change) -> anyhow::Result<Result<(), HashSet<ChangeHash>>> {
        if let Err(needed_dependencies) = WriteDocument::apply(&mut *self, change)? {
            return Ok(Err(needed_dependencies));
        }
        self.instance.call_apply_change(
            &mut self.store,
            Resource::new_borrow(0),
            Resource::new_borrow(0),
            Resource::new_borrow(0),
            &change.hash().to_string(),
            &inner::Change {
                dependencies: change
                    .dependencies
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                used_contents: change
                    .used_contents
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                plugin_data: change.plugin_data.clone(),
            },
        )?;
        Ok(Ok(()))
    }

    /// Unapply a [Change] from the document.
    ///
    /// # Errors
    ///
    /// This function can return an [anyhow] error if the plugin send bad data
    /// or panic or if there is an unrecoverable error.
    ///
    /// If there is other [Change] that depends on this one, it returns an error
    /// with a set of the [`ChangeHash`] of those changes.
    pub fn unapply(
        &mut self,
        change_hash: ChangeHash,
    ) -> anyhow::Result<Result<(), HashSet<ChangeHash>>> {
        if let Err(dependents) = WriteDocument::unapply(&mut *self, change_hash)? {
            return Ok(Err(dependents));
        }
        self.instance.call_unapply_change(
            &mut self.store,
            Resource::new_borrow(0),
            Resource::new_borrow(0),
            Resource::new_borrow(0),
            &change_hash.to_string(),
        )?;
        Ok(Ok(()))
    }
}

impl<'tx> Deref for PluginWriteDocument<'tx> {
    type Target = WriteDocument<'tx>;

    fn deref(&self) -> &Self::Target {
        &self.store.data().document
    }
}

impl DerefMut for PluginWriteDocument<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.store.data_mut().document
    }
}

/// The data stored by the host to interact with a plugin with a read-write
/// access to the document.
struct WriteHost<'tx> {
    /// The registry used.
    registry: Registry,

    /// The read-write document used.
    document: WriteDocument<'tx>,

    /// A buffer to store block that are rendered.
    rendered_blocks: Vec<RenderBlock>,
}

impl HostReadKvStore for WriteHost<'_> {
    fn read(
        &mut self,
        _: Resource<ReadKvStore>,
        key: Vec<u8>,
    ) -> wasmtime::Result<Option<Vec<u8>>> {
        self.document
            .store_read(key)?
            .map_or_else(|| Ok(None), |value| Ok(Some(value.to_vec())))
    }

    fn keys(
        &mut self,
        _: Resource<ReadKvStore>,
        prefix: Vec<u8>,
    ) -> wasmtime::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.document
            .store_keys(prefix)
            .map(|entry| entry.map(|(key, value)| (key.to_vec(), value.to_vec())))
            .collect::<Result<Vec<_>, _>>()
    }

    fn drop(&mut self, _: Resource<ReadKvStore>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl HostWriteKvStore for WriteHost<'_> {
    fn read(
        &mut self,
        _: Resource<WriteKvStore>,
        key: Vec<u8>,
    ) -> wasmtime::Result<Option<Vec<u8>>> {
        self.document
            .store_read(key)?
            .map_or_else(|| Ok(None), |value| Ok(Some(value.to_vec())))
    }

    fn keys(
        &mut self,
        _: Resource<WriteKvStore>,
        prefix: Vec<u8>,
    ) -> wasmtime::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.document
            .store_keys(prefix)
            .map(|entry| entry.map(|(key, value)| (key.to_vec(), value.to_vec())))
            .collect::<Result<Vec<_>, _>>()
    }

    fn write(
        &mut self,
        _: Resource<WriteKvStore>,
        key: Vec<u8>,
        value: Option<Vec<u8>>,
    ) -> wasmtime::Result<()> {
        self.document.store_write(key, value)
    }

    fn drop(&mut self, _: Resource<WriteKvStore>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl HostReadRegistry for WriteHost<'_> {
    fn read(
        &mut self,
        _: Resource<ReadRegistry>,
        content_hash: String,
        start: u64,
        length: Option<u64>,
    ) -> wasmtime::Result<Option<Vec<u8>>> {
        let Some(mut data) = self.registry.read(ContentHash::from_str(&content_hash)?)? else {
            return Ok(None);
        };
        data.seek(SeekFrom::Start(start))?;
        if let Some(length) = length {
            let mut buffer = Vec::with_capacity(length.try_into()?);
            data.take(length).read_to_end(&mut buffer)?;
            Ok(Some(buffer))
        } else {
            let mut buffer = Vec::new();
            data.read_to_end(&mut buffer)?;
            Ok(Some(buffer))
        }
    }

    fn size(
        &mut self,
        _: Resource<ReadRegistry>,
        content_hash: String,
    ) -> wasmtime::Result<Option<u64>> {
        self.registry.size(ContentHash::from_str(&content_hash)?)
    }

    fn drop(&mut self, _: Resource<ReadRegistry>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl HostWriteRegistry for WriteHost<'_> {
    fn read(
        &mut self,
        _: Resource<WriteRegistry>,
        content_hash: String,
        start: u64,
        length: Option<u64>,
    ) -> wasmtime::Result<Option<Vec<u8>>> {
        let Some(mut data) = self.registry.read(ContentHash::from_str(&content_hash)?)? else {
            return Ok(None);
        };
        data.seek(SeekFrom::Start(start))?;
        if let Some(length) = length {
            let mut buffer = Vec::with_capacity(length.try_into()?);
            data.take(length).read_to_end(&mut buffer)?;
            Ok(Some(buffer))
        } else {
            let mut buffer = Vec::new();
            data.read_to_end(&mut buffer)?;
            Ok(Some(buffer))
        }
    }

    fn size(
        &mut self,
        _: Resource<WriteRegistry>,
        content_hash: String,
    ) -> wasmtime::Result<Option<u64>> {
        self.registry.size(ContentHash::from_str(&content_hash)?)
    }

    fn write(&mut self, _: Resource<WriteRegistry>, data: Vec<u8>) -> wasmtime::Result<String> {
        Ok(self.registry.write(&data[..])?.to_string())
    }

    fn cut(
        &mut self,
        _: Resource<WriteRegistry>,
        content_hash: String,
        start: u64,
        length: Option<u64>,
    ) -> wasmtime::Result<Option<String>> {
        let Some(mut data) = self.registry.read(ContentHash::from_str(&content_hash)?)? else {
            return Ok(None);
        };
        data.seek(SeekFrom::Start(start))?;
        if let Some(length) = length {
            Ok(Some(self.registry.write(data.take(length))?.to_string()))
        } else {
            Ok(Some(self.registry.write(data)?.to_string()))
        }
    }

    fn drop(&mut self, _: Resource<WriteRegistry>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl HostDocument for WriteHost<'_> {
    fn get_change(
        &mut self,
        _: Resource<Document>,
        change_hash: String,
    ) -> wasmtime::Result<Option<inner::Change>> {
        let Some(change) = self.document.change(ChangeHash::from_str(&change_hash)?)? else {
            return Ok(None);
        };
        Ok(Some(inner::Change {
            dependencies: change
                .dependencies
                .into_iter()
                .map(|change_hash| change_hash.to_string())
                .collect(),
            used_contents: change
                .used_contents
                .into_iter()
                .map(|content_hash| content_hash.to_string())
                .collect(),
            plugin_data: change.plugin_data,
        }))
    }

    fn dependent_changes(
        &mut self,
        _: Resource<Document>,
        change_hash: String,
    ) -> wasmtime::Result<Vec<String>> {
        Ok(self
            .document
            .dependents(ChangeHash::from_str(&change_hash)?)?
            .into_iter()
            .map(|change_hash| change_hash.to_string())
            .collect())
    }

    fn drop(&mut self, _: Resource<Document>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl HostRenderer for WriteHost<'_> {
    fn render_bytes(&mut self, _: Resource<Renderer>, bytes: Vec<u8>) -> wasmtime::Result<()> {
        self.rendered_blocks.push(RenderBlock::Bytes(bytes));
        Ok(())
    }

    fn render_content(
        &mut self,
        _: Resource<Renderer>,
        content_hash: String,
    ) -> wasmtime::Result<()> {
        self.rendered_blocks
            .push(RenderBlock::Content(ContentHash::from_str(&content_hash)?));
        Ok(())
    }

    fn drop(&mut self, _: Resource<Renderer>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl DocumentPluginImports for WriteHost<'_> {}
