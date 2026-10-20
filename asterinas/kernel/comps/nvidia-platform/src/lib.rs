// SPDX-License-Identifier: MPL-2.0
//! Read-only NVIDIA PCI resource inspection built on Asterinas' real `aster-pci` framework.
//!
//! Default initialization takes read-only snapshots. The explicit `device` API additionally
//! provides reversible pre-RM register-aperture ownership, never working-driver probe success.
#![no_std]
#![deny(unsafe_code)]

extern crate alloc;
pub mod device;
pub mod worker;

#[cfg(feature = "nvport-runtime")]
mod runtime;

macro_rules! __log_prefix {
    () => {
        "nvidia-platform: "
    };
}

use alloc::vec::Vec;

use aster_pci::{
    PciDeviceLocation,
    cfg_space::{AddrLen, Bar},
    common_device::PciCommonDevice,
};
use component::{ComponentInitError, init_component};

/// Inspects unclaimed NVIDIA display functions using the initialized PCI bus.
///
/// This only copies enumerator metadata and does not claim or initialize GPUs.
pub fn resource_inventory() -> Vec<Result<ResourceSnapshot, SnapshotError>> {
    aster_pci::PCI_BUS
        .lock()
        .unclaimed_devices()
        .filter(|device| {
            let id = device.device_id();
            id.vendor_id == NVIDIA_VENDOR_ID && id.class == PCI_BASE_CLASS_DISPLAY
        })
        .map(ResourceSnapshot::from_device)
        .collect()
}

#[init_component]
fn init() -> Result<(), ComponentInitError> {
    let resources = resource_inventory();
    if resources.is_empty() {
        ostd::warn!(
            "NVIDIA PCI diagnostics: no unclaimed NVIDIA display controller; RM/GSP not initialized"
        );
    }
    for resource in resources {
        match resource {
            Ok(snapshot) => ostd::info!(
                "NVIDIA PCI resources (not claimed, RM/GSP not initialized): {snapshot:?}"
            ),
            Err(error) => ostd::error!("NVIDIA PCI resource validation failed: {error:?}"),
        }
    }
    // Successful diagnostic initialization is not a successful GPU probe: this
    // component deliberately does not implement or register PciDriver.
    Ok(())
}

#[cfg(feature = "nvport-runtime")]
#[init_component(kthread)]
fn initialize_runtime() -> Result<(), ComponentInitError> {
    runtime::install_runtime().map_err(|_| ComponentInitError::Unknown)?;
    let uptime = ostd_nvidia_nvport::runtime::upstream_uptime_ns()
        .map_err(|_| ComponentInitError::Unknown)?;
    let realtime = ostd_nvidia_nvport::runtime::upstream_realtime_us()
        .map_err(|_| ComponentInitError::Unknown)?;
    let resolution = ostd_nvidia_nvport::runtime::upstream_resolution_ns()
        .map_err(|_| ComponentInitError::Unknown)?;
    let task = ostd_nvidia_nvport::runtime::upstream_task_identity()
        .map_err(|_| ComponentInitError::Unknown)?;
    ostd_nvidia_nvport::runtime::upstream_yield().map_err(|_| ComponentInitError::Unknown)?;
    if ostd_nvidia_nvport::runtime::upstream_task_identity().ok() != Some(task) {
        return Err(ComponentInitError::Unknown);
    }
    ostd::info!(
        "NVIDIA NVport runtime: real clock/thread/yield round trip; uptime_ns={uptime} realtime_us={realtime} nominal_tick_ns={resolution}; RM/GSP not initialized"
    );
    Ok(())
}

/// NVIDIA's PCI vendor ID.
pub const NVIDIA_VENDOR_ID: u16 = 0x10de;
/// PCI base class for display controllers (including VGA-compatible and 3D controllers).
pub const PCI_BASE_CLASS_DISPLAY: u8 = 0x03;
/// Conservative limit for the conventional PCI configuration header, supported by legacy config
/// mechanisms as well as ECAM. Extended configuration space is deliberately not exposed here.
pub const CONFIG_SPACE_LIMIT: u16 = 0x0100;

/// Resource inventory errors. A snapshot is rejected rather than making assumptions about an
/// absent, zero-sized, or wrapping BAR.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    /// The PCI function is not manufactured by NVIDIA.
    NotNvidia { vendor_id: u16 },
    /// PCI class information is present but is not a display controller.
    NotDisplayController { base_class: u8 },
    /// BAR0 was not found in the already-parsed BAR manager.
    MissingBar0,
    /// NVIDIA's required BAR0 is present but is not a memory BAR.
    Bar0NotMemory,
    /// A BAR reports an unusable zero base or zero size.
    InvalidBar { index: u8 },
    /// The exclusive end of a BAR's address range is not representable in u64.
    AddressOverflow { index: u8 },
}

