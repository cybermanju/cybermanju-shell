// SPDX-License-Identifier: MPL-2.0
//! Explicit numeric registry for one real Rust OSTD function address.
//!
//! Linux export admission is deliberately opt-in by ABI and currently rejects
//! every registered export. This registry does not resolve real Linux symbols,
//! establish Linux compatibility, or evaluate license status, CRCs, or vermagic.
//! The numeric lookup APIs remain independently usable and are not a global
//! admission boundary.

/// The sole export name owned by this registry.
pub const OSTD_NUM_CPUS_EXPORT_NAME: &[u8] = b"cybermanju_ostd_num_cpus";

/// ABI class recorded for a registered OSTD export.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExportAbi {
    /// The function uses Rust's internal ABI and is not admitted as a Linux export.
    RustInternal,
}

/// Compile-time-checked function signature metadata.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExportSignature {
    /// A function taking no arguments and returning `usize`.
    NoArgsUsize,
}

/// Why a registered name cannot be admitted as a Linux export.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LinuxExportAdmissionError {
    /// No exact byte-for-byte match exists in the fixed registry.
    UnregisteredName,
    /// The registered ABI is not explicitly admitted for Linux exports.
    AbiNotAdmitted,
}

/// Metadata for an OSTD export; fields and construction remain private.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OstdExportDescriptor {
    export: linux_module_elf::Export<'static>,
    abi: ExportAbi,
    signature: ExportSignature,
}

impl OstdExportDescriptor {
    /// Return the exact registered export name.
    pub const fn name(&self) -> &'static [u8] {
        self.export.name
    }

    /// Return the numeric function address.
    pub const fn value(&self) -> u64 {
        self.export.value
    }

    /// Return the recorded ABI class.
    pub const fn abi(&self) -> ExportAbi {
        self.abi
    }

    /// Return the compile-time-checked signature class.
    pub const fn signature(&self) -> ExportSignature {
        self.signature
    }
}

/// A fixed, explicit set of genuine Rust OSTD function addresses.
///
/// It cannot be extended with caller-provided names or addresses and uses no
/// heap allocation. Its numeric APIs do not imply Linux ABI compatibility.
pub struct OstdExportRegistry {
    exports: [linux_module_elf::Export<'static>; 1],
}

impl OstdExportRegistry {
    /// Create the registry containing only the OSTD CPU-count function address.
    pub fn new() -> Self {
        let function: fn() -> usize = ostd::cpu::num_cpus;
        let value = function as *const () as usize as u64;
        Self {
            exports: [linux_module_elf::Export {
                name: OSTD_NUM_CPUS_EXPORT_NAME,
                value,
            }],
        }
    }

    /// Borrow the fixed export list.
    pub fn exports(&self) -> &[linux_module_elf::Export<'static>] {
        &self.exports
    }

    /// Resolve an exact byte-for-byte export name to its numeric address.
    pub fn lookup(&self, name: &[u8]) -> Option<u64> {
        self.exports
            .iter()
            .find(|export| export.name == name)
            .map(|export| export.value)
    }

    /// Describe an exact registered name, including its ABI and signature.
    pub fn describe(&self, name: &[u8]) -> Option<OstdExportDescriptor> {
        let export = self
            .exports
            .iter()
            .copied()
            .find(|export| export.name == name)?;
        Some(OstdExportDescriptor {
            export,
            abi: ExportAbi::RustInternal,
            signature: ExportSignature::NoArgsUsize,
        })
    }

    /// Apply the opt-in Linux export admission check to an exact registered name.
    ///
    /// This does not resolve a real Linux symbol or admit ABI compatibility, and
    /// it does not evaluate license status, CRCs, or vermagic. All current
    /// registry entries use the Rust internal ABI and are therefore rejected.
    pub fn admit_linux_export(
        &self,
        name: &[u8],
    ) -> Result<linux_module_elf::Export<'static>, LinuxExportAdmissionError> {
        let descriptor = self
            .describe(name)
            .ok_or(LinuxExportAdmissionError::UnregisteredName)?;
        match descriptor.abi {
            ExportAbi::RustInternal => Err(LinuxExportAdmissionError::AbiNotAdmitted),
        }
    }

    /// Create an ELF resolver that consults Linux export admission only for
    /// requested undefined imports. This is not global admission or Linux
    /// certification, and it does not map or execute module code. Numeric
    /// registry lookup and the numeric resolver remain independently available.
    pub fn linux_symbol_resolver<'a>(
        &'a self,
        bytes: &'a [u8],
        layout: &'a linux_module_elf::Layout,
    ) -> Result<linux_module_elf::SymbolResolver<'a>, linux_module_elf::Error> {
        linux_module_elf::SymbolResolver::with_provider(bytes, layout, self)
    }
}

impl linux_module_elf::ExportProvider for OstdExportRegistry {
    fn resolve_export(&self, name: &[u8]) -> Result<u64, linux_module_elf::Error> {
        self.admit_linux_export(name)
            .map(|export| export.value)
            .map_err(|error| match error {
                LinuxExportAdmissionError::UnregisteredName => {
                    linux_module_elf::Error::MissingSymbol
                }
                LinuxExportAdmissionError::AbiNotAdmitted => {
                    linux_module_elf::Error::ExportNotAdmitted
                }
            })
    }
}

impl Default for OstdExportRegistry {
    fn default() -> Self {
        Self::new()
    }
}
