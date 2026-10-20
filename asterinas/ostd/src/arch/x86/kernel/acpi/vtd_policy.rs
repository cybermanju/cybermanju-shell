// SPDX-License-Identifier: MPL-2.0
#![expect(dead_code)]

//! Strict initial Intel VT-d topology acceptance.
//!
//! This deliberately accepts only one segment-zero DRHD and never treats
//! INCLUDE_PCI_ALL as authorization. Validated type-1 endpoint scopes authorize
//! exact paths; type-2 bridge scopes authorize strict descendants of their path,
//! but not the bridge itself. Paths are metadata supplied by the PCI owner, not
//! hardware-authenticated topology. The parser inventories and validates the
//! complete ACPI root and every immediate child; unsupported structures fail closed.

use acpi::{AcpiHandler, rsdp::Rsdp};
use spin::Once;

use super::AcpiMemoryHandler;
use crate::{
    boot::{self, BootloaderAcpiArg},
    mm::{PAGE_SIZE, kspace::LINEAR_MAPPING_VADDR_RANGE, paddr_to_vaddr},
};

const SDT_HEADER_LENGTH: usize = 36;
const RSDP_V1_LENGTH: usize = 20;
const RSDP_V2_MIN_LENGTH: usize = 36;
const MAX_RSDP_LENGTH: usize = 4 * 1024;
const MAX_ROOT_TABLE_LENGTH: usize = 64 * 1024;
const MAX_CHILD_TABLE_LENGTH: usize = 1024 * 1024;
const MAX_ROOT_ENTRIES: usize = 8 * 1024;
const MAX_ACPI_PHYSICAL_ADDRESS: usize = 1 << 52;
const DMAR_HEADER_LENGTH: usize = 48;
const DRHD_HEADER_LENGTH: usize = 16;
const DEVICE_SCOPE_HEADER_LENGTH: usize = 6;
const MAX_DEVICE_PATH_LENGTH: usize = 8;
const MAX_DEVICE_SCOPES: usize = 128;
const MCFG_HEADER_LENGTH: usize = SDT_HEADER_LENGTH + 8;
const MCFG_ENTRY_LENGTH: usize = 16;
const MCFG_LENGTH: usize = MCFG_HEADER_LENGTH + MCFG_ENTRY_LENGTH;
const ECAM_BUS_SIZE: u64 = 1 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeKind {
    Endpoint,
    Bridge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeviceScope {
    kind: ScopeKind,
    start_bus: u8,
    path: [(u8, u8); MAX_DEVICE_PATH_LENGTH],
    length: usize,
}

impl DeviceScope {
    const EMPTY: Self = Self {
        kind: ScopeKind::Endpoint,
        start_bus: 0,
        path: [(0, 0); MAX_DEVICE_PATH_LENGTH],
        length: 0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VtdPolicy {
    register_base: u64,
    include_all: bool,
    scopes: [DeviceScope; MAX_DEVICE_SCOPES],
    scope_count: usize,
}

impl VtdPolicy {
    fn allows_requester(&self, segment: u16, bus: u8, device: u8, function: u8) -> bool {
        self.allows_requester_with_path(segment, bus, device, function, &[(device, function)])
    }

    fn allows_requester_with_path(
        &self,
        segment: u16,
        bus: u8,
        device: u8,
        function: u8,
        path: &[(u8, u8)],
    ) -> bool {
        if self.include_all
            || segment != 0
            || device > 31
            || function > 7
            || path.is_empty()
            || path.len() > MAX_DEVICE_PATH_LENGTH
            || path.last() != Some(&(device, function))
            // A one-element route is a root-bus endpoint. Nonzero requester
            // buses require a genuine bridge chain from the root bus.
            || (bus == 0 && path.len() != 1)
            || (bus != 0 && path.len() < 2)
            || path.iter().any(|(dev, fun)| *dev > 31 || *fun > 7)
        {
            return false;
        }

        self.scopes[..self.scope_count]
            .iter()
            .any(|scope| match scope.kind {
                ScopeKind::Endpoint => {
                    // A root-start scope can describe the complete route. QEMU
                    // also emits one-hop endpoint scopes whose start bus is the
                    // endpoint's actual bus; match those only to the route's
                    // final requester, never to an unrelated same-BDF device.
                    (scope.start_bus == 0
                        && scope.length == path.len()
                        && scope.path[..scope.length] == path[..])
                        || (scope.start_bus == bus
                            && scope.length == 1
                            && scope.path[0] == (device, function))
                }
                // A bridge scope applies only below the bridge, never to its own BDF.
                ScopeKind::Bridge => {
                    // The route API carries BDF hops but not intermediate bus
                    // numbers, so only a scope rooted at bus zero can safely be
                    // matched as a route prefix. Other bridge scopes are parsed
                    // and retained but do not independently grant permission.
                    scope.start_bus == 0
                        && scope.length < path.len()
                        && scope.path[..scope.length] == path[..scope.length]
                }
            })
    }
}

static VALIDATED_POLICY: Once<VtdPolicy> = Once::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::arch) enum VtdPolicyError {
    MissingAcpiRoot,
    InvalidRsdp,
    InvalidSignature,
    InvalidHeaderFields,
    InvalidLength,
    InvalidChecksum,
    InvalidRootEntries,
    InvalidPhysicalAddress,
    AddressOverflow,
    MissingDmar,
    MultipleDmar,
    UnsupportedDmarStructure,
    InvalidDmarHeader,
    InvalidDrhd,
    InvalidDeviceScope,
    InvalidMcfg,
    MultipleMcfg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootKind {
    Rsdt,
    Xsdt,
}

/// Returns the accepted DMAR register base, or a descriptive error when the
/// complete ACPI inventory does not match the bounded initial VT-d policy.
pub(in crate::arch) fn validated_register_base() -> Result<u64, VtdPolicyError> {
    let early_info = boot::EARLY_INFO
        .get()
        .ok_or(VtdPolicyError::MissingAcpiRoot)?;
    let (kind, address) = match early_info.acpi_arg {
        BootloaderAcpiArg::NotProvided => return Err(VtdPolicyError::MissingAcpiRoot),
        BootloaderAcpiArg::Rsdp(address) => root_from_rsdp(address)?,
        BootloaderAcpiArg::Rsdt(address) => (RootKind::Rsdt, address),
        BootloaderAcpiArg::Xsdt(address) => (RootKind::Xsdt, address),
        BootloaderAcpiArg::ScanBios => {
            // SAFETY: This boot argument explicitly permits the legacy BIOS RSDP scan.
            let mapping = unsafe { Rsdp::search_for_on_bios(AcpiMemoryHandler {}) }
                .map_err(|_| VtdPolicyError::MissingAcpiRoot)?;
            let address = mapping.physical_start();
            drop(mapping);
            root_from_rsdp(address)?
        }
    };
    let policy = inventory_root(kind, address)?;
    // Only firmware data that has passed the complete production ACPI validation
    // path is published. Unit-test parsers never mutate this cache.
    let policy = *VALIDATED_POLICY.call_once(|| policy);
    Ok(policy.register_base)
}

/// Returns true for an explicitly scoped root-bus endpoint only.
/// An INCLUDE_PCI_ALL DRHD never widens software authorization.
pub(in crate::arch) fn requester_allowed(segment: u16, bus: u8, device: u8, function: u8) -> bool {
    VALIDATED_POLICY
        .get()
        .is_some_and(|policy| policy.allows_requester(segment, bus, device, function))
}

/// Checks a caller-resolved root-bus-to-requester route against validated DMAR
/// scopes. The PCI owner must supply the actual topology path; this function
/// only checks route metadata and does not authenticate hardware topology.
pub(in crate::arch) fn requester_allowed_with_path(
    segment: u16,
    bus: u8,
    device: u8,
    function: u8,
    path: &[(u8, u8)],
) -> bool {
    VALIDATED_POLICY.get().is_some_and(|policy| {
        policy.allows_requester_with_path(segment, bus, device, function, path)
    })
}

fn root_from_rsdp(address: usize) -> Result<(RootKind, usize), VtdPolicyError> {
    let v1 = read_physical_array::<RSDP_V1_LENGTH>(address)?;
    if &v1[..8] != b"RSD PTR " || !v1[9..15].iter().all(u8::is_ascii) {
        return Err(VtdPolicyError::InvalidRsdp);
    }
    if checksum(&v1) != 0 {
        return Err(VtdPolicyError::InvalidChecksum);
    }
    if v1[15] == 0 {
        let address = u32::from_le_bytes(v1[16..20].try_into().unwrap()) as usize;
        validate_physical_region(address, SDT_HEADER_LENGTH)?;
        return Ok((RootKind::Rsdt, address));
    }

    let prefix = read_physical_array::<RSDP_V2_MIN_LENGTH>(address)?;
    let length = u32::from_le_bytes(prefix[20..24].try_into().unwrap()) as usize;
    if !(RSDP_V2_MIN_LENGTH..=MAX_RSDP_LENGTH).contains(&length) {
        return Err(VtdPolicyError::InvalidLength);
    }
    let bytes = read_physical_slice(address, length)?;
    if checksum(&bytes[..RSDP_V1_LENGTH]) != 0 || checksum(bytes) != 0 {
        return Err(VtdPolicyError::InvalidChecksum);
    }
    let address = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
    let address = usize::try_from(address).map_err(|_| VtdPolicyError::InvalidPhysicalAddress)?;
    validate_physical_region(address, SDT_HEADER_LENGTH)?;
    Ok((RootKind::Xsdt, address))
}

fn inventory_root(kind: RootKind, address: usize) -> Result<VtdPolicy, VtdPolicyError> {
    validate_physical_region(address, SDT_HEADER_LENGTH)?;
    let expected_signature = match kind {
        RootKind::Rsdt => *b"RSDT",
        RootKind::Xsdt => *b"XSDT",
    };
    let first_header = read_physical_slice(address, SDT_HEADER_LENGTH)?;
    let root_length = parse_sdt_header(first_header, Some(expected_signature))?;
    if root_length > MAX_ROOT_TABLE_LENGTH {
        return Err(VtdPolicyError::InvalidLength);
    }
    let root = read_physical_slice(address, root_length)?;
    validate_sdt_table(root, Some(expected_signature), MAX_ROOT_TABLE_LENGTH)?;
    let entry_size = match kind {
        RootKind::Rsdt => 4,
        RootKind::Xsdt => 8,
    };
    let entries_length = root_length - SDT_HEADER_LENGTH;
    if entries_length % entry_size != 0 {
        return Err(VtdPolicyError::InvalidRootEntries);
    }
    let entry_count = entries_length / entry_size;
    if entry_count > MAX_ROOT_ENTRIES {
        return Err(VtdPolicyError::InvalidLength);
    }

    let mut found_dmar = None;
    let mut found_mcfg = false;
    for index in 0..entry_count {
        let offset = SDT_HEADER_LENGTH
            .checked_add(
                index
                    .checked_mul(entry_size)
                    .ok_or(VtdPolicyError::AddressOverflow)?,
            )
            .ok_or(VtdPolicyError::AddressOverflow)?;
        let child_address = parse_root_pointer(&root[offset..offset + entry_size])?;
        let header_bytes = read_physical_slice(child_address, SDT_HEADER_LENGTH)?;
        let child_length = parse_sdt_header(header_bytes, None)?;
        if child_length > MAX_CHILD_TABLE_LENGTH {
            return Err(VtdPolicyError::InvalidLength);
        }
        let child = read_physical_slice(child_address, child_length)?;
        let signature = validate_sdt_table(
            child,
            Some(child[..4].try_into().unwrap()),
            MAX_CHILD_TABLE_LENGTH,
        )?;
        if signature == *b"IVRS" {
            return Err(VtdPolicyError::UnsupportedDmarStructure);
        }
        if signature == *b"DMAR" {
            if found_dmar.is_some() {
                return Err(VtdPolicyError::MultipleDmar);
            }
            found_dmar = Some(parse_dmar(child)?);
        }
        if signature == *b"MCFG" {
            validate_mcfg_inventory(&mut found_mcfg, child)?;
        }
    }
    found_dmar.ok_or(VtdPolicyError::MissingDmar)
}

/// MCFG is optional for conventional CF8 access, but when present only one
/// segment-zero allocation covering buses 0..=N is accepted.
fn validate_mcfg_inventory(found: &mut bool, bytes: &[u8]) -> Result<(), VtdPolicyError> {
    if *found {
        return Err(VtdPolicyError::MultipleMcfg);
    }
    parse_mcfg(bytes)?;
    *found = true;
    Ok(())
}

fn parse_mcfg(bytes: &[u8]) -> Result<(), VtdPolicyError> {
    if bytes.len() != MCFG_LENGTH || &bytes[..4] != b"MCFG" {
        return Err(VtdPolicyError::InvalidMcfg);
    }
    let declared_length = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if declared_length != MCFG_LENGTH {
        return Err(VtdPolicyError::InvalidMcfg);
    }
    if checksum(bytes) != 0 {
        return Err(VtdPolicyError::InvalidChecksum);
    }
    let entry = &bytes[MCFG_HEADER_LENGTH..];
    let base = u64::from_le_bytes(entry[..8].try_into().unwrap());
    let segment = u16::from_le_bytes(entry[8..10].try_into().unwrap());
    let bus_start = entry[10];
    let bus_end = entry[11];
    if bytes[SDT_HEADER_LENGTH..MCFG_HEADER_LENGTH]
        .iter()
        .any(|byte| *byte != 0)
        || entry[12..].iter().any(|byte| *byte != 0)
        || segment != 0
        || bus_start != 0
        || bus_start > bus_end
        || base == 0
        || !base.is_multiple_of(ECAM_BUS_SIZE)
    {
        return Err(VtdPolicyError::InvalidMcfg);
    }
    let end = base
        .checked_add((u64::from(bus_end) + 1) * ECAM_BUS_SIZE)
        .ok_or(VtdPolicyError::InvalidMcfg)?;
    if end > MAX_ACPI_PHYSICAL_ADDRESS as u64 {
        return Err(VtdPolicyError::InvalidMcfg);
    }
    Ok(())
}

fn parse_sdt_header(
    bytes: &[u8],
    expected_signature: Option<[u8; 4]>,
) -> Result<usize, VtdPolicyError> {
    if bytes.len() < SDT_HEADER_LENGTH {
        return Err(VtdPolicyError::InvalidLength);
    }
    let signature: [u8; 4] = bytes[..4].try_into().unwrap();
    if !signature.iter().all(u8::is_ascii)
        || expected_signature.is_some_and(|expected| expected != signature)
    {
        return Err(VtdPolicyError::InvalidSignature);
    }
    if !bytes[10..16].iter().all(u8::is_ascii) || !bytes[16..24].iter().all(u8::is_ascii) {
        return Err(VtdPolicyError::InvalidHeaderFields);
    }
    let length = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if !(SDT_HEADER_LENGTH..=MAX_CHILD_TABLE_LENGTH).contains(&length) {
        return Err(VtdPolicyError::InvalidLength);
    }
    Ok(length)
}

fn validate_sdt_table(
    bytes: &[u8],
    expected_signature: Option<[u8; 4]>,
    max_length: usize,
) -> Result<[u8; 4], VtdPolicyError> {
    let length = parse_sdt_header(bytes, expected_signature)?;
    if length > max_length || bytes.len() < length {
        return Err(VtdPolicyError::InvalidLength);
    }
    if checksum(&bytes[..length]) != 0 {
        return Err(VtdPolicyError::InvalidChecksum);
    }
    Ok(bytes[..4].try_into().unwrap())
}

fn parse_dmar(bytes: &[u8]) -> Result<VtdPolicy, VtdPolicyError> {
    if bytes.len() < DMAR_HEADER_LENGTH || &bytes[..4] != b"DMAR" {
        return Err(VtdPolicyError::InvalidDmarHeader);
    }
    let declared_length = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if declared_length != bytes.len() || declared_length < DMAR_HEADER_LENGTH {
        return Err(VtdPolicyError::InvalidLength);
    }
    if checksum(bytes) != 0 {
        return Err(VtdPolicyError::InvalidChecksum);
    }
    // Host address width is encoded as width minus one; reserved DMAR flag bits and bytes
    // must be zero. (Bits 0..=2 are defined by the VT-d specification.)
    if bytes[36] > 63 || bytes[37] & !0x07 != 0 || bytes[38..48].iter().any(|byte| *byte != 0) {
        return Err(VtdPolicyError::InvalidDmarHeader);
    }

    let mut offset = DMAR_HEADER_LENGTH;
    let mut accepted_base = None;
    while offset < declared_length {
        if declared_length - offset < 4 {
            return Err(VtdPolicyError::InvalidLength);
        }
        let typ = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        let length = u16::from_le_bytes(bytes[offset + 2..offset + 4].try_into().unwrap()) as usize;
        if length < 4 || length > declared_length - offset {
            return Err(VtdPolicyError::InvalidLength);
        }
        if typ != 0 {
            // Reject RMRR, ATSR, RHSA, ANDD, SATC, SIDP and all future/unknown types.
            return Err(VtdPolicyError::UnsupportedDmarStructure);
        }
        if accepted_base.is_some() {
            return Err(VtdPolicyError::UnsupportedDmarStructure);
        }
        accepted_base = Some(parse_drhd(&bytes[offset..offset + length])?);
        offset += length;
    }
    accepted_base.ok_or(VtdPolicyError::InvalidDrhd)
}

fn parse_drhd(bytes: &[u8]) -> Result<VtdPolicy, VtdPolicyError> {
    if bytes.len() < DRHD_HEADER_LENGTH {
        return Err(VtdPolicyError::InvalidLength);
    }
    if u16::from_le_bytes(bytes[..2].try_into().unwrap()) != 0 {
        return Err(VtdPolicyError::InvalidDrhd);
    }
    let declared_length = u16::from_le_bytes(bytes[2..4].try_into().unwrap()) as usize;
    let flags = bytes[4];
    let size = bytes[5];
    let segment = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
    let register_base = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    if declared_length != bytes.len() || flags > 1 || size != 0 || segment != 0 {
        return Err(VtdPolicyError::InvalidDrhd);
    }
    let base =
        usize::try_from(register_base).map_err(|_| VtdPolicyError::InvalidPhysicalAddress)?;
    if base == 0
        || !base.is_multiple_of(PAGE_SIZE)
        || base >= MAX_ACPI_PHYSICAL_ADDRESS
        || base
            .checked_add(PAGE_SIZE)
            .is_none_or(|end| end > MAX_ACPI_PHYSICAL_ADDRESS)
    {
        return Err(VtdPolicyError::InvalidDrhd);
    }

    let mut policy = VtdPolicy {
        register_base,
        include_all: flags == 1,
        scopes: [DeviceScope::EMPTY; MAX_DEVICE_SCOPES],
        scope_count: 0,
    };
    let mut saw_ioapic = false;
    let mut offset = DRHD_HEADER_LENGTH;
    while offset < bytes.len() {
        if bytes.len() - offset < DEVICE_SCOPE_HEADER_LENGTH {
            return Err(VtdPolicyError::InvalidDeviceScope);
        }
        let length = bytes[offset + 1] as usize;
        if length < DEVICE_SCOPE_HEADER_LENGTH || length > bytes.len() - offset {
            return Err(VtdPolicyError::InvalidDeviceScope);
        }
        let scope = &bytes[offset..offset + length];
        match parse_device_scope(scope)? {
            Some(device_scope) => {
                if policy.include_all {
                    return Err(VtdPolicyError::InvalidDeviceScope);
                }
                if policy.scope_count == MAX_DEVICE_SCOPES
                    || policy.scopes[..policy.scope_count]
                        .iter()
                        .any(|previous| scopes_overlap(*previous, device_scope))
                {
                    return Err(VtdPolicyError::InvalidDeviceScope);
                }
                policy.scopes[policy.scope_count] = device_scope;
                policy.scope_count += 1;
            }
            None => {
                if saw_ioapic {
                    return Err(VtdPolicyError::InvalidDeviceScope);
                }
                saw_ioapic = true;
            }
        }
        offset += length;
    }
    Ok(policy)
}

/// Only standard endpoint and bridge scopes are retained, plus the canonical
/// IOAPIC scope which grants no PCI permission. Nonzero start buses are accepted
/// as metadata; authorization uses them only for an endpoint's exact requester
/// bus. Nonzero-bus bridge scopes cannot authorize without per-hop bus metadata.
/// Aliases and all other scope kinds remain unsupported.
fn parse_device_scope(bytes: &[u8]) -> Result<Option<DeviceScope>, VtdPolicyError> {
    if bytes.len() < DEVICE_SCOPE_HEADER_LENGTH {
        return Err(VtdPolicyError::InvalidDeviceScope);
    }
    let typ = bytes[0];
    let length = bytes[1] as usize;
    if length != bytes.len() || bytes[2] != 0 || bytes[3] != 0 || bytes[4] != 0 {
        return Err(VtdPolicyError::InvalidDeviceScope);
    }
    match typ {
        1 | 2 => {
            let path_bytes = bytes.len() - DEVICE_SCOPE_HEADER_LENGTH;
            if path_bytes == 0
                || !path_bytes.is_multiple_of(2)
                || path_bytes / 2 > MAX_DEVICE_PATH_LENGTH
            {
                return Err(VtdPolicyError::InvalidDeviceScope);
            }
            let mut path = [(0, 0); MAX_DEVICE_PATH_LENGTH];
            let length = path_bytes / 2;
            for (index, pair) in bytes[DEVICE_SCOPE_HEADER_LENGTH..]
                .chunks_exact(2)
                .enumerate()
            {
                let device = pair[0];
                let function = pair[1];
                if device > 31 || function > 7 {
                    return Err(VtdPolicyError::InvalidDeviceScope);
                }
                path[index] = (device, function);
            }
            Ok(Some(DeviceScope {
                kind: if typ == 1 {
                    ScopeKind::Endpoint
                } else {
                    ScopeKind::Bridge
                },
                start_bus: bytes[5],
                path,
                length,
            }))
        }
        3 if bytes.len() == DEVICE_SCOPE_HEADER_LENGTH + 2
            && bytes[5] == 0xff
            && bytes[6] == 0
            && bytes[7] == 0 =>
        {
            Ok(None)
        }
        _ => Err(VtdPolicyError::InvalidDeviceScope),
    }
}

fn scopes_overlap(left: DeviceScope, right: DeviceScope) -> bool {
    // Start-bus numbers identify the bus on which each scope's path begins;
    // same BDF sequences rooted at different buses are distinct scope objects.
    if left.start_bus != right.start_bus {
        return false;
    }
    let common = core::cmp::min(left.length, right.length);
    if left.path[..common] != right.path[..common] {
        return false;
    }
    match (left.kind, right.kind) {
        (ScopeKind::Endpoint, ScopeKind::Endpoint) => left.length == right.length,
        (ScopeKind::Bridge, ScopeKind::Bridge) => true,
        (ScopeKind::Bridge, ScopeKind::Endpoint) => left.length < right.length,
        (ScopeKind::Endpoint, ScopeKind::Bridge) => right.length < left.length,
    }
}

fn parse_root_pointer(bytes: &[u8]) -> Result<usize, VtdPolicyError> {
    let address = match bytes.len() {
        4 => u32::from_le_bytes(bytes.try_into().unwrap()) as u64,
        8 => u64::from_le_bytes(bytes.try_into().unwrap()),
        _ => return Err(VtdPolicyError::InvalidRootEntries),
    };
    let address = usize::try_from(address).map_err(|_| VtdPolicyError::InvalidPhysicalAddress)?;
    validate_physical_region(address, SDT_HEADER_LENGTH)?;
    Ok(address)
}

fn validate_physical_region(address: usize, length: usize) -> Result<(), VtdPolicyError> {
    let linear_map_size = LINEAR_MAPPING_VADDR_RANGE
        .end
        .checked_sub(LINEAR_MAPPING_VADDR_RANGE.start)
        .ok_or(VtdPolicyError::AddressOverflow)?;
    let max_address = core::cmp::min(MAX_ACPI_PHYSICAL_ADDRESS, linear_map_size);
    if address == 0 || address >= max_address {
        return Err(VtdPolicyError::InvalidPhysicalAddress);
    }
    let end = address
        .checked_add(length)
        .ok_or(VtdPolicyError::AddressOverflow)?;
    if end > max_address {
        return Err(VtdPolicyError::InvalidPhysicalAddress);
    }
    Ok(())
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))
}

fn read_physical_array<const N: usize>(address: usize) -> Result<[u8; N], VtdPolicyError> {
    let bytes = read_physical_slice(address, N)?;
    let mut result = [0u8; N];
    result.copy_from_slice(bytes);
    Ok(result)
}

fn read_physical_slice(address: usize, length: usize) -> Result<&'static [u8], VtdPolicyError> {
    validate_physical_region(address, length)?;
    paddr_to_vaddr(address)
        .checked_add(length)
        .ok_or(VtdPolicyError::AddressOverflow)?;
    // SAFETY: As with the ACPI parser, firmware-provided addresses rely on OSTD's existing
    // immutable linear-map/trusted-firmware assumption. All lengths and arithmetic are bounded.
    let mapping = unsafe { AcpiMemoryHandler {}.map_physical_region::<u8>(address, length) };
    // SAFETY: The validated linear mapping remains live for the kernel lifetime.
    let bytes = unsafe { core::slice::from_raw_parts(mapping.virtual_start().as_ptr(), length) };
    core::mem::forget(mapping);
    Ok(bytes)
}

