// SPDX-License-Identifier: MPL-2.0
//! Runtime validation of one closed x86_64 C ABI shim; no ELF code is executed.
use alloc::vec::Vec;

use linux_module_elf::{Error as ElfError, ExportProvider, plan_page_layout};
use ostd::prelude::*;

use crate::{
    CShimExportRegistry, ExportAbi, ExportSignature, LINUX_MEMCMP_EXPORT_NAME,
    LinuxExportAdmissionError, OSTD_NUM_CPUS_EXPORT_NAME, OstdExportRegistry,
};

use super::{BASE, FIXTURE, MAX_SECTIONS, MAX_SPAN, relative_import_fixture};

fn rust_export_import_fixture() -> Vec<u8> {
    let (mut bytes, _name, target, offset) = relative_import_fixture(1);
    let r32 = |b: &[u8], o| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let r64 = |b: &[u8], o| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let shoff = r64(&bytes, 40) as usize;
    let shnum = u16::from_le_bytes(bytes[60..62].try_into().unwrap()) as usize;
    for index in 0..shnum {
        let rela = shoff + index * 64;
        if r32(&bytes, rela + 4) != 4 || r32(&bytes, rela + 44) != u32::from(target) {
            continue;
        }
        let entries = r64(&bytes, rela + 24) as usize;
        let size = r64(&bytes, rela + 32) as usize;
        for entry in (entries..entries + size).step_by(24) {
            let info = r64(&bytes, entry + 8);
            if r64(&bytes, entry) != offset || info as u32 != 1 {
                continue;
            }
            let symtab = shoff + r32(&bytes, rela + 40) as usize * 64;
            let symbol_index = (info >> 32) as usize;
            let symbol = r64(&bytes, symtab + 24) as usize + symbol_index * 24;
            let strtab = shoff + r32(&bytes, symtab + 40) as usize * 64;
            let strings = r64(&bytes, strtab + 24) as usize;
            let string_size = r64(&bytes, strtab + 32) as usize;
            let mut new_strings = bytes[strings..strings + string_size].to_vec();
            let new_name = new_strings.len();
            new_strings.extend_from_slice(OSTD_NUM_CPUS_EXPORT_NAME);
            new_strings.push(0);
            let new_offset = bytes.len();
            bytes.extend_from_slice(&new_strings);
            bytes[strtab + 24..strtab + 32].copy_from_slice(&(new_offset as u64).to_le_bytes());
            bytes[strtab + 32..strtab + 40]
                .copy_from_slice(&(new_strings.len() as u64).to_le_bytes());
            bytes[symbol..symbol + 4].copy_from_slice(&(new_name as u32).to_le_bytes());
            return bytes;
        }
    }
    panic!("fixture needs an ALLOC RELA global symbol");
}

#[ktest]
fn cybermanju_linux_c_abi_runtime() {
    println!("");
    let cpus = ostd::cpu::num_cpus();
    assert_eq!(cpus, 1);
    let registry = CShimExportRegistry::new();
    let exports = registry.exports();
    assert_eq!(exports.len(), 1);
    println!(
        "CYBERMANJU_LINUX_C_ABI:begin:PASS cpus={} exports={}",
        cpus,
        exports.len()
    );

    let function: ostd::linux_abi::MemcmpFn = ostd::linux_abi::memcmp_entry();
    let expected = function as usize as u64;
    assert_eq!(exports[0].name, LINUX_MEMCMP_EXPORT_NAME);
    assert_eq!(exports[0].value, expected);
    let descriptor = registry.describe(b"memcmp").unwrap();
    assert_eq!(descriptor.name(), b"memcmp");
    assert_eq!(descriptor.value(), expected);
    assert_eq!(descriptor.abi(), ExportAbi::X86_64FreestandingC);
    assert_eq!(descriptor.signature(), ExportSignature::Memcmp);
    assert_eq!(
        registry.admit_linux_export(b"memcmp").unwrap().value,
        expected
    );
    assert_eq!(registry.resolve_export(b"memcmp"), Ok(expected));

    let rust_registry = OstdExportRegistry::new();
    assert_eq!(
        rust_registry.admit_linux_export(OSTD_NUM_CPUS_EXPORT_NAME),
        Err(LinuxExportAdmissionError::AbiNotAdmitted)
    );
    assert_eq!(
        rust_registry.resolve_export(OSTD_NUM_CPUS_EXPORT_NAME),
        Err(ElfError::ExportNotAdmitted)
    );
    let (unknown_fixture, _, _, _) = relative_import_fixture(1);
    let unknown_layout = plan_page_layout(&unknown_fixture, BASE, MAX_SPAN, MAX_SECTIONS).unwrap();
    assert!(matches!(
        registry
            .linux_symbol_resolver(&unknown_fixture, &unknown_layout)
            .unwrap()
            .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS),
        Err(ElfError::MissingSymbol)
    ));
    let rust_fixture = rust_export_import_fixture();
    let rust_layout = plan_page_layout(&rust_fixture, BASE, MAX_SPAN, MAX_SECTIONS).unwrap();
    assert!(matches!(
        rust_registry
            .linux_symbol_resolver(&rust_fixture, &rust_layout)
            .unwrap()
            .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS),
        Err(ElfError::ExportNotAdmitted)
    ));
    let mut unknown_denied = 0usize;
    for unknown in [b"MEMCMP".as_slice(), b"memcmp\0", b"memcpy"] {
        assert_eq!(registry.describe(unknown), None);
        assert_eq!(
            registry.admit_linux_export(unknown),
            Err(LinuxExportAdmissionError::UnregisteredName)
        );
        assert_eq!(
            registry.resolve_export(unknown),
            Err(ElfError::MissingSymbol)
        );
        unknown_denied += 1;
    }
    // The standard provider remains closed to the C shim, too.
    assert_eq!(
        rust_registry.resolve_export(b"memcmp"),
        Err(ElfError::MissingSymbol)
    );

    let probe = ostd::linux_abi::run_memcmp_c_probe_for_test();
    assert_eq!(
        probe.c_calls, 16,
        "C ABI callback must enter Rust for each case"
    );
    assert_eq!(
        probe.rust_calls, 16,
        "C ABI callback must enter Rust for each case"
    );
    assert_eq!(
        probe.passed_mask, 0xffff,
        "C ABI memcmp cases must match unsigned-byte oracle"
    );
    assert!(
        probe.buffers_preserved,
        "C ABI memcmp must preserve C buffers"
    );
    println!(
        "CYBERMANJU_LINUX_C_ABI:memcmp:PASS cases={} c_calls={} rust_calls={} buffers_preserved={}",
        probe.passed_mask.count_ones(),
        probe.c_calls,
        probe.rust_calls,
        probe.buffers_preserved
    );
    println!(
        "CYBERMANJU_LINUX_C_ABI:policy:PASS admitted={} rust_denied={} unknown_denied={} signatures={}",
        exports.len(),
        usize::from(
            rust_registry.resolve_export(OSTD_NUM_CPUS_EXPORT_NAME)
                == Err(ElfError::ExportNotAdmitted)
        ),
        unknown_denied,
        (descriptor.signature() == ExportSignature::Memcmp) as usize
    );
    println!(
        "CYBERMANJU_LINUX_C_ABI:complete:PASS c_to_rust=true elf_imports_verified=false modules_executed=0 production_mapping=false wx=false smp_mapping=false"
    );
}
