// SPDX-License-Identifier: MPL-2.0
//! Strict ACPI table inventory for the direct-DMA safety decision.
//!
//! This only inventories the immediate RSDT/XSDT children; it intentionally
//! does not parse DMAR/IVRS bodies. Limits are conservative denial-of-service
//! bounds: root tables are at most 64 KiB (at most 8,187 XSDT entries), child
//! SDTs and extended RSDPs at most 1 MiB and 4 KiB respectively. On x86, ACPI
//! physical addresses are required to fit in the architectural 52-bit range.
//!
//! The firmware-provided physical addresses still rely on OSTD's existing
//! trusted-firmware/TLB linear-map assumption: `AcpiMemoryHandler` maps physical
//! memory through `paddr_to_vaddr`; the bounds below prevent unchecked length
//! and arithmetic use but cannot prove that firmware pointers refer to mapped
//! RAM.

use acpi::{AcpiHandler, rsdp::Rsdp};

use super::AcpiMemoryHandler;
use crate::{
    boot::{self, BootloaderAcpiArg},
    mm::paddr_to_vaddr,
};

const SDT_HEADER_LENGTH: usize = 36;
const RSDP_V1_LENGTH: usize = 20;
const RSDP_V2_MIN_LENGTH: usize = 36;
const MAX_RSDP_LENGTH: usize = 4 * 1024;
const MAX_ROOT_TABLE_LENGTH: usize = 64 * 1024;
const MAX_CHILD_TABLE_LENGTH: usize = 1024 * 1024;
const MAX_ROOT_ENTRIES: usize = 8 * 1024;
const MAX_ACPI_PHYSICAL_ADDRESS: usize = 1 << 52;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootKind {
    Rsdt,
    Xsdt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RootAddress {
    kind: RootKind,
    address: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SdtHeaderInfo {
    signature: [u8; 4],
    length: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::arch) enum DmaPolicyError {
    MissingAcpiRoot,
    InvalidRsdp,
    InvalidSignature,
    InvalidHeaderFields,
    InvalidLength,
    InvalidChecksum,
    InvalidRootEntries,
    InvalidPhysicalAddress,
    AddressOverflow,
}

/// Returns whether no DMAR or IVRS signature exists in a fully validated ACPI
/// root and all of its immediate child tables. Any unavailable or malformed
/// structure is an error, never evidence that direct DMA is safe.
pub(in crate::arch) fn has_no_dma_remapping_tables() -> Result<bool, DmaPolicyError> {
    let early_info = boot::EARLY_INFO
        .get()
        .ok_or(DmaPolicyError::MissingAcpiRoot)?;
    let root = match early_info.acpi_arg {
        BootloaderAcpiArg::NotProvided => return Err(DmaPolicyError::MissingAcpiRoot),
        BootloaderAcpiArg::Rsdp(address) => root_from_rsdp(address)?,
        BootloaderAcpiArg::Rsdt(address) => RootAddress {
            kind: RootKind::Rsdt,
            address,
        },
        BootloaderAcpiArg::Xsdt(address) => RootAddress {
            kind: RootKind::Xsdt,
            address,
        },
        BootloaderAcpiArg::ScanBios => {
            // SAFETY: This boot argument explicitly permits the legacy BIOS
            // RSDP scan; the ACPI crate validates the located RSDP.
            let mapping = unsafe { Rsdp::search_for_on_bios(AcpiMemoryHandler {}) }
                .map_err(|_| DmaPolicyError::MissingAcpiRoot)?;
            let physical_start = mapping.physical_start();
            drop(mapping);
            root_from_rsdp(physical_start)?
        }
    };
    inventory_root(root)
}

fn root_from_rsdp(address: usize) -> Result<RootAddress, DmaPolicyError> {
    let v1 = read_physical_array::<RSDP_V1_LENGTH>(address)?;
    if &v1[0..8] != b"RSD PTR " {
        return Err(DmaPolicyError::InvalidRsdp);
    }
    if core::str::from_utf8(&v1[9..15]).is_err() {
        return Err(DmaPolicyError::InvalidRsdp);
    }
    if checksum(&v1) != 0 {
        return Err(DmaPolicyError::InvalidChecksum);
    }
    let revision = v1[15];
    if revision == 0 {
        let rsdt_address = u32::from_le_bytes(v1[16..20].try_into().unwrap()) as usize;
        return checked_root_address(RootKind::Rsdt, rsdt_address);
    }

    let prefix = read_physical_array::<RSDP_V2_MIN_LENGTH>(address)?;
    let length = u32::from_le_bytes(prefix[20..24].try_into().unwrap()) as usize;
    if !(RSDP_V2_MIN_LENGTH..=MAX_RSDP_LENGTH).contains(&length) {
        return Err(DmaPolicyError::InvalidLength);
    }
    let bytes = read_physical_slice(address, length)?;
    if checksum(&bytes[..RSDP_V1_LENGTH]) != 0 || checksum(bytes) != 0 {
        return Err(DmaPolicyError::InvalidChecksum);
    }
    let xsdt_address = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
    let xsdt_address =
        usize::try_from(xsdt_address).map_err(|_| DmaPolicyError::InvalidPhysicalAddress)?;
    checked_root_address(RootKind::Xsdt, xsdt_address)
}

fn checked_root_address(kind: RootKind, address: usize) -> Result<RootAddress, DmaPolicyError> {
    validate_physical_region(address, SDT_HEADER_LENGTH)?;
    Ok(RootAddress { kind, address })
}

fn inventory_root(root: RootAddress) -> Result<bool, DmaPolicyError> {
    let header_bytes = read_physical_slice(root.address, SDT_HEADER_LENGTH)?;
    let expected_signature = match root.kind {
        RootKind::Rsdt => *b"RSDT",
        RootKind::Xsdt => *b"XSDT",
    };
    let header = parse_sdt_header(header_bytes, Some(expected_signature))?;
    if header.length > MAX_ROOT_TABLE_LENGTH {
        return Err(DmaPolicyError::InvalidLength);
    }
    let root_bytes = read_physical_slice(root.address, header.length)?;
    let root_header =
        validate_sdt_table(root_bytes, Some(expected_signature), MAX_ROOT_TABLE_LENGTH)?;
    let entry_size = match root.kind {
        RootKind::Rsdt => 4,
        RootKind::Xsdt => 8,
    };
    let entry_count = root_entry_count(root_header.length, entry_size)?;

    let mut found_dma_remapping = false;
    for index in 0..entry_count {
        let offset = SDT_HEADER_LENGTH
            .checked_add(
                index
                    .checked_mul(entry_size)
                    .ok_or(DmaPolicyError::AddressOverflow)?,
            )
            .ok_or(DmaPolicyError::AddressOverflow)?;
        let address = parse_root_pointer(&root_bytes[offset..offset + entry_size])?;
        validate_physical_region(address, SDT_HEADER_LENGTH)?;
        let child_header_bytes = read_physical_slice(address, SDT_HEADER_LENGTH)?;
        let child_header = parse_sdt_header(child_header_bytes, None)?;
        if child_header.length > MAX_CHILD_TABLE_LENGTH {
            return Err(DmaPolicyError::InvalidLength);
        }
        let child_bytes = read_physical_slice(address, child_header.length)?;
        let child = validate_sdt_table(
            child_bytes,
            Some(child_header.signature),
            MAX_CHILD_TABLE_LENGTH,
        )?;
        found_dma_remapping |= child.signature == *b"DMAR" || child.signature == *b"IVRS";
    }
    Ok(!found_dma_remapping)
}

/// Parses and validates a fixed-size SDT header before any table-sized mapping.
fn parse_sdt_header(
    bytes: &[u8],
    expected_signature: Option<[u8; 4]>,
) -> Result<SdtHeaderInfo, DmaPolicyError> {
    if bytes.len() < SDT_HEADER_LENGTH {
        return Err(DmaPolicyError::InvalidLength);
    }
    let signature: [u8; 4] = bytes[..4].try_into().unwrap();
    if !signature.iter().all(u8::is_ascii)
        || expected_signature.is_some_and(|expected| expected != signature)
    {
        return Err(DmaPolicyError::InvalidSignature);
    }
    if core::str::from_utf8(&bytes[10..16]).is_err()
        || core::str::from_utf8(&bytes[16..24]).is_err()
    {
        return Err(DmaPolicyError::InvalidHeaderFields);
    }
    let length = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if !(SDT_HEADER_LENGTH..=MAX_CHILD_TABLE_LENGTH).contains(&length) {
        return Err(DmaPolicyError::InvalidLength);
    }
    Ok(SdtHeaderInfo { signature, length })
}

/// Validates signature, declared length, and the checksum of a complete SDT.
fn validate_sdt_table(
    bytes: &[u8],
    expected_signature: Option<[u8; 4]>,
    max_length: usize,
) -> Result<SdtHeaderInfo, DmaPolicyError> {
    let header = parse_sdt_header(bytes, expected_signature)?;
    if header.length > max_length || bytes.len() < header.length {
        return Err(DmaPolicyError::InvalidLength);
    }
    if checksum(&bytes[..header.length]) != 0 {
        return Err(DmaPolicyError::InvalidChecksum);
    }
    Ok(header)
}

fn root_entry_count(root_length: usize, entry_size: usize) -> Result<usize, DmaPolicyError> {
    if root_length < SDT_HEADER_LENGTH || ![4, 8].contains(&entry_size) {
        return Err(DmaPolicyError::InvalidRootEntries);
    }
    let entries_length = root_length - SDT_HEADER_LENGTH;
    if entries_length % entry_size != 0 {
        return Err(DmaPolicyError::InvalidRootEntries);
    }
    let entry_count = entries_length / entry_size;
    if entry_count > MAX_ROOT_ENTRIES {
        return Err(DmaPolicyError::InvalidLength);
    }
    Ok(entry_count)
}

/// Decodes an RSDT/XSDT child address and rejects zero, truncated and oversized
/// physical pointers before they are used for a physical mapping.
fn parse_root_pointer(bytes: &[u8]) -> Result<usize, DmaPolicyError> {
    let address = match bytes.len() {
        4 => u32::from_le_bytes(bytes.try_into().unwrap()) as u64,
        8 => u64::from_le_bytes(bytes.try_into().unwrap()),
        _ => return Err(DmaPolicyError::InvalidRootEntries),
    };
    let address = usize::try_from(address).map_err(|_| DmaPolicyError::InvalidPhysicalAddress)?;
    validate_physical_region(address, SDT_HEADER_LENGTH)?;
    Ok(address)
}

fn validate_physical_region(address: usize, length: usize) -> Result<(), DmaPolicyError> {
    let linear_map_size = crate::mm::kspace::LINEAR_MAPPING_VADDR_RANGE
        .end
        .checked_sub(crate::mm::kspace::LINEAR_MAPPING_VADDR_RANGE.start)
        .ok_or(DmaPolicyError::AddressOverflow)?;
    let max_address = core::cmp::min(MAX_ACPI_PHYSICAL_ADDRESS, linear_map_size);
    if address == 0 || address >= max_address {
        return Err(DmaPolicyError::InvalidPhysicalAddress);
    }
    let end = address
        .checked_add(length)
        .ok_or(DmaPolicyError::AddressOverflow)?;
    if end > max_address {
        return Err(DmaPolicyError::InvalidPhysicalAddress);
    }
    Ok(())
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))
}

