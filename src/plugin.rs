//! The plugin system of Solipr.

#![allow(missing_docs)]

use wasmtime::component::bindgen;

bindgen!({
    world: "document-plugin",
    trappable_imports: true,
});
