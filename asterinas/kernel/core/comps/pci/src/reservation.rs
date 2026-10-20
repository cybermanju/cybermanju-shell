// SPDX-License-Identifier: MPL-2.0
//! Exclusive, reversible ownership of an enumerated, unbound PCI function.
//!
//! A reservation is not a successfully probed driver. The bus remains borrowed until the
//! reservation is dropped, so neither driver registration nor another reservation can take it.
use core::ops::Deref;

use crate::{bus::PciBus, common_device::PciCommonDevice};

/// An unbound PCI function temporarily removed from the bus probe queue.
///
/// Drop returns the same function to the queue without probing it or advertising readiness.
/// This is a non-hotplug interface: callers must not use it after physical device removal.
/// Hardware services must be stopped and their resource guards dropped before this guard.
pub struct PciReservation<'a> {
    bus: &'a mut PciBus,
    device: Option<PciCommonDevice>,
}

impl<'a> PciReservation<'a> {
    pub(crate) fn new(bus: &'a mut PciBus, device: PciCommonDevice) -> Self {
        Self {
            bus,
            device: Some(device),
        }
    }

    /// Borrows a resource guard that cannot outlive this reservation.
    /// No mutable PciCommonDevice is exposed, preventing independent MSI-X ownership.
    pub fn prepare_mmio(
        &mut self,
        index: u8,
    ) -> Result<crate::mmio::PciMmioGuard<'_>, crate::mmio::MmioPrepareError> {
        crate::mmio::PciMmioGuard::prepare(self.device.as_mut().unwrap(), index)
    }
}

impl Deref for PciReservation<'_> {
    type Target = PciCommonDevice;
    fn deref(&self) -> &Self::Target {
        self.device.as_ref().unwrap()
    }
}

impl Drop for PciReservation<'_> {
    fn drop(&mut self) {
        self.bus.return_reserved(self.device.take().unwrap());
    }
}

/// Exclusive ownership of every present function in one conventional PCI slot.
/// No mutable device escapes this guard. Hotplug/configuration changes are not supported.
/// Callers must stop every controller before reservation; BME-off alone does not
/// establish device-specific quiescence or drain already issued transactions.
pub struct PciGroupReservation<'a> {
    bus: &'a mut PciBus,
    devices: alloc::vec::Vec<PciCommonDevice>,
}

impl<'a> PciGroupReservation<'a> {
    pub(crate) fn new(bus: &'a mut PciBus, devices: alloc::vec::Vec<PciCommonDevice>) -> Self {
        Self { bus, devices }
    }

    /// All owned functions, in increasing function-number order, including zero.
    pub fn locations(&self) -> impl Iterator<Item = crate::PciDeviceLocation> + '_ {
        self.devices.iter().map(|device| *device.location())
    }

    /// Maps one BAR per function transactionally. Any failure drops all earlier
    /// mappings and restores their command words; the reservation remains owned.
    /// Functions without a usable selected MMIO BAR are conservatively rejected.
    pub fn prepare_mmio(
        &mut self,
        bar_indices: &[u8],
    ) -> Result<crate::mmio::PciGroupMmioGuard<'_>, crate::mmio::MmioPrepareError> {
        use crate::mmio::{MmioPrepareError, PciGroupMmioGuard, PciMmioGuard};
        if bar_indices.len() != self.devices.len() {
            return Err(MmioPrepareError::Mapping(ostd::Error::InvalidArgs));
        }
        let mut guards = alloc::vec::Vec::with_capacity(self.devices.len());
        for (device, index) in self.devices.iter_mut().zip(bar_indices) {
            guards.push(PciMmioGuard::prepare(device, *index)?);
        }
        Ok(PciGroupMmioGuard::new(guards))
    }
}

impl Drop for PciGroupReservation<'_> {
    fn drop(&mut self) {
        if self.devices.iter().any(PciCommonDevice::irqs_quarantined) {
            for device in &self.devices {
                device.quarantine_irqs();
            }
        }
        for device in self.devices.drain(..) {
            self.bus.return_reserved(device);
        }
    }
}
