// SPDX-License-Identifier: MPL-2.0
//! Read-only, fail-closed PCI requester/topology admission for isolated DMA.
//!
//! Supported PCIe topology is intentionally limited to segment-zero type-0
//! endpoints in slot zero immediately below one genuine PCIe root port
//! on bus zero. Switches, nested bridges, ambiguous bus ranges, ARI/SR-IOV and
//! device translation services are not admitted. This module never writes config.
use ostd::mm::dma::DeviceDmaError;

use crate::PciDeviceLocation;

const PCI_STATUS_CAPABILITIES: u16 = 1 << 4;
const PCI_HEADER_MULTIFUNCTION: u8 = 1 << 7;
const PCI_CAP_ID_EXPRESS: u8 = 0x10;
const PCI_CLASS_BRIDGE: u8 = 0x06;
const PCI_SUBCLASS_PCI_BRIDGE: u8 = 0x04;
const PCI_EXP_TYPE_ENDPOINT: u8 = 0;
const PCI_EXP_TYPE_ROOT_PORT: u8 = 4;
const PCI_EXT_CAP_ID_ACS: u16 = 0x000d;
const PCI_EXT_CAP_ID_ARI: u16 = 0x000e;
const PCI_EXT_CAP_ID_SRIOV: u16 = 0x0010;
const PCI_EXT_CAP_ID_ATS: u16 = 0x000f;
const PCI_EXT_CAP_ID_PRI: u16 = 0x0013;
const PCI_EXT_CAP_ID_PASID: u16 = 0x001b;
const MAX_EXT_CAPS: usize = 960;

/// Validates a DMA requester and returns its reviewed PCIe isolation path.
///
/// `None` means the narrowly retained conventional bus-zero endpoint path;
/// `Some(path)` is a PCIe root-port-to-endpoint path whose elements are
/// `(device, function)` and are passed unchanged to the IOMMU domain constructor.
pub(crate) fn admit(location: PciDeviceLocation) -> Result<Option<[(u8, u8); 2]>, DeviceDmaError> {
    admit_function(location, false)
}

/// Enumerates all eight function numbers, independent of the multifunction bit.
/// Presence, header and BME checks are read-only; this is not device shutdown.
pub(crate) fn present_slot_functions(
    location: PciDeviceLocation,
) -> Result<alloc::vec::Vec<PciDeviceLocation>, DeviceDmaError> {
    if location.device > 31 || location.function != 0 || read_u16(location, 0)? == 0xffff {
        return Err(DeviceDmaError::InvalidRequester);
    }
    let mut present = alloc::vec::Vec::new();
    for function in 0..=7 {
        let loc = PciDeviceLocation {
            function,
            ..location
        };
        if read_u16(loc, 0)? == 0xffff {
            continue;
        }
        if read_u8(loc, 0x0e)? & 0x7f != 0 || read_u8(loc, 0x0b)? == PCI_CLASS_BRIDGE {
            return Err(DeviceDmaError::UnsupportedPlatform);
        }
        if read_u16(loc, 4)? & crate::cfg_space::Command::BUS_MASTER.bits() != 0 {
            return Err(DeviceDmaError::InvalidContext);
        }
        present.push(loc);
    }
    if present.len() > 1 && read_u8(location, 0x0e)? & PCI_HEADER_MULTIFUNCTION == 0 {
        return Err(DeviceDmaError::InvalidRequester);
    }
    Ok(present)
}

/// The owner supplies the complete, exclusively reserved slot, not a subset.
/// Each function independently passes PCIe/ACS validation. Internal peer routing
/// is permitted only because all functions share one domain and one owner.
pub(crate) fn admit_group(
    locations: &[PciDeviceLocation],
) -> Result<alloc::vec::Vec<[(u8, u8); 2]>, DeviceDmaError> {
    let first = *locations.first().ok_or(DeviceDmaError::InvalidRequester)?;
    if present_slot_functions(first)?.as_slice() != locations {
        return Err(DeviceDmaError::InvalidRequester);
    }
    let mut routes = alloc::vec::Vec::with_capacity(locations.len());
    for loc in locations {
        let route = admit_function(*loc, true)?.ok_or(DeviceDmaError::UnsupportedPlatform)?;
        if routes
            .first()
            .is_some_and(|previous: &[(u8, u8); 2]| previous[0] != route[0])
        {
            return Err(DeviceDmaError::InvalidRequester);
        }
        routes.push(route);
    }
    if present_slot_functions(first)?.as_slice() != locations {
        return Err(DeviceDmaError::InvalidContext);
    }
    Ok(routes)
}