fn read_physical_array<const N: usize>(address: usize) -> Result<[u8; N], DmaPolicyError> {
    let bytes = read_physical_slice(address, N)?;
    let mut result = [0u8; N];
    result.copy_from_slice(bytes);
    Ok(result)
}

fn read_physical_slice(address: usize, length: usize) -> Result<&'static [u8], DmaPolicyError> {
    validate_physical_region(address, length)?;
    let virtual_address = paddr_to_vaddr(address);
    virtual_address
        .checked_add(length)
        .ok_or(DmaPolicyError::AddressOverflow)?;
    // SAFETY: The addresses originate in trusted boot firmware/root tables, as
    // required by AcpiMemoryHandler. Arithmetic/length bounds were checked above;
    // readable, immutable backing is the existing OSTD firmware trust assumption.
    let mapping = unsafe { AcpiMemoryHandler {}.map_physical_region::<u8>(address, length) };
    // SAFETY: Under that same assumption the complete region is readable and
    // immutable; the OSTD linear map remains alive for the kernel lifetime.
    let bytes = unsafe { core::slice::from_raw_parts(mapping.virtual_start().as_ptr(), length) };
    // The existing linear ACPI mapping is immutable and remains mapped for the
    // kernel lifetime; do not retain the temporary mapping object itself.
    core::mem::forget(mapping);
    Ok(bytes)
}