#[cfg(ktest)]
mod tests {
    use super::*;
    use crate::prelude::ktest;

    fn synthetic_dmar(remapping: &[u8]) -> alloc::vec::Vec<u8> {
        let mut table = alloc::vec![0u8; DMAR_HEADER_LENGTH];
        table[..4].copy_from_slice(b"DMAR");
        table[36] = 39;
        table[4..8].copy_from_slice(&((DMAR_HEADER_LENGTH + remapping.len()) as u32).to_le_bytes());
        table.extend_from_slice(remapping);
        let sum = checksum(&table);
        table[9] = 0u8.wrapping_sub(sum);
        table
    }

    fn synthetic_mcfg() -> alloc::vec::Vec<u8> {
        let mut table = alloc::vec![0u8; MCFG_LENGTH];
        table[..4].copy_from_slice(b"MCFG");
        table[4..8].copy_from_slice(&(MCFG_LENGTH as u32).to_le_bytes());
        let entry = &mut table[MCFG_HEADER_LENGTH..];
        // Q35's common ECAM aperture: segment 0, buses 0 through 255.
        entry[..8].copy_from_slice(&0xb000_0000u64.to_le_bytes());
        entry[10] = 0;
        entry[11] = 255;
        reseal(&mut table);
        table
    }

    fn reseal(table: &mut [u8]) {
        table[9] = 0;
        table[9] = 0u8.wrapping_sub(checksum(table));
    }

