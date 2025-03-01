//! This module contains the implementation of a runtime for Solipr plugins.

use std::sync::LazyLock;

use anyhow::Context;
use wasmtime::{Engine, Instance, Linker, Memory, Module, Store, TypedFunc};

/// The [Engine] used by all the instances of [Plugin].
static ENGINE: LazyLock<Engine> = LazyLock::new(Engine::default);

/// The context used when a host function is called from the guest code.
struct PluginCtx<Data> {
    /// The data that is passed to the plugin.
    #[expect(dead_code, reason = "will be used in the future")]
    data: Data,

    /// The memory of the [Plugin].
    #[expect(dead_code, reason = "will be used in the future")]
    memory: Memory,

    /// The plugin function that allocates a buffer of a given size.
    #[expect(dead_code, reason = "will be used in the future")]
    alloc: TypedFunc<u32, u32>,

    /// The plugin function that frees a buffer allocated by the [`Self::alloc`]
    /// function.
    #[expect(dead_code, reason = "will be used in the future")]
    dealloc: TypedFunc<(u32, u32), ()>,
}

/// A WebAssembly plugin instance.
pub struct Plugin<Data> {
    /// The [Store] used by the [Plugin] in the [wasmtime] runtime.
    #[expect(dead_code, reason = "will be used in the future")]
    store: Store<Option<PluginCtx<Data>>>,

    /// The [Instance] of the [Plugin] in the [wasmtime] runtime.
    #[expect(dead_code, reason = "will be used in the future")]
    instance: Instance,
}

impl<Data> Plugin<Data> {
    /// Loads a [Plugin] from the given WebAssembly bytes.
    ///
    /// # Errors
    ///
    /// If the WebAssembly module is invalid or cannot be loaded an error is
    /// returned.
    pub fn load(bytes: impl AsRef<[u8]>, data: Data) -> anyhow::Result<Self> {
        let module = Module::new(&ENGINE, bytes)?;
        let mut store = Store::new(&ENGINE, None);

        // Instantiate the plugin.
        let linker = Linker::new(&ENGINE);
        let instance = linker.instantiate(&mut store, &module)?;

        // Define the plugin context.
        *store.data_mut() = Some(PluginCtx {
            data,
            memory: instance
                .get_memory(&mut store, "memory")
                .context("no memory")?,
            alloc: instance.get_typed_func(&mut store, "alloc")?,
            dealloc: instance.get_typed_func(&mut store, "dealloc")?,
        });

        // Return the plugin instance.
        Ok(Self { store, instance })
    }
}
