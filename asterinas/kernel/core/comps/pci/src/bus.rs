// SPDX-License-Identifier: MPL-2.0

//! PCI bus

use alloc::{collections::VecDeque, sync::Arc, vec::Vec};
use core::fmt::Debug;

use ostd::{bus::BusProbeError, debug, error};

use super::{PciCommonDevice, device_info::PciDeviceId};

/// A trait that represents PCI devices.
pub trait PciDevice: Sync + Send + Debug {
    /// Returns the device ID.
    fn device_id(&self) -> PciDeviceId;
}

/// A trait that represents PCI device drivers.
///
/// The PCI bus will pass the device through the `probe` function when a new device is registered.
pub trait PciDriver: Sync + Send + Debug {
    /// Probes an unclaimed PCI device.
    ///
    /// If the driver matches and succeeds in initializing the unclaimed device,
    /// then the driver will return an claimed instance of the device,
    /// signaling that the PCI device is now ready to work.
    ///
    /// Once a device is matched and claimed by a driver,
    /// it won't be fed to another driver for probing.
    #[expect(clippy::result_large_err)]
    fn probe(
        &self,
        device: PciCommonDevice,
    ) -> Result<Arc<dyn PciDevice>, (BusProbeError, PciCommonDevice)>;
}

/// The PCI bus used to register PCI devices.
///
/// If a component wishes to drive a PCI device, it needs to provide the following:
/// 1. The structure that implements the [`PciDevice`] trait.
/// 2. A [`PciDriver`] instance.
pub struct PciBus {
    common_devices: VecDeque<PciCommonDevice>,
    devices: Vec<Arc<dyn PciDevice>>,
    drivers: Vec<Arc<dyn PciDriver>>,
}

impl PciBus {
    /// Temporarily reserves an unclaimed function without reporting a successful probe.
    ///
    /// The mutable bus borrow prevents concurrent probing. The returned guard puts the
    /// function back on the unclaimed queue on drop. No driver remove/hotplug is implied.
    pub fn reserve_unclaimed(
        &mut self,
        location: super::PciDeviceLocation,
    ) -> Option<super::reservation::PciReservation<'_>> {
        let index = self
            .common_devices
            .iter()
            .position(|device| *device.location() == location)?;
        let device = self.common_devices.remove(index).unwrap();
        Some(super::reservation::PciReservation::new(self, device))
    }

    pub(crate) fn return_reserved(&mut self, device: PciCommonDevice) {
        self.common_devices.push_back(device);
    }

    /// Reserves every physically present function of one slot, or none.
    /// A missing/unowned enumerated sibling is a hard error. The bus remains
    /// mutably borrowed for the entire group lifetime; no probing can interleave.
    /// This read-only operation never stops hardware or clears bus mastering.
    pub fn reserve_unclaimed_group(
        &mut self,
        location: super::PciDeviceLocation,
    ) -> Result<super::reservation::PciGroupReservation<'_>, ostd::mm::dma::DeviceDmaError> {
        use ostd::mm::dma::DeviceDmaError;
        let locations = super::topology::present_slot_functions(location)?;
        for loc in &locations {
            if !self.common_devices.iter().any(|d| d.location() == loc) {
                return Err(DeviceDmaError::InvalidContext);
            }
        }
        let mut devices = Vec::with_capacity(locations.len());
        for loc in &locations {
            let index = self
                .common_devices
                .iter()
                .position(|d| d.location() == loc)
                .unwrap();
            devices.push(self.common_devices.remove(index).unwrap());
        }
        // Recheck presence and BME after taking ownership; rollback the whole
        // group if configuration changed. Hotplug is deliberately unsupported.
        if super::topology::present_slot_functions(location).as_ref() != Ok(&locations) {
            for device in devices {
                self.return_reserved(device);
            }
            return Err(DeviceDmaError::InvalidContext);
        }
        Ok(super::reservation::PciGroupReservation::new(self, devices))
    }

    /// Removes an unclaimed function for lifecycle tests that need simultaneous ownership.
    /// The test must return the same device with `return_reserved` after dropping its guards.
    #[cfg(ktest)]
    pub(crate) fn test_take_unclaimed(
        &mut self,
        location: super::PciDeviceLocation,
    ) -> Option<PciCommonDevice> {
        let index = self
            .common_devices
            .iter()
            .position(|device| *device.location() == location)?;
        self.common_devices.remove(index)
    }

    /// Borrows the enumerated functions that have not been claimed by a driver.
    ///
    /// The caller must retain the bus lock while using this iterator. Inspecting
    /// a function does not claim it or establish a working device driver.
    pub fn unclaimed_devices(&self) -> impl Iterator<Item = &PciCommonDevice> {
        self.common_devices.iter()
    }

    /// Registers a PCI driver to the PCI bus.
    pub fn register_driver(&mut self, driver: Arc<dyn PciDriver>) {
        debug!("Register PCI driver: {:#x?}", driver);
        let length = self.common_devices.len();
        for _ in (0..length).rev() {
            let common_device = self.common_devices.pop_front().unwrap();
            let device_id = *common_device.device_id();
            let device = match driver.probe(common_device) {
                Ok(device) => {
                    debug_assert!(device_id == device.device_id());
                    self.devices.push(device);
                    continue;
                }
                Err((err, common_device)) => {
                    if err != BusProbeError::DeviceNotMatch {
                        error!("device construction failed, reason: {:?}", err);
                    }
                    debug_assert!(device_id == *common_device.device_id());
                    common_device
                }
            };
            self.common_devices.push_back(device);
        }
        self.drivers.push(driver);
    }

    pub(super) fn register_common_device(&mut self, mut common_device: PciCommonDevice) {
        debug!("found common device: {:#x?}", common_device);
        let device_id = *common_device.device_id();
        for driver in self.drivers.iter() {
            common_device = match driver.probe(common_device) {
                Ok(device) => {
                    debug_assert!(device_id == device.device_id());
                    self.devices.push(device);
                    return;
                }
                Err((err, common_device)) => {
                    if err != BusProbeError::DeviceNotMatch {
                        error!("device construction failed, reason: {:?}", err);
                    }
                    debug_assert!(device_id == *common_device.device_id());
                    common_device
                }
            };
        }
        self.common_devices.push_back(common_device);
    }

    pub(super) const fn new() -> Self {
        Self {
            common_devices: VecDeque::new(),
            devices: Vec::new(),
            drivers: Vec::new(),
        }
    }
}