    fn synthetic_drhd(flags: u8, segment: u16, scopes: &[u8], base: u64) -> alloc::vec::Vec<u8> {
        let mut drhd = alloc::vec![0u8; DRHD_HEADER_LENGTH];
        drhd[2..4].copy_from_slice(&((DRHD_HEADER_LENGTH + scopes.len()) as u16).to_le_bytes());
        drhd[4] = flags;
        drhd[6..8].copy_from_slice(&segment.to_le_bytes());
        drhd[8..16].copy_from_slice(&base.to_le_bytes());
        drhd.extend_from_slice(scopes);
        drhd
    }

    fn scope(typ: u8, bus: u8, path: &[(u8, u8)]) -> alloc::vec::Vec<u8> {
        let mut bytes = alloc::vec![
            typ,
            (DEVICE_SCOPE_HEADER_LENGTH + 2 * path.len()) as u8,
            0,
            0,
            0,
            bus,
        ];
        for (device, function) in path {
            bytes.push(*device);
            bytes.push(*function);
        }
        bytes
    }

    fn policy(flags: u8, scopes: &[u8]) -> Result<VtdPolicy, VtdPolicyError> {
        parse_dmar(&synthetic_dmar(&synthetic_drhd(
            flags,
            0,
            scopes,
            0xfed9_0000,
        )))
    }

