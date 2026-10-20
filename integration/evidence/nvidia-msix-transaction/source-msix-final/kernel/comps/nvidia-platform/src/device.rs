// SPDX-License-Identifier: MPL-2.0
//! NVIDIA pre-RM resource ownership. This module deliberately does not implement PciDriver.
use aster_pci::{
    common_device::PciCommonDevice,
    mmio::{MmioPrepareError, PciMmioGuard},
};

use crate::{ResourceSnapshot, SnapshotError};

/// Failed validation or actual PCI/MMIO acquisition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrepareError {
    /// Enumerated identity/BAR metadata is invalid.
    Resources(SnapshotError),
    /// Real PCI/MMIO service rejected the request; no GPU success is inferred.
    Pci(MmioPrepareError),
    /// Prefetchable BAR0 cannot be used as the register aperture by this adapter.
    PrefetchableRegisters,
}

/// Exclusive register aperture for a validated NVIDIA display-class function.
///
/// This borrows an owned PciCommonDevice (for example, from PciReservation) so its lifetime
/// cannot exceed the reservation. NVIDIA's nv_pci_probe_body separately requires
/// rm_wait_for_bar_firewall and rm_is_supported_pci_device; this class does NOT replace
/// those checks, does not establish RM support and must not be returned as probe success.
/// It does not start GSP, enable DMA, wire interrupts, issue GPU commands or map BAR1.
/// Drop releases MMIO and restores the exact pre-acquisition command state.
pub struct NvidiaDeviceResources<'a> {
    snapshot: ResourceSnapshot,
    registers: PciMmioGuard<'a>,
}

impl<'a> NvidiaDeviceResources<'a> {
    /// Validates real enumerator metadata, then acquires the actual register BAR.
    ///
    /// Requires a non-hotremoved exclusively owned function, with IRQs enabled. GPUs
    /// requiring CXL subrange ownership, ReBAR reassignment, or power transitions are not
    /// supported by this pre-RM adapter. No firmware file is read or loaded by this path.
    pub fn prepare(device: &'a mut PciCommonDevice) -> Result<Self, PrepareError> {
        let snapshot = ResourceSnapshot::from_device(device).map_err(PrepareError::Resources)?;
        if snapshot.bars[0].unwrap().prefetchable == Some(true) {
            return Err(PrepareError::PrefetchableRegisters);
        }
        let registers = PciMmioGuard::prepare(device, 0).map_err(PrepareError::Pci)?;
        Ok(Self {
            snapshot,
            registers,
        })
    }

    /// Acquires through the bus reservation without exposing mutable device state.
    pub fn prepare_reserved(
        reservation: &'a mut aster_pci::reservation::PciReservation<'_>,
    ) -> Result<Self, PrepareError> {
        let snapshot =
            ResourceSnapshot::from_device(reservation).map_err(PrepareError::Resources)?;
        if snapshot.bars[0].unwrap().prefetchable == Some(true) {
            return Err(PrepareError::PrefetchableRegisters);
        }
        let registers = reservation.prepare_mmio(0).map_err(PrepareError::Pci)?;
        Ok(Self {
            snapshot,
            registers,
        })
    }

    /// Validated metadata, not a supported-chip or initialized-GPU certificate.
    pub fn snapshot(&self) -> &ResourceSnapshot {
        &self.snapshot
    }

    /// Prepare explicitly non-isolated direct DMA for this actual PCI requester.
    ///
    /// NVIDIA probe initially requires a 32-bit mask; callers must supply that mask
    /// until real RM capability negotiation changes it. This does not start RM/GSP,
    /// enable bus mastering, authenticate firmware or provide an IOMMU domain.
    /// IOMMU/CVM and unreachable physical allocations fail rather than falling back
    /// to OSTD's global shared DMA mappings. Buffers cannot outlive this resource owner.
    pub fn dma_context(
        &self,
        mask: ostd::mm::dma::DmaMask,
    ) -> Result<aster_pci::dma::PciDmaContext<'_>, ostd::mm::dma::DeviceDmaError> {
        self.registers.dma_context(mask)
    }

    /// Request bounded per-requester VT-d memory without enabling bus mastering.
    ///
    /// PCIe requires a single-function endpoint immediately below one segment-zero
    /// root port with verified ACS/routing and no ATS/PRI/PASID or aliases. Common
    /// multifunction NVIDIA adapters, switches and unsupported firmware fail closed.
    /// This does not establish supported-chip/BAR-firewall checks or RM/GSP startup,
    /// and it never falls back to the shared mapper or enables bus mastering.
    #[cfg(target_arch = "x86_64")]
    pub fn isolated_dma_context(
        &self,
        mask: ostd::mm::dma::DmaMask,
    ) -> Result<aster_pci::dma::PciIsolatedDmaContext<'_>, ostd::mm::dma::DeviceDmaError> {
        self.registers.isolated_dma_context(mask)
    }

    /// Reserves real MSI-X vectors masked, without enabling DMA or inventing RM callbacks.
    /// The caller must register real device handlers before activation, and implement NVIDIA
    /// hardware/worker quiescence before shutdown. This is not probe or RM/GSP success.
    pub fn prepare_msix(
        &self,
        count: u16,
    ) -> Result<aster_pci::msix_transaction::PciMsixGuard<'_>, ostd::Error> {
        self.registers.prepare_msix(count)
    }

    /// Read a register via the real bounded MMIO mapping.
    ///
    /// Do not call until NVIDIA's BAR firewall/power prerequisites have been satisfied.
    /// The adapter never interprets all-ones/zero/read success as successful GPU probe.
    pub fn read_register32(&self, offset: usize) -> Result<u32, ostd::Error> {
        self.registers.read32(offset)
    }
}
