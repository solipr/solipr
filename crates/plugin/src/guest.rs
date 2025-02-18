//! The guest utilities, which are used by the host to communicate with the
//! plugin.

use std::alloc::{Layout, alloc as std_alloc, dealloc as std_dealloc};
use std::ptr::null_mut;

pub use solipr_macros::{export_fn, import_fn};

/// This module contains utility functions used by the crate macros.
///
/// This module should not be used by the user of the crate.
pub mod __private {
    #[expect(unused_imports, reason = "this is used by some macros")]
    pub use bincode;
}

/// Allocate a new buffer of the given size.
///
/// This function is used by the host to communicate with the plugin. The host
/// is responsible for freeing the buffer when it's no longer needed using the
/// [dealloc] function.
#[no_mangle]
unsafe extern "C" fn alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return null_mut();
    }
    #[expect(clippy::unwrap_used, reason = "we want to crash on purpose")]
    std_alloc(Layout::array::<u8>(len).unwrap())
}

/// Deallocate the given buffer.
///
/// This function is used by the host after communication with the plugin. This
/// function should be used by the host after using [alloc] to free the buffer.
#[no_mangle]
unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if len == 0 {
        return;
    }
    #[expect(clippy::unwrap_used, reason = "we want to crash on purpose")]
    std_dealloc(ptr, Layout::array::<u8>(len).unwrap());
}
