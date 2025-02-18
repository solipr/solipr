//! This module contains the implementation of a runtime for Solipr plugins.

use std::borrow::Borrow;
use std::future::Future;
use std::sync::LazyLock;

use __private::PluginCtx;
use anyhow::{Context, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;
pub use solipr_macros::{host_fn, host_fn_registry};
use wasmtime::{
    Config as EngineConfig, Engine, Instance, Linker, Memory, Module, Store, TypedFunc,
};

/// The [Engine] used by all the instances of [Plugin].
static ENGINE: LazyLock<Engine> = LazyLock::new(|| {
    let mut config = EngineConfig::new();
    config.async_support(true);
    #[expect(clippy::unwrap_used, reason = "this config is valid")]
    Engine::new(&config).unwrap()
});

/// A WebAssembly plugin instance.
pub struct Plugin<Data: Send> {
    /// The [Store] used by the [Plugin] in the [wasmtime] runtime.
    store: Store<Data>,

    /// The [Instance] of the [Plugin] in the [wasmtime] runtime.
    instance: Instance,

    /// The memory of the [Plugin].
    ///
    /// This is used to pass data to and from the plugin in the [`Plugin::call`]
    /// function.
    memory: Memory,

    /// The plugin function that allocates a buffer of a given size.
    ///
    /// This is used to pass data to and from the plugin in the [`Plugin::call`]
    /// function.
    alloc: TypedFunc<u32, u32>,

    /// The plugin function that frees a buffer allocated by [alloc].
    ///
    /// This is used to pass data to and from the plugin in the [`Plugin::call`]
    /// function.
    dealloc: TypedFunc<(u32, u32), ()>,
}

/// The registry of host functions that can be called by the plugin.
type FunctionRegistry<Data> = linkme::DistributedSlice<
    [(
        &'static str,
        for<'store> fn(
            __private::PluginCtx<'store, Data>,
            u32,
            u32,
        ) -> Box<dyn Future<Output = u64> + Send + 'store>,
    )],
>;

/// This module contains utility functions used by the crate macros.
///
/// This module should not be used by the user of the crate.
pub mod __private {
    use std::ops::{Deref, DerefMut};

    use wasmtime::{Caller, Extern, Memory, TypedFunc};

    /// The context given to a host function when called by the plugin.
    ///
    /// This should not be used directly, use the [`host_fn`](super::host_fn)
    /// macro instead.
    pub struct PluginCtx<'store, Data: Send>(Caller<'store, Data>);

    impl<'store, Data: Send> PluginCtx<'store, Data> {
        /// Creates a new [`PluginCtx`] from the given [Caller].
        pub(crate) const fn new(caller: Caller<'store, Data>) -> Self {
            Self(caller)
        }

        /// Returns the plugin's memory.
        pub fn memory(&mut self) -> Option<Memory> {
            match self.get_export("memory") {
                Some(Extern::Memory(memory)) => Some(memory),
                _ => None,
            }
        }

        /// Returns the alloc function of the plugin.
        pub fn alloc(&mut self) -> Option<TypedFunc<u32, u32>> {
            match self.get_export("alloc") {
                Some(Extern::Func(func)) => func.typed(&mut self.0).ok(),
                _ => None,
            }
        }
    }

    impl<'store, Data: Send> Deref for PluginCtx<'store, Data> {
        type Target = Caller<'store, Data>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<Data: Send> DerefMut for PluginCtx<'_, Data> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
}