fn admit_function(
    location: PciDeviceLocation,
    grouped: bool,
) -> Result<Option<[(u8, u8); 2]>, DeviceDmaError> {
    if read_u16(location, 0)? == 0xffff {
        return Err(DeviceDmaError::InvalidRequester);
    }
    if location.device > 31 || location.function > 7 {
        return Err(DeviceDmaError::InvalidRequester);
    }
    if read_u16(location, 4)? & crate::cfg_space::Command::BUS_MASTER.bits() != 0 {
        return Err(DeviceDmaError::InvalidContext);
    }

    let header = read_u8(location, 0x0e)?;
    if header & 0x7f != 0 || (!grouped && location.function != 0) {
        return Err(DeviceDmaError::UnsupportedPlatform);
    }
    if read_u8(location, 0x0b)? == PCI_CLASS_BRIDGE {
        return Err(DeviceDmaError::UnsupportedPlatform);
    }
    if !grouped {
        reject_function_aliases(location)?;
    }

    let pcie_cap = conventional_pcie_capability(location)?;
    match pcie_cap {
        None => {
            // Preserve only the existing conventional EDU-like root-bus case.
            if location.bus != 0 || grouped {
                return Err(DeviceDmaError::UnsupportedPlatform);
            }
            // Legacy CF8/CFC systems cannot address extended configuration
            // space; the absent PCIe capability is then the applicable limit.
            // If ECAM does expose it, validate any advertised list rather than
            // treating inaccessible or malformed PCIe metadata as safe.
            if let Ok(header) = read_u32(location, 0x100)
                && header != 0
                && header != u32::MAX
            {
                validate_extended_capabilities(location, false)?;
            }
            Ok(None)
        }
        Some(cap) => {
            if location.bus == 0 {
                return Err(DeviceDmaError::UnsupportedPlatform);
            }
            let ecam = ostd::arch::kernel::ACPI_INFO
                .get()
                .and_then(|info| info.pci_ecam_region.as_ref())
                .ok_or(DeviceDmaError::UnsupportedPlatform)?;
            // VT-d's complete ACPI inventory separately requires one segment-zero
            // MCFG allocation; the PCI backend currently exposes only that region.
            if ecam.bus_start != 0 || location.bus > ecam.bus_end {
                return Err(DeviceDmaError::UnsupportedPlatform);
            }
            let pcie_header = read_u16(location, cap + 2)?;
            let port_type = ((pcie_header >> 4) & 0xf) as u8;
            if port_type != PCI_EXP_TYPE_ENDPOINT
                || location.device != 0
                || (!grouped && location.function != 0)
            {
                // Reject legacy endpoints, integrated endpoints, root complexes,
                // multifunction-like aliases, and all bridge/device port types.
                return Err(DeviceDmaError::UnsupportedPlatform);
            }
            validate_extended_capabilities(location, false)?;
            find_unique_root_port_route(location)
        }
    }
}

fn read_u8(location: PciDeviceLocation, offset: u16) -> Result<u8, DeviceDmaError> {
    let dword = read_u32(location, offset & !3)?;
    Ok((dword >> ((offset as u32 & 3) * 8)) as u8)
}

fn read_u16(location: PciDeviceLocation, offset: u16) -> Result<u16, DeviceDmaError> {
    let dword = read_u32(location, offset & !3)?;
    Ok((dword >> ((offset as u32 & 2) * 8)) as u16)
}

