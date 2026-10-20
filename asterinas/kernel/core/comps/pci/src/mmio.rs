// SPDX-License-Identifier: MPL-2.0
//! Pre-driver MMIO resource lifetime, with bus mastering and interrupts kept disabled.
use ostd::{Error, io::IoMem, mm::VmIoOnce};

use crate::{
    cfg_space::{Bar, Command, PciCommonCfgOffset, Status},
    common_device::PciCommonDevice,
};

/// A rejected pre-driver resource acquisition. No error means RM or a GPU is initialized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MmioPrepareError {
    /// Mapping cannot be performed with IRQs disabled or from interrupt context.
    InvalidContext,
    /// An absent, already mapped, invalid, or unallocatable memory BAR.
    Mapping(Error),
    /// Device configuration no longer responds.
    DeviceGone,
    /// Invalid/cyclic conventional capability chain.
    MalformedCapabilities,
    /// Existing MSI/MSI-X interrupts require a real driver stop/reset first.
    ActiveMessageInterrupts,
    /// Existing DMA needs driver-specific shutdown/reset; clearing a bit is not quiescence.
    BusMasterActive,
    /// The function is not in D0. This guard does not implement power transitions.
    NotD0,
    /// PCI command bits did not latch as requested.
    CommandRejected,
}

/// A temporary mapping of one memory BAR of an exclusively owned function.
///
/// This is not a DMA domain, IRQ registration, RM instance, hardware reset or working driver.
/// The mutable device borrow prevents the reservation from returning to the PCI bus early.
/// It rejects enabled MSI/MSI-X and non-D0 power states, disables bus mastering and INTx,
/// enables memory decoding, and restores the exact command word after dropping the mapping.
/// It must be acquired/dropped with IRQs enabled, as IoMem mapping uses cross-CPU TLB flushes.
/// No mapping reference/clone or MMIO write interface is exposed.
pub struct PciMmioGuard<'a> {
    device: &'a mut PciCommonDevice,
    mapping: Option<IoMem>,
    original_command: u16,
}

impl<'a> PciMmioGuard<'a> {
    /// Acquires one BAR for pre-driver CPU MMIO access. All ordinary error paths roll back.
    pub fn prepare(
        device: &'a mut PciCommonDevice,
        bar_index: u8,
    ) -> Result<Self, MmioPrepareError> {
        if !ostd::irq::is_local_enabled()
            || ostd::irq::InterruptLevel::current().is_interrupt_context()
        {
            return Err(MmioPrepareError::InvalidContext);
        }
        if bar_index >= 6 {
            return Err(MmioPrepareError::Mapping(Error::InvalidArgs));
        }
        let loc = device.location();
        if loc.read16(0) == 0xffff {
            return Err(MmioPrepareError::DeviceGone);
        }
        if device.read_status().contains(Status::CAPABILITIES_LIST) {
            validate_capabilities(
                loc.read8(0x34),
                |offset| loc.read8(offset),
                |offset| loc.read16(offset),
            )?;
        }
        let original_command = loc.read16(PciCommonCfgOffset::Command as u16);
        if original_command & Command::BUS_MASTER.bits() != 0 {
            return Err(MmioPrepareError::BusMasterActive);
        }
        let prepared = prepared_command(original_command);
        device.write_command_raw(prepared);
        // Config read flushes the write before mapping or issuing CPU MMIO reads.
        if device.location().read16(PciCommonCfgOffset::Command as u16) != prepared {
            device.write_command_raw(original_command);
            return Err(MmioPrepareError::CommandRejected);
        }
        let mut guard = Self {
            device,
            mapping: None,
            original_command,
        };
        let bar = guard
            .device
            .bar_manager_mut()
            .bar_mut(bar_index)
            .ok_or(MmioPrepareError::Mapping(Error::InvalidArgs))?;
        let Bar::Memory(memory) = bar else {
            return Err(MmioPrepareError::Mapping(Error::InvalidArgs));
        };
        guard.mapping = Some(memory.acquire_owned().map_err(MmioPrepareError::Mapping)?);
        Ok(guard)
    }

    /// Read a naturally aligned 32-bit register inside the mapped BAR.
    ///
    /// Register-specific read side effects remain the caller's responsibility. This does not
    /// infer any GPU identity, readiness or successful hardware initialization from its value.
    pub fn read32(&self, offset: usize) -> Result<u32, Error> {
        if !offset.is_multiple_of(4) {
            return Err(Error::InvalidArgs);
        }
        self.mapping.as_ref().unwrap().read_once(offset)
    }