#[cfg(ktest)]
mod tests {
    use super::*;
    use crate::prelude::ktest;

    fn synthetic_sdt(signature: [u8; 4], payload: &[u8]) -> alloc::vec::Vec<u8> {
        let mut table = alloc::vec![0u8; SDT_HEADER_LENGTH];
        table[..4].copy_from_slice(&signature);
        table[4..8].copy_from_slice(&((SDT_HEADER_LENGTH + payload.len()) as u32).to_le_bytes());
        table.extend_from_slice(payload);
        let sum = checksum(&table);
        table[9] = 0u8.wrapping_sub(sum);
        table
    }

    #[ktest]
    fn sdt_parser_rejects_signature_length_and_checksum_errors() {
        let valid = synthetic_sdt(*b"TEST", &[1, 2, 3]);
        assert_eq!(
            validate_sdt_table(&valid, Some(*b"TEST"), MAX_CHILD_TABLE_LENGTH)
                .unwrap()
                .signature,
            *b"TEST"
        );
        assert_eq!(
            validate_sdt_table(&valid, Some(*b"NOPE"), MAX_CHILD_TABLE_LENGTH),
            Err(DmaPolicyError::InvalidSignature)
        );

        let mut bad_checksum = valid.clone();
        bad_checksum[SDT_HEADER_LENGTH] ^= 0x80;
        assert_eq!(
            validate_sdt_table(&bad_checksum, None, MAX_CHILD_TABLE_LENGTH),
            Err(DmaPolicyError::InvalidChecksum)
        );

        let mut short_length = valid.clone();
        short_length[4..8].copy_from_slice(&((SDT_HEADER_LENGTH - 1) as u32).to_le_bytes());
        assert_eq!(
            parse_sdt_header(&short_length, None),
            Err(DmaPolicyError::InvalidLength)
        );

        let mut excessive_length = valid;
        excessive_length[4..8]
            .copy_from_slice(&((MAX_CHILD_TABLE_LENGTH + 1) as u32).to_le_bytes());
        assert_eq!(
            parse_sdt_header(&excessive_length, None),
            Err(DmaPolicyError::InvalidLength)
        );

        let mut invalid_oem = synthetic_sdt(*b"TEST", &[]);
        invalid_oem[10] = 0xff;
        assert_eq!(
            parse_sdt_header(&invalid_oem, None),
            Err(DmaPolicyError::InvalidHeaderFields)
        );
    }

