// SPDX-License-Identifier: MPL-2.0

//! PCI device capabilities.

use alloc::vec::Vec;

use int_to_c_enum::TryFromInt;
use ostd::Result;

use self::{
    msix::{CapabilityMsixData, RawCapabilityMsix},
    vendor::{CapabilityVndrData, RawCapabilityVndr},
};
use crate::{
    PciDeviceLocation,
    cfg_space::{PciGeneralDeviceCfgOffset, Status},
    common_device::{BarManager, PciCommonDevice},
};

pub mod msix;
pub mod vendor;

/// Raw PCI Capabilities.
#[derive(Debug, Default)]
pub(super) struct RawCapabilities {
    msix: Option<RawCapabilityMsix>,
    vndr: Vec<RawCapabilityVndr>,
}

/// PCI capability types.
#[repr(u8)]
#[derive(Clone, Copy, Debug, TryFromInt)]
enum CapabilityType {
    /// Power Management
    Pm = 0x01,
    /// Accelerated Graphics Part
    Agp = 0x02,
    /// Vital Product Data
    Vpd = 0x03,
    /// Slot Identification
    SlotId = 0x04,
    /// Message Signalled Interrupts
    Msi = 0x05,
    /// CompactPCI HotSwap
    Chswp = 0x06,
    /// PCI-X
    PciX = 0x07,
    /// HyperTransport
    Hp = 0x08,
    /// Vendor-Specific
    Vndr = 0x09,
    /// Debug port
    Dbg = 0x0A,
    /// CompactPCI Central Resource Control
    Ccrc = 0x0B,
    /// PCI Standard Hot-Plug Controller
    Shpc = 0x0C,
    /// Bridge subsystem vendor/device ID
    Ssvid = 0x0D,
    /// AGP Target PCI-PCI bridge
    Agp3 = 0x0E,
    /// Secure Device
    Secdev = 0x0F,
    /// PCI Express
    Exp = 0x10,
    /// MSI-X
    Msix = 0x11,
    /// SATA Data/Index Conf
    Sata = 0x12,
    /// PCI Advanced Features
    Af = 0x13,
    /// Enhanced Allocation
    Ea = 0x14,
}

impl RawCapabilities {
    /// The top of the capability position.
    const CAPABILITY_TOP: u16 = 0x100;

    /// Parses the capabilities of the PCI device.
    pub(super) fn parse(dev: &PciCommonDevice) -> Self {
        if !dev.read_status().contains(Status::CAPABILITIES_LIST) {
            return Self::default();
        }

        // The offset of the first capability pointer is the same for PCI general devices and PCI
        // bridge devices.
        const CAP_OFFSET: u16 = PciGeneralDeviceCfgOffset::CapabilitiesPointer as u16;
        let Some(mut cap_ptr_vec) =
            capability_offsets(dev.location().read8(CAP_OFFSET), |offset| {
                dev.location().read8(offset)
            })
        else {
            ostd::warn!(
                "malformed PCI capability list at {:?}; ignoring all capabilities",
                dev.location()
            );
            return Self::default();
        };
        cap_ptr_vec.sort();

        // Push the top position so that we can calculate the length of the last capability.
        cap_ptr_vec.push(Self::CAPABILITY_TOP);

        let mut caps = Self::default();

        let length = cap_ptr_vec.len();
        for i in 0..length - 1 {
            let cap_ptr = cap_ptr_vec[i];
            let next_ptr = cap_ptr_vec[i + 1];
            let raw_cap_type = dev.location().read8(cap_ptr);

            let Ok(cap_type) = CapabilityType::try_from(raw_cap_type) else {
                continue;
            };
            match cap_type {
                CapabilityType::Msix => {
                    // The fixed MSI-X header is 12 bytes, wholly inside this capability.
                    if next_ptr - cap_ptr < 12 {
                        ostd::warn!("truncated MSI-X capability at {:?}", dev.location());
                        continue;
                    }
                    // "More than one MSI-X Capability structure per Function is prohibited."
                    if caps.msix.is_some() {
                        ostd::warn!(
                            "superfluous MSI-X Capability structures at {:?} are ignored",
                            dev.location()
                        );
                        continue;
                    }
                    caps.msix = Some(RawCapabilityMsix::parse(dev, cap_ptr));
                }
                CapabilityType::Vndr => {
                    caps.vndr
                        .push(RawCapabilityVndr::new(cap_ptr, next_ptr - cap_ptr));
                }
                _ => {}
            }
        }

        caps
    }

    /// Acquires a new [`CapabilityMsixData`] instance.
    pub(super) fn acquire_msix_data(
        &self,
        loc: &PciDeviceLocation,
        bar_manager: &mut BarManager,
    ) -> Result<Option<CapabilityMsixData>> {
        let Some(raw_msix) = self.msix.as_ref() else {
            return Ok(None);
        };

        Ok(Some(CapabilityMsixData::new(loc, bar_manager, raw_msix)?))
    }

    /// Iterates over [`CapabilityVndrData`] instances.
    pub(super) fn iter_vndr_data(
        &self,
        loc: &PciDeviceLocation,
    ) -> impl Iterator<Item = CapabilityVndrData> {
        self.vndr
            .iter()
            .map(|raw_vndr| CapabilityVndrData::new(loc, raw_vndr))
    }
}

fn capability_offsets(mut pointer: u8, read8: impl Fn(u16) -> u8) -> Option<Vec<u16>> {
    let mut result = Vec::new();
    let mut visited = 0u64;
    while pointer != 0 {
        if !(0x40..=0xfc).contains(&pointer) || pointer & 3 != 0 {
            return None;
        }
        let bit = 1u64 << (pointer / 4);
        if visited & bit != 0 {
            return None;
        }
        visited |= bit;
        result.push(u16::from(pointer));
        pointer = read8(u16::from(pointer) + 1);
    }
    Some(result)
}

#[cfg(ktest)]
mod tests {
    use ostd::prelude::ktest;

    use super::*;
    #[ktest]
    fn enumeration_capability_walk_is_bounded_before_device_construction() {
        let mut data = [0u8; 256];
        assert!(
            capability_offsets(0, |_| panic!("empty list must not read"))
                .unwrap()
                .is_empty()
        );
        for invalid in [1, 0x3c, 0x41, 0xff] {
            assert!(
                capability_offsets(invalid, |_| panic!("invalid pointer must not read")).is_none()
            );
        }
        data[0x41] = 0x40;
        assert!(capability_offsets(0x40, |o| data[o as usize]).is_none());
        for ptr in (0x40..0xfc).step_by(4) {
            data[ptr + 1] = (ptr + 4) as u8;
        }
        assert_eq!(
            capability_offsets(0x40, |o| data[o as usize])
                .unwrap()
                .len(),
            48
        );
        data[0xfd] = 0x40;
        assert!(capability_offsets(0x40, |o| data[o as usize]).is_none());
    }
}