    /// Prepare device-bound direct DMA without enabling bus mastering or interrupts.
    /// Rejects IOMMU/CVM configurations that cannot provide this direct-address contract.
    pub fn dma_context(
        &self,
        mask: ostd::mm::dma::DmaMask,
    ) -> Result<crate::dma::PciDmaContext<'_>, ostd::mm::dma::DeviceDmaError> {
        crate::dma::PciDmaContext::new(self, mask)
    }

    /// Creates an isolated IOMMU context for this exclusively prepared function.
    /// Admits conventional bus-zero type-0 endpoints or PCIe type-0 endpoints
    /// behind one validated PCIe root port with required ACS redirects enabled.
    /// Other bridge topologies, aliases, ATS/PRI/PASID and non-ECAM PCIe access
    /// are rejected; no config-space registers are programmed by this method.
    ///
    /// Device completion
    /// The caller must ensure exclusive requester ownership, a stable bridge route,
    /// and device quiescence with bus mastering disabled. This does not protect
    /// against hotplug or bridge reconfiguration and cannot stop device activity.
    #[cfg(target_arch = "x86_64")]
    pub fn isolated_dma_context(
        &self,
        mask: ostd::mm::dma::DmaMask,
    ) -> Result<crate::dma::PciIsolatedDmaContext<'_>, ostd::mm::dma::DeviceDmaError> {
        // SAFETY: the caller of this method is responsible for upholding the
        // isolated-context constructor's documented device-quiescence contract.
        crate::dma::PciIsolatedDmaContext::new(self, mask)
    }

    pub(crate) fn location(&self) -> crate::PciDeviceLocation {
        *self.device.location()
    }

    // Only the QEMU EDU lifecycle test can issue device commands through this guard.
    // No public NVIDIA register-write or bus-master enable interface is added.
    #[cfg(ktest)]
    pub(crate) fn test_write32(&self, offset: usize, value: u32) -> Result<(), Error> {
        if !offset.is_multiple_of(4) {
            return Err(Error::InvalidArgs);
        }
        self.mapping.as_ref().unwrap().write_once(offset, &value)
    }

    #[cfg(ktest)]
    pub(crate) fn test_write64(&self, offset: usize, value: u64) -> Result<(), Error> {
        if !offset.is_multiple_of(8) {
            return Err(Error::InvalidArgs);
        }
        self.mapping.as_ref().unwrap().write_once(offset, &value)
    }
}

impl Drop for PciMmioGuard<'_> {
    fn drop(&mut self) {
        // No DMA buffers, ISR callbacks, task queues or GPU commands are ever created here.
        // Drop/unmap/recycle CPU mapping before restoring the prior function command state.
        drop(self.mapping.take());
        self.device.write_command_raw(self.original_command);
        let _ = self
            .device
            .location()
            .read16(PciCommonCfgOffset::Command as u16);
    }
}

fn prepared_command(original: u16) -> u16 {
    (original | Command::MEMORY_SPACE.bits() | Command::INTERRUPT_DISABLE.bits())
        & !Command::BUS_MASTER.bits()
}

fn validate_capabilities(
    mut pointer: u8,
    read8: impl Fn(u16) -> u8,
    read16: impl Fn(u16) -> u16,
) -> Result<(), MmioPrepareError> {
    let mut visited = 0u64;
    while pointer != 0 {
        if pointer < 0x40 || pointer > 0xfc || pointer & 3 != 0 {
            return Err(MmioPrepareError::MalformedCapabilities);
        }
        let bit = 1u64 << (pointer / 4);
        if visited & bit != 0 {
            return Err(MmioPrepareError::MalformedCapabilities);
        }
        visited |= bit;
        let offset = u16::from(pointer);
        let control = read16(offset + 2);
        match read8(offset) {
            0x05 if control & 1 != 0 => return Err(MmioPrepareError::ActiveMessageInterrupts),
            0x11 if control & 0x8000 != 0 => return Err(MmioPrepareError::ActiveMessageInterrupts),
            0x01 => {
                if offset > 0xf8 {
                    return Err(MmioPrepareError::MalformedCapabilities);
                }
                if read16(offset + 4) & 3 != 0 {
                    return Err(MmioPrepareError::NotD0);
                }
            }
            _ => {}
        }
        pointer = read8(offset + 1);
    }
    Ok(())
}

