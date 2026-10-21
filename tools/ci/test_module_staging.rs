// SPDX-License-Identifier: MPL-2.0
//! Host ownership/rollback tests, NOT physical frame or MMU tests.
#![forbid(unsafe_code)]
extern crate alloc;
// Error's PageTableError conversion needs a type, not a real page table.
mod mm {
    pub mod page_table {
        pub struct PageTableError;
    }
}
#[path = "../../asterinas/ostd/src/error.rs"]
mod error;
pub use error::Error;
#[path = "../../asterinas/ostd/src/mm/module_staging/storage.rs"]
mod storage;
