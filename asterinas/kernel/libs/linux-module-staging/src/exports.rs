// SPDX-License-Identifier: MPL-2.0
//! Explicit numeric registry for one real Rust OSTD function address.
//!
//! This registry does not consult a symbol table or the Linux linker and does
//! not evaluate GPL status, CRCs, vermagic, or C ABI compatibility. It is not
//! authorization to call, initialize, or execute anything. It defines no Linux
//! symbol names and does not enable `insmod`. Existing numeric APIs remain
//! available; this registry is not a global admission boundary.

/// The sole export name owned by this registry.
pub const OSTD_NUM_CPUS_EXPORT_NAME: &[u8] = b"cybermanju_ostd_num_cpus";

/// A fixed, explicit set of genuine Rust OSTD function addresses.
///
/// This cannot be extended with caller-provided names or addresses and uses no
/// heap allocation. The Rust `num_cpus` function is not thereby Linux-compatible.
pub struct OstdExportRegistry {
    exports: [linux_module_elf::Export<'static>; 1],
}

impl OstdExportRegistry {
    /// Create the registry containing only the OSTD CPU-count function address.
    pub fn new() -> Self {
        Self {
            exports: [linux_module_elf::Export {
                name: OSTD_NUM_CPUS_EXPORT_NAME,
                value: ostd::cpu::num_cpus as *const () as usize as u64,
            }],
        }
    }

    /// Borrow the fixed export list.
    pub fn exports(&self) -> &[linux_module_elf::Export<'static>] {
        &self.exports
    }

    /// Resolve an exact byte-for-byte export name.
    pub fn lookup(&self, name: &[u8]) -> Option<u64> {
        self.exports
            .iter()
            .find(|export| export.name == name)
            .map(|export| export.value)
    }
}

impl Default for OstdExportRegistry {
    fn default() -> Self {
        Self::new()
    }
}