impl<Data: Send + 'static> Plugin<Data> {
    /// Loads a [Plugin] from the given WebAssembly bytes.
    ///
    /// # Errors
    ///
    /// This function can fail in multiple ways:
    /// * The given [bytes] are not valid WebAssembly (see [`Module::new`]).
    /// * The WebAssembly module can't be instantiated by the [wasmtime] runtime
    ///   (see [`Linker::instantiate`]).
    /// * The WebAssembly module wasn't made to be used in Solipr.
    pub async fn load(
        bytes: impl AsRef<[u8]> + Send,
        data: Data,
        functions: FunctionRegistry<Data>,
    ) -> anyhow::Result<Self> {
        let module = Module::new(&ENGINE, bytes)?;
        let mut store = Store::new(&ENGINE, data);

        // Instantiate the WebAssembly module.
        let mut linker: Linker<Data> = Linker::new(&ENGINE);
        for (function_name, function) in functions {
            linker.func_wrap_async(
                "env",
                function_name,
                move |caller, (ptr, len): (u32, u32)| function(PluginCtx::new(caller), ptr, len),
            )?;
        }
        let instance = linker.instantiate_async(&mut store, &module).await?;

        // Get the utility functions from the WebAssembly module.
        let memory = instance
            .get_memory(&mut store, "memory")
            .context("no memory")?;
        let alloc = instance.get_typed_func(&mut store, "alloc")?;
        let dealloc = instance.get_typed_func(&mut store, "dealloc")?;

        // Return the plugin instance.
        Ok(Self {
            store,
            instance,
            memory,
            alloc,
            dealloc,
        })
    }

    /// Returns an [Iterator] over the names of all functions in this plugin.
    pub fn functions(&mut self) -> impl Iterator<Item = &str> {
        self.instance.exports(&mut self.store).filter_map(|export| {
            let name = export.name().strip_prefix("_wasm_guest_")?;
            export.into_func()?;
            Some(name)
        })
    }

    /// Calls a function of the [Plugin] with the given arguments and returns
    /// the function's return value.
    ///
    /// # Notes
    ///
    /// This function dont check if the type of the arguments and the return
    /// value correspond to the plugin function signature. So calling a plugin
    /// function with the wrong type can cause a panic in the plugin (but this
    /// will not make the main program panic because plugins are isolated).
    ///
    /// # Errors
    ///
    /// This function can return an error if:
    /// * The function does not exist.
    /// * The given arguments cannot be serialized.
    /// * The function returns a value that cannot be deserialized.
    /// * The plugin function panics.
    /// * The plugin is not made to be executed by Solipr.
    pub async fn call<I: Serialize, O: DeserializeOwned + Send>(
        &mut self,
        function_name: impl AsRef<str> + Send,
        args: impl Borrow<I> + Send,
    ) -> anyhow::Result<O> {
        // Get the function from it's name.
        let function_name = format!("_wasm_guest_{}", function_name.as_ref());
        let function: TypedFunc<(u32, u32), u64> = self
            .instance
            .get_typed_func(&mut self.store, &function_name)?;

        // Get the memory slice to write the arguments to.
        let len: u32 = bincode::serialized_size(args.borrow())?.try_into()?;
        let ptr = self.alloc.call_async(&mut self.store, len).await?;
        let Some(args_slice) = self
            .memory
            .data_mut(&mut self.store)
            .get_mut(ptr as usize..(ptr.saturating_add(len) as usize))
        else {
            self.dealloc.call_async(&mut self.store, (ptr, len)).await?;
            bail!("invalid allocation");
        };

        // Serialize the arguments into the memory slice.
        if let Err(error) = bincode::serialize_into(args_slice, args.borrow()) {
            self.dealloc.call_async(&mut self.store, (ptr, len)).await?;
            bail!("failed to serialize arguments: {}", error);
        }

        // Call the function.
        let value = function.call_async(&mut self.store, (ptr, len)).await?;
        let (ptr, len) = ((value >> 32_i64) as u32, (value & 0xffff_ffff) as u32);

        // Get the memory slice for the result.
        let Some(result_slice) = self
            .memory
            .data(&mut self.store)
            .get(ptr as usize..(ptr.saturating_add(len) as usize))
        else {
            self.dealloc.call_async(&mut self.store, (ptr, len)).await?;
            bail!("invalid allocation");
        };

        // Deserialize the result from the memory slice.
        let result = bincode::deserialize(result_slice);
        self.dealloc.call_async(&mut self.store, (ptr, len)).await?;
        result.context("failed to deserialize result")
    }
}
