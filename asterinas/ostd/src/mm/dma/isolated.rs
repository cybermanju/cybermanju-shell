// SPDX-License-Identifier: MPL-2.0
//! Owned coherent DMA buffers in a bounded Intel VT-d requester domain.
use alloc::vec::Vec;

use super::{DeviceDmaError, DmaMask, PciDmaRequester};
use crate::{
    irq,
    mm::{
        FrameAllocOptions, HasDaddr, HasPaddr, HasSize, Infallible, PAGE_SIZE, Segment, VmReader,
        VmWriter,
        io::util::{HasVmReaderWriter, VmReaderWriterIdentity},
    },
};

/// Private hardware translation tables for a requester. No bus mastering is enabled.
/// This low-level OSTD primitive does not authenticate BDF metadata or prove
/// exclusive PCI ownership. Use the PCI-bound wrapper for that binding. Every
/// mapped physical frame is owned untyped storage; no arbitrary PADDR is accepted.
/// Legacy TT=00 denies translated requests (device-TLB operation is not enabled).
///
/// Only explicit supported DMAR routes are accepted; unsupported firmware
/// topology, nonzero segments and CVMs remain rejected. Use [`Self::new_with_path`]
/// for a requester below a PCI bridge. This primitive does not discover or
/// authenticate topology: its PCI owner must resolve and validate the route.
#[cfg(target_arch = "x86_64")]
pub struct IsolatedDmaContext {
    handle: crate::arch::iommu::DomainHandle,
    requester: PciDmaRequester,
    requesters: Vec<PciDmaRequester>,
    mask: DmaMask,
}
#[cfg(target_arch = "x86_64")]
impl IsolatedDmaContext {
    /// Creates a private domain without enabling device DMA.
    ///
    /// Requester binding
    /// The caller must exclusively own the actual enumerated PCI function and
    /// keep it quiescent with BME disabled until this returns. There must be no
    /// requester aliases, and ATS, PRI and PASID must be absent or disabled
    /// throughout the domain lifetime. This compatibility constructor admits
    /// only an explicitly scoped endpoint directly on root bus zero. Its
    /// ownership must outlive this context. No device DMA may be pending when
    /// buffers/context are dropped.
    pub fn new(requester: PciDmaRequester, mask: DmaMask) -> Result<Self, DeviceDmaError> {
        check_context()?;
        if crate::arch::if_tdx_enabled!({ true } else { false }) {
            return Err(DeviceDmaError::UnsupportedPlatform);
        }
        let handle = crate::arch::iommu::DomainHandle::new(requester)?;
        Ok(Self {
            handle,
            requester,
            requesters: alloc::vec![requester],
            mask,
        })
    }

    /// Creates one private domain shared by a bounded multifunction requester
    /// group. Each tuple contains an endpoint and its two-hop root-port-to-
    /// endpoint route resolved by the PCI owner. All requesters must be distinct
    /// functions of the same slot, include function zero, and share the same
    /// route prefix. Every member is checked against DMAR policy before any
    /// context is changed. The PCI owner must exclusively own and quiesce every
    /// function, keep BME disabled until this returns, prevent aliases and
    /// topology changes, and keep ownership alive for this context's lifetime.
    /// OSTD does not enable or verify BME.
    pub fn new_group_with_paths(
        requesters: &[(PciDmaRequester, [(u8, u8); 2])],
        mask: DmaMask,
    ) -> Result<Self, DeviceDmaError> {
        check_context()?;
        if crate::arch::if_tdx_enabled!({ true } else { false }) {
            return Err(DeviceDmaError::UnsupportedPlatform);
        }
        if requesters.is_empty() || requesters.len() > 8 {
            return Err(DeviceDmaError::InvalidRequester);
        }
        let paths: Vec<_> = requesters
            .iter()
            .map(|(requester, path)| (*requester, path.as_slice()))
            .collect();
        let handle = crate::arch::iommu::DomainHandle::new_group_with_paths(&paths)?;
        let requester = requesters
            .first()
            .map(|(requester, _)| *requester)
            .ok_or(DeviceDmaError::InvalidRequester)?;
        let requesters = requesters.iter().map(|(requester, _)| *requester).collect();
        Ok(Self {
            handle,
            requester,
            requesters,
            mask,
        })
    }