    #[ktest]
    fn accepts_only_one_strict_first_region_mcfg() {
        let valid = synthetic_mcfg();
        assert_eq!(parse_mcfg(&valid), Ok(()));
        let mut found = false;
        assert_eq!(validate_mcfg_inventory(&mut found, &valid), Ok(()));
        assert!(found);
        assert_eq!(
            validate_mcfg_inventory(&mut found, &valid),
            Err(VtdPolicyError::MultipleMcfg)
        );

        let mut multi_entry = valid.clone();
        multi_entry.extend_from_slice(&[0; MCFG_ENTRY_LENGTH]);
        let multi_entry_length = multi_entry.len() as u32;
        multi_entry[4..8].copy_from_slice(&multi_entry_length.to_le_bytes());
        reseal(&mut multi_entry);
        assert_eq!(parse_mcfg(&multi_entry), Err(VtdPolicyError::InvalidMcfg));

        for (offset, value) in [(52, 1), (54, 1), (36, 1), (56, 1)] {
            let mut invalid = valid.clone();
            invalid[offset] = value;
            reseal(&mut invalid);
            assert_eq!(parse_mcfg(&invalid), Err(VtdPolicyError::InvalidMcfg));
        }

        let mut invalid_range = valid.clone();
        invalid_range[54] = 1;
        invalid_range[55] = 0;
        reseal(&mut invalid_range);
        assert_eq!(parse_mcfg(&invalid_range), Err(VtdPolicyError::InvalidMcfg));

        let mut unaligned = valid.clone();
        unaligned[MCFG_HEADER_LENGTH] = 1;
        reseal(&mut unaligned);
        assert_eq!(parse_mcfg(&unaligned), Err(VtdPolicyError::InvalidMcfg));

        let mut zero_base = valid.clone();
        zero_base[MCFG_HEADER_LENGTH..MCFG_HEADER_LENGTH + 8].fill(0);
        reseal(&mut zero_base);
        assert_eq!(parse_mcfg(&zero_base), Err(VtdPolicyError::InvalidMcfg));

        let mut out_of_bounds = valid.clone();
        let base = MAX_ACPI_PHYSICAL_ADDRESS as u64 - ECAM_BUS_SIZE;
        out_of_bounds[MCFG_HEADER_LENGTH..MCFG_HEADER_LENGTH + 8]
            .copy_from_slice(&base.to_le_bytes());
        out_of_bounds[55] = 1;
        reseal(&mut out_of_bounds);
        assert_eq!(parse_mcfg(&out_of_bounds), Err(VtdPolicyError::InvalidMcfg));

        let mut overflowing = valid.clone();
        overflowing[MCFG_HEADER_LENGTH..MCFG_HEADER_LENGTH + 8]
            .copy_from_slice(&0xffff_ffff_fff0_0000u64.to_le_bytes());
        reseal(&mut overflowing);
        assert_eq!(parse_mcfg(&overflowing), Err(VtdPolicyError::InvalidMcfg));

        let mut truncated = valid;
        truncated.pop();
        assert_eq!(parse_mcfg(&truncated), Err(VtdPolicyError::InvalidMcfg));
    }