/// Kind of PCI BAR, preserving whether the range is MMIO or port I/O.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarKind {
    /// Memory-mapped I/O BAR.
    Memory,
    /// I/O-port BAR.
    Io,
}

/// A diagnostic snapshot of one BAR's already-decoded resource information.
///
/// `end_exclusive` is computed with checked arithmetic. These fields describe the PCI framework's
/// current BAR metadata; they do not indicate that this crate or a driver has claimed or mapped it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BarSnapshot {
    /// BAR slot number (0 through 5). A 64-bit BAR's upper slot is not a separate entry.
    pub index: u8,
    /// Resource address as provided by `aster-pci`.
    pub base: u64,
    /// Resource length as provided by `aster-pci`'s hardware BAR sizing path.
    pub size: u64,
    /// Exclusive end address (`base + size`).
    pub end_exclusive: u64,
    /// Whether this is a memory BAR or an I/O-port BAR.
    pub kind: BarKind,
    /// Memory BAR address width; `None` for I/O BARs.
    pub address_length: Option<AddrLen>,
    /// PCI prefetchable attribute; `None` for I/O BARs.
    pub prefetchable: Option<bool>,
}

/// A read-only diagnostic resource inventory for one NVIDIA display-class PCI function.
///
/// This is a **snapshot only**, not a PCI resource claim, initialized device, successful driver
/// probe, MMIO mapping, or indication that NVIDIA RM/GSP is operational.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceSnapshot {
    /// PCI bus/device/function location.
    pub location: PciDeviceLocation,
    /// Vendor ID (`0x10de` for accepted devices).
    pub vendor_id: u16,
    /// PCI device ID.
    pub device_id: u16,
    /// PCI base class.
    pub base_class: u8,
    /// PCI subclass.
    pub subclass: u8,
    /// Slots with a BAR parsed by the PCI framework. Empty slots and upper halves of 64-bit BARs
    /// are `None`.
    pub bars: [Option<BarSnapshot>; 6],
}

impl ResourceSnapshot {
    /// Validate and snapshot an enumerated device using its existing `BarManager` metadata.
    ///
    /// This method never calls `Bar::acquire`, resizes BARs, reads config-space BARs directly,
    /// maps MMIO, or changes PCI command state. The PCI framework itself previously sized the BARs
    /// while enumerating the device; this method only consumes the resulting `base`, `size`, and
    /// attributes.
    pub fn from_device(device: &PciCommonDevice) -> Result<Self, SnapshotError> {
        let id = device.device_id();
        validate_identity(id.vendor_id, id.class)?;

        let mut bars = [None; 6];
        for index in 0..6u8 {
            if let Some(bar) = device.bar_manager().bar(index) {
                let snapshot = snapshot_bar(index, bar)?;
                bars[index as usize] = Some(snapshot);
            }
        }

        match bars[0] {
            None => return Err(SnapshotError::MissingBar0),
            Some(bar) if bar.kind != BarKind::Memory => return Err(SnapshotError::Bar0NotMemory),
            Some(_) => {}
        }

        Ok(Self {
            location: *device.location(),
            vendor_id: id.vendor_id,
            device_id: id.device_id,
            base_class: id.class,
            subclass: id.subclass,
            bars,
        })
    }
}

fn validate_identity(vendor_id: u16, base_class: u8) -> Result<(), SnapshotError> {
    if vendor_id != NVIDIA_VENDOR_ID {
        return Err(SnapshotError::NotNvidia { vendor_id });
    }
    // The PCI framework always reads the base-class byte into PciDeviceId. Do not silently accept
    // unknown/other class values: this API intentionally inventories display-class GPUs only.
    if base_class != PCI_BASE_CLASS_DISPLAY {
        return Err(SnapshotError::NotDisplayController { base_class });
    }
    Ok(())
}

fn checked_range(index: u8, base: u64, size: u64) -> Result<u64, SnapshotError> {
    if base == 0 || size == 0 {
        return Err(SnapshotError::InvalidBar { index });
    }
    base.checked_add(size)
        .ok_or(SnapshotError::AddressOverflow { index })
}