    /// Creates a private domain for an explicitly scoped endpoint at the end
    /// of a route resolved by the PCI owner.
    ///
    /// `path` is the ordered `(device, function)` chain from root bus zero to
    /// `requester`, with at most eight elements. A direct root-bus endpoint has
    /// one element; a requester behind bridges has a multi-element chain. The
    /// route is authorization metadata, not hardware authentication: the PCI
    /// wrapper must resolve the actual topology and bind this path to the
    /// exclusively owned endpoint. The caller must retain the same quiescence,
    /// no-alias, ATS/PRI/PASID, and lifetime guarantees as for [`Self::new`].
    pub fn new_with_path(
        requester: PciDmaRequester,
        mask: DmaMask,
        path: &[(u8, u8)],
    ) -> Result<Self, DeviceDmaError> {
        check_context()?;
        if crate::arch::if_tdx_enabled!({ true } else { false }) {
            return Err(DeviceDmaError::UnsupportedPlatform);
        }
        let handle = crate::arch::iommu::DomainHandle::new_with_path(requester, path)?;
        Ok(Self {
            handle,
            requester,
            requesters: alloc::vec![requester],
            mask,
        })
    }
    /// Actual requester; for a group this is the first member supplied.
    pub fn requester(&self) -> &PciDmaRequester {
        &self.requester
    }
    /// Every requester bound to this shared private domain, in constructor order.
    pub fn requesters(&self) -> &[PciDmaRequester] {
        &self.requesters
    }
    /// Inclusive maximum IOVA address.
    pub fn mask(&self) -> &DmaMask {
        &self.mask
    }
    /// Distinct hardware Domain ID (zero is reserved for the legacy shared mapper).
    pub fn domain_id(&self) -> u16 {
        self.handle.id()
    }
    /// Allocates zeroed contiguous RAM and maps it at a mask-constrained IOVA.
    /// A buffer must not be dropped until the device has completed using it.
    pub fn alloc_coherent(&self, nframes: usize) -> Result<IsolatedDmaBuffer<'_>, DeviceDmaError> {
        check_context()?;
        let size = nframes
            .checked_mul(PAGE_SIZE)
            .filter(|size| *size != 0)
            .ok_or(DeviceDmaError::InvalidSize)?;
        let segment = FrameAllocOptions::new()
            .alloc_segment(nframes)
            .map_err(DeviceDmaError::Memory)?;
        let iova = match self.handle.map(segment.paddr(), size, self.mask) {
            Ok(iova) => iova,
            Err(error) => {
                if error == DeviceDmaError::InvalidationFailed {
                    // Hardware may retain any partially installed translation.
                    core::mem::forget(segment);
                }
                return Err(error);
            }
        };
        Ok(IsolatedDmaBuffer {
            segment: Some(segment),
            context: self,
            iova,
        })
    }
}
fn check_context() -> Result<(), DeviceDmaError> {
    if !irq::is_local_enabled() || irq::InterruptLevel::current().is_interrupt_context() {
        Err(DeviceDmaError::InvalidContext)
    } else {
        Ok(())
    }
}

/// Mapping and physical storage owned until completed IOTLB revocation.
/// Borrowing enforces that the context outlives every buffer. Hardware
/// quiescence remains the driver's obligation; IOTLB invalidation cannot stop
/// an endpoint that is still issuing new requests.
#[cfg(target_arch = "x86_64")]
pub struct IsolatedDmaBuffer<'a> {
    segment: Option<Segment<()>>,
    context: &'a IsolatedDmaContext,
    iova: usize,
}
#[cfg(target_arch = "x86_64")]
impl IsolatedDmaBuffer<'_> {
    /// Revokes this mapping, waits for context/IOTLB completion and draining,
    /// then returns the still-owned physical allocation for CPU-only use.
    /// The device must already have completed its legitimate use of the buffer.
    /// On failure storage is quarantined, never returned to the allocator.
    pub fn revoke(mut self) -> Result<Segment<()>, DeviceDmaError> {
        check_context()?;
        if let Err(error) = self.context.handle.unmap(self.iova, self.size()) {
            core::mem::forget(self.segment.take());
            return Err(error);
        }
        Ok(self.segment.take().unwrap())
    }
}
#[cfg(target_arch = "x86_64")]
impl HasDaddr for IsolatedDmaBuffer<'_> {
    fn daddr(&self) -> usize {
        self.iova
    }
}
#[cfg(target_arch = "x86_64")]
impl HasSize for IsolatedDmaBuffer<'_> {
    fn size(&self) -> usize {
        self.segment.as_ref().unwrap().size()
    }
}
#[cfg(target_arch = "x86_64")]
impl HasVmReaderWriter for IsolatedDmaBuffer<'_> {
    type Types = VmReaderWriterIdentity;
    fn reader(&self) -> VmReader<'_, Infallible> {
        <Segment<()> as HasVmReaderWriter>::reader(self.segment.as_ref().unwrap())
    }
    fn writer(&self) -> VmWriter<'_, Infallible> {
        <Segment<()> as HasVmReaderWriter>::writer(self.segment.as_ref().unwrap())
    }
}
#[cfg(target_arch = "x86_64")]
impl Drop for IsolatedDmaBuffer<'_> {
    fn drop(&mut self) {
        if self.segment.is_none() {
            return;
        }
        if self.context.handle.unmap(self.iova, self.size()).is_err() {
            // Fail closed: neither physical RAM nor table pages/DIDs can be reused.
            core::mem::forget(self.segment.take());
            crate::error!("VT-d invalidation failed: DMA allocation quarantined, domain disabled");
        }
    }
}