    #[ktest]
    fn accepts_qemu_ioapic_and_exact_endpoint_membership() {
        let mut scopes = scope(3, 0xff, &[(0, 0)]);
        scopes.extend_from_slice(&scope(1, 0, &[(0, 0)]));
        scopes.extend_from_slice(&scope(1, 0, &[(1, 0)]));
        scopes.extend_from_slice(&scope(1, 0, &[(31, 2)]));
        let policy = policy(0, &scopes).unwrap();
        assert_eq!(policy.register_base, 0xfed9_0000);
        assert!(!policy.include_all);
        assert!(policy.allows_requester(0, 0, 0, 0));
        assert!(policy.allows_requester(0, 0, 1, 0));
        assert!(policy.allows_requester(0, 0, 31, 2));
        assert!(!policy.allows_requester(0, 0, 1, 1));
        assert!(!policy.allows_requester(0, 1, 1, 0));
        assert!(!policy.allows_requester(1, 0, 1, 0));
        assert!(!policy.allows_requester(0, 0, 32, 0));
    }

    #[ktest]
    fn endpoint_path_is_exact_and_bridge_scope_excludes_the_bridge_itself() {
        let mut scopes = scope(1, 0, &[(2, 0), (4, 1)]);
        scopes.extend_from_slice(&scope(2, 0, &[(5, 0)]));
        let policy = policy(0, &scopes).unwrap();

        assert!(policy.allows_requester_with_path(0, 2, 4, 1, &[(2, 0), (4, 1)]));
        assert!(!policy.allows_requester_with_path(0, 2, 4, 0, &[(2, 0), (4, 1)]));
        assert!(!policy.allows_requester_with_path(0, 2, 4, 1, &[(2, 0), (4, 1), (6, 0)]));
        assert!(policy.allows_requester_with_path(0, 3, 7, 2, &[(5, 0), (7, 2)]));
        assert!(!policy.allows_requester_with_path(0, 0, 5, 0, &[(5, 0)]));
        assert!(!policy.allows_requester_with_path(0, 3, 7, 2, &[(6, 0), (7, 2)]));
        assert!(!policy.allows_requester_with_path(1, 3, 7, 2, &[(5, 0), (7, 2)]));
        assert!(!policy.allows_requester_with_path(0, 3, 7, 2, &[(5, 0)]));
    }