fn snapshot_bar(index: u8, bar: &Bar) -> Result<BarSnapshot, SnapshotError> {
    let snapshot = match bar {
        Bar::Memory(memory) => BarSnapshot {
            index,
            base: memory.base(),
            size: memory.size(),
            end_exclusive: checked_range(index, memory.base(), memory.size())?,
            kind: BarKind::Memory,
            address_length: Some(memory.address_length()),
            prefetchable: Some(memory.prefetchable()),
        },
        Bar::Io(io) => BarSnapshot {
            index,
            base: io.base() as u64,
            size: io.size() as u64,
            end_exclusive: checked_range(index, io.base() as u64, io.size() as u64)?,
            kind: BarKind::Io,
            address_length: None,
            prefetchable: None,
        },
    };
    Ok(snapshot)
}

/// Bounds and validates reads through the public `PciDeviceLocation` config-space interface.
///
/// Reads are read-only and restricted to the first 256 bytes. Callers should use this only with a
/// device returned by the initialized PCI enumerator; the underlying architecture accessor can
/// report hardware/configuration faults by panicking in the existing PCI API, and cannot be made
/// fallible by this wrapper.
pub struct ConfigReader<'a> {
    location: &'a PciDeviceLocation,
}

impl<'a> ConfigReader<'a> {
    /// Create a bounded reader for this enumerated PCI function.
    pub fn new(device: &'a PciCommonDevice) -> Self {
        Self {
            location: device.location(),
        }
    }

    /// Read one byte at an offset in conventional PCI configuration space.
    pub fn read8(&self, offset: u16) -> Result<u8, ConfigReadError> {
        check_bounds(offset, 1)?;
        Ok(self.location.read8(offset))
    }

    /// Read a little-endian 16-bit value at an aligned offset.
    pub fn read16(&self, offset: u16) -> Result<u16, ConfigReadError> {
        check_alignment(offset, 2)?;
        check_bounds(offset, 2)?;
        Ok(self.location.read16(offset))
    }

    /// Read a little-endian 32-bit value at an aligned offset.
    pub fn read32(&self, offset: u16) -> Result<u32, ConfigReadError> {
        check_alignment(offset, 4)?;
        check_bounds(offset, 4)?;
        Ok(self.location.read32(offset))
    }
}

/// Rejection from the safe bounds/alignment checks of [`ConfigReader`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigReadError {
    /// Offset/width extends beyond conventional 256-byte config space.
    OutOfBounds { offset: u16, width: u8 },
    /// 16/32-bit reads must be naturally aligned.
    Misaligned { offset: u16, width: u8 },
}

fn check_bounds(offset: u16, width: u8) -> Result<(), ConfigReadError> {
    let end = offset.checked_add(width as u16);
    if offset >= CONFIG_SPACE_LIMIT || !matches!(end, Some(end) if end <= CONFIG_SPACE_LIMIT) {
        return Err(ConfigReadError::OutOfBounds { offset, width });
    }
    Ok(())
}

fn check_alignment(offset: u16, width: u8) -> Result<(), ConfigReadError> {
    if offset % width as u16 != 0 {
        return Err(ConfigReadError::Misaligned { offset, width });
    }
    Ok(())
}

#[cfg(any(test, ktest))]
mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn identity_requires_nvidia_display_class() {
        assert_eq!(validate_identity(0x10de, 0x03), Ok(()));
        assert_eq!(
            validate_identity(0x1234, 0x03),
            Err(SnapshotError::NotNvidia { vendor_id: 0x1234 })
        );
        assert_eq!(
            validate_identity(0x10de, 0x02),
            Err(SnapshotError::NotDisplayController { base_class: 0x02 })
        );
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn range_is_checked_and_rejects_empty_or_zero_base() {
        assert_eq!(checked_range(0, 0x1000, 0x2000), Ok(0x3000));
        assert_eq!(
            checked_range(0, 0, 0x1000),
            Err(SnapshotError::InvalidBar { index: 0 })
        );
        assert_eq!(
            checked_range(2, 0x1000, 0),
            Err(SnapshotError::InvalidBar { index: 2 })
        );
        assert_eq!(
            checked_range(4, u64::MAX - 7, 8),
            Err(SnapshotError::AddressOverflow { index: 4 })
        );
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn conventional_config_reads_are_bounded_and_aligned() {
        assert_eq!(check_bounds(0xfc, 4), Ok(()));
        assert_eq!(
            check_bounds(0xfd, 4),
            Err(ConfigReadError::OutOfBounds {
                offset: 0xfd,
                width: 4
            })
        );
        assert_eq!(
            check_bounds(0x100, 1),
            Err(ConfigReadError::OutOfBounds {
                offset: 0x100,
                width: 1
            })
        );
        assert_eq!(check_alignment(0x10, 4), Ok(()));
        assert_eq!(
            check_alignment(0x11, 4),
            Err(ConfigReadError::Misaligned {
                offset: 0x11,
                width: 4
            })
        );
    }
}