/// Unlike `PciDeviceLocation::read32`, this returns inaccessible ECAM errors
/// rather than unwrapping them. Offsets above 0xff are therefore fail-closed on
/// CF8/CFC-only systems, while x86 ECAM can safely expose the full 4 KiB space.
fn read_u32(location: PciDeviceLocation, offset: u16) -> Result<u32, DeviceDmaError> {
    crate::arch::read32(&location, u32::from(offset))
        .map(u32::from_le)
        .map_err(|_| DeviceDmaError::UnsupportedPlatform)
}

fn conventional_pcie_capability(
    location: PciDeviceLocation,
) -> Result<Option<u16>, DeviceDmaError> {
    if read_u16(location, 6)? & PCI_STATUS_CAPABILITIES == 0 {
        return Ok(None);
    }
    let mut pointer = read_u8(location, 0x34)?;
    if pointer == 0 {
        return Err(DeviceDmaError::InvalidRequester);
    }
    let mut visited = 0u64;
    let mut pcie = None;
    while pointer != 0 {
        if !(0x40..=0xfc).contains(&pointer) || pointer & 3 != 0 {
            return Err(DeviceDmaError::InvalidRequester);
        }
        let bit = 1u64 << (pointer / 4);
        if visited & bit != 0 {
            return Err(DeviceDmaError::InvalidRequester);
        }
        visited |= bit;
        let id = read_u8(location, u16::from(pointer))?;
        if id == PCI_CAP_ID_EXPRESS {
            if pointer > 0xc4 {
                return Err(DeviceDmaError::InvalidRequester);
            }
            if pcie.replace(u16::from(pointer)).is_some() {
                return Err(DeviceDmaError::InvalidRequester);
            }
        }
        pointer = read_u8(location, u16::from(pointer) + 1)?;
    }
    Ok(pcie)
}

fn reject_function_aliases(location: PciDeviceLocation) -> Result<(), DeviceDmaError> {
    // Function-number aliases are unsafe even when function zero forgot to set
    // the multifunction header bit. Probe all functions rather than trusting it.
    for function in 1..=7 {
        let other = PciDeviceLocation {
            function,
            ..location
        };
        if read_u16(other, 0)? != 0xffff {
            return Err(DeviceDmaError::UnsupportedPlatform);
        }
    }
    if read_u8(location, 0x0e)? & PCI_HEADER_MULTIFUNCTION != 0 {
        return Err(DeviceDmaError::UnsupportedPlatform);
    }
    Ok(())
}

/// Walk a complete 4-KiB PCIe extended-capability list. Any broken pointer,
/// cycle, inaccessible config space, or requester-translation/alias service is
/// a hard rejection; absence of an ECAM mapping is never interpreted as absence
/// of such a capability.
fn validate_extended_capabilities(
    location: PciDeviceLocation,
    is_root_port: bool,
) -> Result<Option<(u16, u32, u32)>, DeviceDmaError> {
    let mut offset = 0x100u16;
    let mut visited = [0u64; 15]; // 960 dword slots in 0x100..=0xffc.
    let mut acs = None;
    for _ in 0..MAX_EXT_CAPS {
        let header = read_u32(location, offset)?;
        if header == 0 {
            return if offset == 0x100 {
                Ok(acs)
            } else {
                Err(DeviceDmaError::InvalidRequester)
            };
        }
        let id = header as u16;
        let version = (header >> 16) & 15;
        if id == 0 || id == 0xffff || version == 0 {
            return Err(DeviceDmaError::InvalidRequester);
        }
        let next = ((header >> 20) & 0xfff) as u16;
        let slot = usize::from((offset - 0x100) / 4);
        let bit = 1u64 << (slot % 64);
        let word = slot / 64;
        if visited[word] & bit != 0 {
            return Err(DeviceDmaError::InvalidRequester);
        }
        visited[word] |= bit;
        if matches!(
            id,
            PCI_EXT_CAP_ID_ATS
                | PCI_EXT_CAP_ID_PRI
                | PCI_EXT_CAP_ID_PASID
                | PCI_EXT_CAP_ID_ARI
                | PCI_EXT_CAP_ID_SRIOV
        ) {
            return Err(DeviceDmaError::UnsupportedPlatform);
        }
        if id == PCI_EXT_CAP_ID_ACS {
            if !is_root_port || acs.is_some() || version != 1 {
                return Err(DeviceDmaError::UnsupportedPlatform);
            }
            let capability = read_u16(location, offset + 4)?;
            let control = read_u16(location, offset + 6)?;
            acs = Some((offset, u32::from(capability), u32::from(control)));
        }
        if next == 0 {
            return Ok(acs);
        }
        if next < 0x100 || next > 0xffc || next & 3 != 0 {
            return Err(DeviceDmaError::InvalidRequester);
        }
        offset = next;
    }
    Err(DeviceDmaError::InvalidRequester)
}