    #[ktest]
    fn matches_qemu_one_hop_endpoint_scope_using_requester_bus() {
        // QEMU 7.2 emits a bridge scope on its parent bus and a one-hop endpoint
        // scope on the endpoint's own bus. The root bridge scope admits the
        // subtree; the endpoint scope independently matches only its final BDF.
        let mut scopes = scope(2, 0, &[(5, 0)]);
        scopes.extend_from_slice(&scope(1, 2, &[(7, 1)]));
        let combined = policy(0, &scopes).unwrap();
        assert!(combined.allows_requester_with_path(0, 2, 7, 1, &[(5, 0), (7, 1)]));
        // The bridge scope authorizes strict descendants. The PCI owner, not DMAR,
        // verifies the live bus numbers along the supplied root-to-endpoint route.
        assert!(combined.allows_requester_with_path(0, 3, 7, 1, &[(5, 0), (7, 1)]));
        let endpoint = policy(0, &scope(1, 2, &[(7, 1)])).unwrap();
        assert!(endpoint.allows_requester_with_path(0, 2, 7, 1, &[(5, 0), (7, 1)]));
        assert!(!endpoint.allows_requester_with_path(0, 3, 7, 1, &[(5, 0), (7, 1)]));
        assert!(!endpoint.allows_requester_with_path(0, 2, 7, 0, &[(5, 0), (7, 1)]));

        // A nonzero-bus bridge scope is accepted, but cannot independently
        // authorize because this API does not carry intermediate bus numbers.
        let nonzero_bridge = policy(0, &scope(2, 2, &[(5, 0)])).unwrap();
        assert!(!nonzero_bridge.allows_requester_with_path(0, 3, 7, 1, &[(5, 0), (7, 1)]));
    }