#[cfg(ktest)]
mod tests {
    use ostd::prelude::ktest;

    use super::*;
    #[ktest]
    fn preparation_disables_dma_preserves_other_command_bits() {
        for original in [0, 0xffff, 0xa5a5, 0x5a5a, Command::BUS_MASTER.bits()] {
            let new = prepared_command(original);
            assert_eq!(new & Command::BUS_MASTER.bits(), 0);
            assert_ne!(new & Command::MEMORY_SPACE.bits(), 0);
            assert_ne!(new & Command::INTERRUPT_DISABLE.bits(), 0);
            let touched =
                (Command::BUS_MASTER | Command::MEMORY_SPACE | Command::INTERRUPT_DISABLE).bits();
            assert_eq!(new & !touched, original & !touched);
        }
    }
    fn capabilities(data: &[u8; 256], pointer: u8) -> Result<(), MmioPrepareError> {
        validate_capabilities(
            pointer,
            |o| data[o as usize],
            |o| u16::from_le_bytes([data[o as usize], data[o as usize + 1]]),
        )
    }
    #[ktest]
    fn capabilities_reject_cycles_alignment_and_header_pointers() {
        let mut data = [0; 256];
        assert_eq!(capabilities(&data, 0), Ok(()));
        for p in [1, 0x3c, 0x41, 0xff] {
            assert_eq!(
                capabilities(&data, p),
                Err(MmioPrepareError::MalformedCapabilities)
            );
        }
        data[0x41] = 0x40;
        assert_eq!(
            capabilities(&data, 0x40),
            Err(MmioPrepareError::MalformedCapabilities)
        );
        data[0x41] = 0x48;
        data[0x49] = 0x40;
        assert_eq!(
            capabilities(&data, 0x40),
            Err(MmioPrepareError::MalformedCapabilities)
        );
    }
    #[ktest]
    fn active_msi_msix_and_low_power_are_rejected_without_writes() {
        let mut data = [0; 256];
        data[0x40] = 5;
        data[0x42] = 1;
        assert_eq!(
            capabilities(&data, 0x40),
            Err(MmioPrepareError::ActiveMessageInterrupts)
        );
        data[0x42] = 0;
        assert_eq!(capabilities(&data, 0x40), Ok(()));
        data[0x40] = 0x11;
        data[0x43] = 0x80;
        assert_eq!(
            capabilities(&data, 0x40),
            Err(MmioPrepareError::ActiveMessageInterrupts)
        );
        data[0x40] = 1;
        data[0x43] = 0;
        data[0x44] = 3;
        assert_eq!(capabilities(&data, 0x40), Err(MmioPrepareError::NotD0));
        data[0x44] = 0;
        assert_eq!(capabilities(&data, 0x40), Ok(()));
        data[0xfc] = 1;
        assert_eq!(
            capabilities(&data, 0xfc),
            Err(MmioPrepareError::MalformedCapabilities)
        );
    }
}

/// All prepared MMIO functions borrowed from an exclusive slot reservation.
/// This does not enable interrupts/BME or imply that any controller is initialized.
pub struct PciGroupMmioGuard<'a> {
    functions: alloc::vec::Vec<PciMmioGuard<'a>>,
}

impl<'a> PciGroupMmioGuard<'a> {
    pub(crate) fn new(functions: alloc::vec::Vec<PciMmioGuard<'a>>) -> Self {
        Self { functions }
    }

    /// The prepared function at its index in the reservation's ordered list.
    pub fn function(&self, index: usize) -> Option<&PciMmioGuard<'a>> {
        self.functions.get(index)
    }

    pub(crate) fn locations(&self) -> alloc::vec::Vec<crate::PciDeviceLocation> {
        self.functions.iter().map(PciMmioGuard::location).collect()
    }

    /// Binds all owned functions to a single shared isolated translation domain.
    /// Every function must be quiescent, BME-off and remain so until mappings
    /// are ready. Internal function-to-function routing is inside this ownership
    /// boundary, not an isolation boundary. Route changes/hotplug are unsupported.
    #[cfg(target_arch = "x86_64")]
    pub fn isolated_dma_context(
        &self,
        mask: ostd::mm::dma::DmaMask,
    ) -> Result<crate::dma::PciIsolatedDmaGroupContext<'_>, ostd::mm::dma::DeviceDmaError> {
        crate::dma::PciIsolatedDmaGroupContext::new(self, mask)
    }
}