#[derive(Clone, Copy)]
struct RootPort {
    location: PciDeviceLocation,
    secondary: u8,
    subordinate: u8,
}

fn find_unique_root_port_route(
    endpoint: PciDeviceLocation,
) -> Result<Option<[(u8, u8); 2]>, DeviceDmaError> {
    let mut candidate = None;
    for device in 0..=31 {
        let fn0 = PciDeviceLocation {
            bus: 0,
            device,
            function: 0,
        };
        let vendor = read_u16(fn0, 0)?;
        if vendor == 0xffff {
            continue;
        }
        let header0 = read_u8(fn0, 0x0e)?;
        for function in 0..=7 {
            if function > 0 && header0 & PCI_HEADER_MULTIFUNCTION == 0 {
                let other = PciDeviceLocation { function, ..fn0 };
                if read_u16(other, 0)? != 0xffff {
                    return Err(DeviceDmaError::InvalidRequester);
                }
                continue;
            }
            let bridge = PciDeviceLocation { function, ..fn0 };
            if read_u16(bridge, 0)? == 0xffff {
                continue;
            }
            let hdr = read_u8(bridge, 0x0e)?;
            let class = read_u8(bridge, 0x0b)?;
            let subclass = read_u8(bridge, 0x0a)?;
            if class == PCI_CLASS_BRIDGE && subclass == PCI_SUBCLASS_PCI_BRIDGE && hdr & 0x7f != 1 {
                return Err(DeviceDmaError::UnsupportedPlatform);
            }
            if class != PCI_CLASS_BRIDGE || subclass != PCI_SUBCLASS_PCI_BRIDGE || hdr & 0x7f != 1 {
                continue;
            }
            let buses = read_u32(bridge, 0x18)?;
            let primary = buses as u8;
            let secondary = (buses >> 8) as u8;
            let subordinate = (buses >> 16) as u8;
            if primary != 0 || secondary == 0 || secondary > subordinate {
                return Err(DeviceDmaError::InvalidRequester);
            }
            if endpoint.bus < secondary || endpoint.bus > subordinate {
                continue;
            }
            if secondary != endpoint.bus || subordinate != endpoint.bus {
                // No nested bridges, overlapping bus-number windows or switches.
                return Err(DeviceDmaError::UnsupportedPlatform);
            }
            let pcie =
                conventional_pcie_capability(bridge)?.ok_or(DeviceDmaError::UnsupportedPlatform)?;
            let pcie_header = read_u16(bridge, pcie + 2)?;
            if ((pcie_header >> 4) & 0xf) as u8 != PCI_EXP_TYPE_ROOT_PORT {
                return Err(DeviceDmaError::UnsupportedPlatform);
            }
            validate_extended_capabilities(bridge, true)?;
            validate_acs(bridge)?;
            // ARI forwarding can add requester aliases beyond ordinary devfn.
            if read_u16(bridge, pcie + 0x28)? & (1 << 5) != 0 {
                return Err(DeviceDmaError::UnsupportedPlatform);
            }
            if hdr & PCI_HEADER_MULTIFUNCTION != 0 {
                return Err(DeviceDmaError::UnsupportedPlatform);
            }
            let port = RootPort {
                location: bridge,
                secondary,
                subordinate,
            };
            if candidate.replace(port).is_some() {
                return Err(DeviceDmaError::InvalidRequester);
            }
        }
    }
    let root = candidate.ok_or(DeviceDmaError::UnsupportedPlatform)?;
    if root.secondary != endpoint.bus || root.subordinate != endpoint.bus {
        return Err(DeviceDmaError::UnsupportedPlatform);
    }
    // Best-effort stability check: firmware/driver configuration must remain
    // unchanged while the domain is created; this is not hotplug protection.
    let buses = read_u32(root.location, 0x18)?;
    if buses as u8 != 0 || (buses >> 8) as u8 != endpoint.bus || (buses >> 16) as u8 != endpoint.bus
    {
        return Err(DeviceDmaError::InvalidRequester);
    }
    validate_acs(root.location)?;
    if read_u16(endpoint, 0)? == 0xffff
        || read_u16(endpoint, 4)? & crate::cfg_space::Command::BUS_MASTER.bits() != 0
    {
        return Err(DeviceDmaError::InvalidContext);
    }
    // No unowned downstream slots or bridge aliases. Probe all functions even
    // when function zero is absent or omits its multifunction header bit.
    for device in 0..=31 {
        for function in 0..=7 {
            let loc = PciDeviceLocation {
                bus: endpoint.bus,
                device,
                function,
            };
            if read_u16(loc, 0)? == 0xffff {
                continue;
            }
            if device != 0 || read_u8(loc, 0x0b)? == PCI_CLASS_BRIDGE {
                return Err(DeviceDmaError::UnsupportedPlatform);
            }
        }
    }
    Ok(Some([
        (root.location.device, root.location.function),
        (endpoint.device, endpoint.function),
    ]))
}