    #[ktest]
    fn include_all_is_accepted_but_never_grants_implicit_requester_membership() {
        let accepted = policy(1, &[]).unwrap();
        assert!(accepted.include_all);
        assert!(!accepted.allows_requester(0, 0, 0, 0));
        assert_eq!(
            policy(1, &scope(1, 0, &[(1, 0)])),
            Err(VtdPolicyError::InvalidDeviceScope)
        );
    }

    #[ktest]
    fn rejects_second_drhd_and_all_non_drhd_remapping_types() {
        let one = synthetic_drhd(0, 0, &[], 0xfed9_0000);
        let mut two = one.clone();
        two.extend_from_slice(&one);
        assert_eq!(
            parse_dmar(&synthetic_dmar(&two)),
            Err(VtdPolicyError::UnsupportedDmarStructure)
        );
        for typ in 1u16..=7 {
            let mut structure = alloc::vec![0, 0, 4, 0];
            structure[..2].copy_from_slice(&typ.to_le_bytes());
            assert_eq!(
                parse_dmar(&synthetic_dmar(&structure)),
                Err(VtdPolicyError::UnsupportedDmarStructure)
            );
        }
    }

    #[ktest]
    fn rejects_nonzero_segment_reserved_flags_and_unsupported_scopes() {
        assert_eq!(
            parse_dmar(&synthetic_dmar(&synthetic_drhd(2, 0, &[], 0xfed9_0000))),
            Err(VtdPolicyError::InvalidDrhd)
        );
        assert_eq!(
            parse_dmar(&synthetic_dmar(&synthetic_drhd(0, 1, &[], 0xfed9_0000))),
            Err(VtdPolicyError::InvalidDrhd)
        );
        for (typ, bus, path) in [(4, 0, &[(1, 0)][..]), (3, 0xff, &[(1, 0)][..])] {
            let bytes = scope(typ, bus, path);
            assert_eq!(policy(0, &bytes), Err(VtdPolicyError::InvalidDeviceScope));
        }
        let mut duplicate = scope(1, 0, &[(1, 0)]);
        duplicate.extend_from_slice(&scope(1, 0, &[(1, 0)]));
        assert_eq!(
            policy(0, &duplicate),
            Err(VtdPolicyError::InvalidDeviceScope)
        );
        let mut repeated_ioapic = scope(3, 0xff, &[(0, 0)]);
        repeated_ioapic.extend_from_slice(&scope(3, 0xff, &[(0, 0)]));
        assert_eq!(
            policy(0, &repeated_ioapic),
            Err(VtdPolicyError::InvalidDeviceScope)
        );
        let mut duplicate_bridge = scope(2, 0, &[(1, 0)]);
        duplicate_bridge.extend_from_slice(&scope(2, 0, &[(1, 0)]));
        assert_eq!(
            policy(0, &duplicate_bridge),
            Err(VtdPolicyError::InvalidDeviceScope)
        );
        let mut nested_overlapping_bridges = scope(2, 0, &[(1, 0)]);
        nested_overlapping_bridges.extend_from_slice(&scope(2, 0, &[(1, 0), (2, 0)]));
        assert_eq!(
            policy(0, &nested_overlapping_bridges),
            Err(VtdPolicyError::InvalidDeviceScope)
        );
        let mut endpoint_inside_bridge = scope(2, 0, &[(1, 0)]);
        endpoint_inside_bridge.extend_from_slice(&scope(1, 0, &[(1, 0), (2, 0)]));
        assert_eq!(
            policy(0, &endpoint_inside_bridge),
            Err(VtdPolicyError::InvalidDeviceScope)
        );
        assert_eq!(
            parse_device_scope(&scope(1, 0, &[(32, 0)])),
            Err(VtdPolicyError::InvalidDeviceScope)
        );
        assert_eq!(
            parse_device_scope(&scope(1, 0, &[(1, 8)])),
            Err(VtdPolicyError::InvalidDeviceScope)
        );
        assert_eq!(
            parse_device_scope(&scope(1, 0, &[(1, 0); MAX_DEVICE_PATH_LENGTH + 1])),
            Err(VtdPolicyError::InvalidDeviceScope)
        );
        assert_eq!(
            parse_device_scope(&scope(1, 0, &[])),
            Err(VtdPolicyError::InvalidDeviceScope)
        );
    }

