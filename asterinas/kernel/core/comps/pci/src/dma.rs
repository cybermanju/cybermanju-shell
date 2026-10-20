// SPDX-License-Identifier: MPL-2.0
//! Device-bound direct and isolated DMA contexts.
use core::marker::PhantomData;

use ostd::mm::dma::{DeviceDmaError, DirectDmaBuffer, DirectDmaContext, DmaMask, PciDmaRequester};
#[cfg(target_arch = "x86_64")]
use ostd::mm::dma::{IsolatedDmaBuffer, IsolatedDmaContext};

use crate::mmio::PciMmioGuard;

/// Direct DMA context borrowed from an exclusively owned, prepared PCI function.
///
/// Buffers borrow this context; the context borrows the MMIO guard, which in turn
/// borrows the function/reservation. None can outlive its owner. No bus mastering
/// is enabled here. Requester identity is taken from the real enumerator, not a
/// caller-provided BDF. Direct DMA has no hardware isolation against other devices.
/// Driver-specific DMA completion/quiescence is still required before freeing buffers.
/// There is no hot-unplug support, low-memory bounce allocator or streaming mapping.
pub struct PciDmaContext<'a> {
    inner: DirectDmaContext,
    _owner: PhantomData<&'a ()>,
}

impl<'a> PciDmaContext<'a> {
    pub(crate) fn new(owner: &'a PciMmioGuard<'_>, mask: DmaMask) -> Result<Self, DeviceDmaError> {
        let loc = owner.location();
        // The current PCI enumerator is segment zero only. Do not invent segment routing.
        let requester = PciDmaRequester::new(0, loc.bus, loc.device, loc.function)?;
        Ok(Self {
            inner: DirectDmaContext::new(requester, mask)?,
            _owner: PhantomData,
        })
    }

    /// Actual enumerated requester. This metadata does not establish IOMMU isolation.
    pub fn requester(&self) -> PciDmaRequester {
        *self.inner.requester()
    }

    /// Inclusive DMA address mask checked on every allocation, including the last byte.
    pub fn mask(&self) -> DmaMask {
        *self.inner.mask()
    }

    /// Allocate zeroed, contiguous, coherent memory in this explicit direct-DMA mode.
    /// Allocation outside the mask fails and frees the memory; addresses are never truncated.
    pub fn alloc_coherent(&self, nframes: usize) -> Result<DirectDmaBuffer<'_>, DeviceDmaError> {
        self.inner.alloc_coherent(nframes)
    }
}

/// An isolated DMA context tied to an exclusively reserved PCI function.
///
/// The context borrows the MMIO guard, which borrows the PCI reservation, so a
/// requester cannot be returned to enumeration while the DMA domain or any of
/// its buffers remain live. It retains the conventional bus-zero type-0 path
/// and admits only PCIe type-0 endpoints behind one validated, ACS-protected
/// root port. Switches, nested bridges, aliases, and ATS/PRI/PASID are rejected.
#[cfg(target_arch = "x86_64")]
pub struct PciIsolatedDmaContext<'a> {
    inner: IsolatedDmaContext,
    _owner: PhantomData<&'a ()>,
}

#[cfg(target_arch = "x86_64")]
impl<'a> PciIsolatedDmaContext<'a> {
    /// Builds an isolated domain for the real requester owned by `owner`.
    ///
    /// Device lifetime and completion
    /// The caller must exclusively own this requester, keep bus mastering
    /// disabled until all buffers are ready, and ensure the device is quiescent.
    /// PCI topology is read-only validated before constructing the domain. The
    /// caller must keep the route/configuration stable (there is no hotplug or
    /// bridge-reconfiguration protection) and ensure device-specific quiescence
    /// and completion; topology checks cannot prove driver-level ownership.
    pub(crate) fn new(owner: &'a PciMmioGuard<'_>, mask: DmaMask) -> Result<Self, DeviceDmaError> {
        let loc = owner.location();
        let route = crate::topology::admit(loc)?;
        let requester = PciDmaRequester::new(0, loc.bus, loc.device, loc.function)?;
        let inner = match route {
            Some(path) => IsolatedDmaContext::new_with_path(requester, mask, &path)?,
            None => IsolatedDmaContext::new(requester, mask)?,
        };
        Ok(Self {
            inner,
            _owner: PhantomData,
        })
    }

    /// Returns the actual requester identity tied to this domain.
    pub fn requester(&self) -> &PciDmaRequester {
        self.inner.requester()
    }

    /// Returns the inclusive address mask enforced for allocations.
    pub fn mask(&self) -> &DmaMask {
        self.inner.mask()
    }

    /// Allocates zeroed, coherent memory mapped only in this requester's domain.
    pub fn alloc_coherent(&self, nframes: usize) -> Result<IsolatedDmaBuffer<'_>, DeviceDmaError> {
        self.inner.alloc_coherent(nframes)
    }

    /// Returns the hardware isolation-domain identifier.
    pub fn domain_id(&self) -> u16 {
        self.inner.domain_id()
    }
}

/// A single isolated domain shared by all exclusively owned functions of a slot.
/// No sibling may be independently probed while this context or a buffer lives.
/// This does not establish isolation *between* functions in the group.
#[cfg(target_arch = "x86_64")]
pub struct PciIsolatedDmaGroupContext<'a> {
    inner: IsolatedDmaContext,
    requesters: alloc::vec::Vec<PciDmaRequester>,
    _owner: PhantomData<&'a ()>,
}

#[cfg(target_arch = "x86_64")]
impl<'a> PciIsolatedDmaGroupContext<'a> {
    pub(crate) fn new(
        owner: &'a crate::mmio::PciGroupMmioGuard<'_>,
        mask: DmaMask,
    ) -> Result<Self, DeviceDmaError> {
        let locations = owner.locations();
        let routes = crate::topology::admit_group(&locations)?;
        let mut members = alloc::vec::Vec::with_capacity(locations.len());
        let mut requesters = alloc::vec::Vec::with_capacity(locations.len());
        for (loc, path) in locations.iter().zip(routes) {
            let requester = PciDmaRequester::new(0, loc.bus, loc.device, loc.function)?;
            members.push((requester, path));
            requesters.push(requester);
        }
        let inner = IsolatedDmaContext::new_group_with_paths(&members, mask)?;
        // A config change during publication invalidates admission. Drop detaches
        // every member transactionally; no function was enabled in this path.
        crate::topology::admit_group(&locations)?;
        Ok(Self {
            inner,
            requesters,
            _owner: PhantomData,
        })
    }

    /// All real requester identities attached to this one hardware domain.
    pub fn requesters(&self) -> &[PciDmaRequester] {
        &self.requesters
    }
    /// Shared hardware domain identifier (not one domain per function).
    pub fn domain_id(&self) -> u16 {
        self.inner.domain_id()
    }
    /// Allocate coherent owned memory visible to every function of this group.
    pub fn alloc_coherent(&self, nframes: usize) -> Result<IsolatedDmaBuffer<'_>, DeviceDmaError> {
        self.inner.alloc_coherent(nframes)
    }
}