fn validate_acs(root_port: PciDeviceLocation) -> Result<(), DeviceDmaError> {
    let (offset, cap, ctl) = validate_extended_capabilities(root_port, true)?
        .ok_or(DeviceDmaError::UnsupportedPlatform)?;
    let _ = offset;
    validate_acs_bits(cap as u16, ctl as u16)
}

fn validate_acs_bits(capability: u16, control: u16) -> Result<(), DeviceDmaError> {
    // Source Validation, P2P Request Redirect, P2P Completion Redirect and
    // Upstream Forwarding must both exist and be enabled. We intentionally do
    // not require ACS Translation Blocking: this isolation path rejects ATS,
    // PRI and PASID on the endpoint and VT-d TT=00 denies translated requests.
    const REQUIRED: u16 = (1 << 0) | (1 << 2) | (1 << 3) | (1 << 4);
    const DIRECT_TRANSLATED_P2P: u16 = 1 << 6;
    if capability & REQUIRED != REQUIRED
        || control & REQUIRED != REQUIRED
        || control & DIRECT_TRANSLATED_P2P != 0
    {
        return Err(DeviceDmaError::UnsupportedPlatform);
    }
    Ok(())
}

#[cfg(ktest)]
mod tests {
    use ostd::prelude::ktest;

    use super::*;

    #[ktest]
    fn acs_requires_each_supported_and_enabled_isolation_control() {
        assert_eq!(validate_acs_bits(0x1d, 0x1d), Ok(()));
        for bit in [0, 2, 3, 4] {
            assert_eq!(
                validate_acs_bits(0x1d & !(1 << bit), 0x1d),
                Err(DeviceDmaError::UnsupportedPlatform)
            );
            assert_eq!(
                validate_acs_bits(0x1d, 0x1d & !(1 << bit)),
                Err(DeviceDmaError::UnsupportedPlatform)
            );
        }
    }

    #[ktest]
    fn direct_translated_peer_path_is_never_accepted() {
        assert_eq!(
            validate_acs_bits(0x7f, 0x5d),
            Err(DeviceDmaError::UnsupportedPlatform)
        );
        assert_eq!(validate_acs_bits(0x7f, 0x1f), Ok(()));
    }
}
