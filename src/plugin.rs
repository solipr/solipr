//! The plugin system of Solipr.

#![allow(missing_docs)]

use borsh::{BorshDeserialize, BorshSerialize};
use wasmtime::component::bindgen;

bindgen!({
    world: "document-plugin",
    trappable_imports: true,
    additional_derives: [
        BorshDeserialize,
        BorshSerialize,
    ],
});
