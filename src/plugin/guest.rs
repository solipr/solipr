//! The guest utilities, which are used by the host to communicate with the
//! plugin.

use std::alloc::{Layout, alloc as std_alloc, dealloc as std_dealloc};
use std::ptr::null_mut;

/// Allocate a new buffer of the given size.
///
/// This function is used by the host to communicate with the plugin. The host
/// is responsible for freeing the buffer when it's no longer needed using the
/// [dealloc] function.
#[unsafe(no_mangle)]
unsafe extern "C" fn alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return null_mut();
    }
    unsafe {
        #[expect(clippy::unwrap_used, reason = "we want to crash on purpose")]
        std_alloc(Layout::array::<u8>(len).unwrap())
    }
}

/// Deallocate the given buffer.
///
/// This function is used by the host after communication with the plugin. This
/// function should be used by the host after using [alloc] to free the buffer.
#[unsafe(no_mangle)]
unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if len == 0 {
        return;
    }
    unsafe {
        #[expect(clippy::unwrap_used, reason = "we want to crash on purpose")]
        std_dealloc(ptr, Layout::array::<u8>(len).unwrap());
    };
}