    #[ktest]
    fn rejects_truncated_invalid_dmar_and_invalid_bases_or_scope_lengths() {
        let table = synthetic_dmar(&[]);
        let mut bad_checksum = table.clone();
        bad_checksum[DMAR_HEADER_LENGTH - 1] ^= 1;
        assert_eq!(
            parse_dmar(&bad_checksum),
            Err(VtdPolicyError::InvalidChecksum)
        );
        let mut truncated = table;
        truncated[4..8].copy_from_slice(&((DMAR_HEADER_LENGTH - 1) as u32).to_le_bytes());
        assert_eq!(parse_dmar(&truncated), Err(VtdPolicyError::InvalidLength));
        assert_eq!(
            parse_dmar(&synthetic_dmar(&[0, 0, 3, 0])),
            Err(VtdPolicyError::InvalidLength)
        );
        let zero_base = synthetic_drhd(0, 0, &[], 0);
        assert_eq!(
            parse_dmar(&synthetic_dmar(&zero_base)),
            Err(VtdPolicyError::InvalidDrhd)
        );
        let unaligned = synthetic_drhd(0, 0, &[], 0xfed9_0001);
        assert_eq!(
            parse_dmar(&synthetic_dmar(&unaligned)),
            Err(VtdPolicyError::InvalidDrhd)
        );
        let mut bad_scope = scope(1, 0, &[(1, 0)]);
        bad_scope[1] = 7;
        assert_eq!(
            policy(0, &bad_scope),
            Err(VtdPolicyError::InvalidDeviceScope)
        );
        let mut bad_header = scope(1, 0, &[(1, 0)]);
        bad_header[3] = 1;
        assert_eq!(
            policy(0, &bad_header),
            Err(VtdPolicyError::InvalidDeviceScope)
        );
    }
}