    #[ktest]
    fn root_pointer_parser_checks_width_and_address_bounds() {
        assert_eq!(parse_root_pointer(&0x1234u32.to_le_bytes()), Ok(0x1234));
        assert_eq!(parse_root_pointer(&0x1234u64.to_le_bytes()), Ok(0x1234));
        assert_eq!(
            parse_root_pointer(&[1, 2]),
            Err(DmaPolicyError::InvalidRootEntries)
        );
        assert_eq!(
            parse_root_pointer(&0u64.to_le_bytes()),
            Err(DmaPolicyError::InvalidPhysicalAddress)
        );
        assert_eq!(
            parse_root_pointer(&(MAX_ACPI_PHYSICAL_ADDRESS as u64).to_le_bytes()),
            Err(DmaPolicyError::InvalidPhysicalAddress)
        );
    }

    #[ktest]
    fn root_inventory_shape_requires_integral_pointer_entries() {
        let root = synthetic_sdt(*b"RSDT", &[1, 2, 3]);
        let header = validate_sdt_table(&root, Some(*b"RSDT"), MAX_ROOT_TABLE_LENGTH).unwrap();
        assert_eq!(
            root_entry_count(header.length, 4),
            Err(DmaPolicyError::InvalidRootEntries)
        );
        assert_eq!(
            root_entry_count(header.length, 8),
            Err(DmaPolicyError::InvalidRootEntries)
        );
        assert_eq!(root_entry_count(SDT_HEADER_LENGTH + 8, 8), Ok(1));
    }
}
