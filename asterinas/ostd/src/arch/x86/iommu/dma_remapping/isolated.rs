// SPDX-License-Identifier: MPL-2.0
//! Bounded legacy VT-d translation domains, including shared multifunction groups.
//! The PCI owner authenticates segment-zero routes and exclusive slot ownership;
//! firmware policy authorizes every requester. No ATS/PASID, RMRR or hot unplug.
//! Table pages remain owned until context/IOTLB invalidation has completed.
use alloc::{collections::BTreeMap, vec::Vec};
use core::sync::atomic::{Ordering, fence};

use spin::Once;

use super::{PAGE_TABLE, PciDeviceLocation};
use crate::{
    arch::iommu::registers::IOMMU_REGS,
    mm::{
        Frame, FrameAllocOptions, HasPaddr, PAGE_SIZE, VmIo,
        dma::{DeviceDmaError as E, DmaMask, PciDmaRequester},
        paddr_to_vaddr,
    },
    sync::{LocalIrqDisabled, SpinLock},
};

// DIDs are capped to the architectural minimum (16), with zero reserved for
// the preexisting shared legacy mapper. IDs are reused only after revocation.
struct Domain {
    root: Frame<()>,
    tables: BTreeMap<(usize, usize), Frame<()>>,
    next_iova: usize,
}
struct Domains {
    groups: BTreeMap<PciDeviceLocation, Domain>,
    members: BTreeMap<PciDeviceLocation, PciDeviceLocation>,
    ids: u16,
    poisoned: bool,
}
static DOMAINS: Once<SpinLock<Domains, LocalIrqDisabled>> = Once::new();

pub(crate) struct DomainHandle {
    location: PciDeviceLocation,
    members: Vec<PciDeviceLocation>,
    did: u16,
}

// ECAP.C may be zero, including under QEMU. Explicitly write back every table
// cache line before publishing it. CPUID CLFSH is checked before any domain is
// created; these helpers run only on that path. An eight-byte stride does not
// assume a particular cache-line size. The operand always points into an owned
// frame in OSTD's permanent linear map.
pub(in crate::arch::iommu) fn flush(paddr: usize, size: usize) {
    for offset in (0..size).step_by(8) {
        let address = paddr_to_vaddr(paddr + offset);
        // SAFETY: caller passes an owned, mapped table/queue frame range.
        unsafe {
            core::arch::asm!("clflush [{}]", in(reg) address, options(nostack, preserves_flags));
        }
    }
    // SAFETY: MFENCE is available on x86-64 and orders the flushes above.
    unsafe {
        core::arch::asm!("mfence", options(nostack, preserves_flags));
    }
}
fn frame() -> Result<Frame<()>, E> {
    let frame = FrameAllocOptions::new().alloc_frame().map_err(E::Memory)?;
    flush(frame.paddr(), PAGE_SIZE);
    Ok(frame)
}
fn write(frame: &Frame<()>, index: usize, value: u64) {
    frame.write_val(index * 8, &value).unwrap();
    flush(frame.paddr() + index * 8, 8);
}
fn location(r: PciDmaRequester) -> Result<PciDeviceLocation, E> {
    if r.segment() != 0
        || r.device() > 31
        || r.function() > 7
        || (r.bus() == 0 && r.device() == 0 && r.function() == 0)
    {
        return Err(E::InvalidRequester);
    }
    Ok(PciDeviceLocation {
        bus: r.bus(),
        device: r.device(),
        function: r.function(),
    })
}
impl DomainHandle {
    /// Caller guarantees exclusive, quiescent endpoint ownership, no aliases,
    /// no ATS/PRI/PASID, and BME disabled until this operation completes.
    pub(crate) fn new(requester: PciDmaRequester) -> Result<Self, E> {
        if requester.bus() != 0 {
            return Err(E::InvalidRequester);
        }
        let path = [(requester.device(), requester.function())];
        Self::new_with_path(requester, &path)
    }

    /// Creates a private domain for a requester whose actual route was resolved
    /// by the PCI owner. `path` is root-bus-to-endpoint device/function metadata;
    /// it is not independently authenticated against PCI configuration space.
    /// The PCI caller must establish that this exact route belongs to the
    /// exclusively owned, quiescent endpoint before invoking this constructor.
    pub(crate) fn new_with_path(requester: PciDmaRequester, path: &[(u8, u8)]) -> Result<Self, E> {
        Self::new_group_with_paths(&[(requester, path)])
    }

    /// Creates one domain for a bounded set of multifunction requesters. The
    /// PCI owner must prove exclusive/quiescent ownership of every member and
    /// keep BME disabled until the complete group is installed.
    pub(crate) fn new_group_with_paths(
        requesters: &[(PciDmaRequester, &[(u8, u8)])],
    ) -> Result<Self, E> {
        let members = validate_group(requesters)?;
        let location = members[0];
        if PAGE_TABLE.get().is_none() {
            return Err(E::UnsupportedPlatform);
        }
        for (requester, path) in requesters {
            if !crate::arch::kernel::acpi::vtd_policy::requester_allowed_with_path(
                requester.segment(),
                requester.bus(),
                requester.device(),
                requester.function(),
                path,
            ) {
                return Err(E::UnsupportedPlatform);
            }
        }
        // SAFETY: CPUID is available on x86-64 and has no memory side effects.
        if core::arch::x86_64::__cpuid(1).edx & (1 << 19) == 0 {
            return Err(E::UnsupportedPlatform);
        }
        {
            let regs = IOMMU_REGS.get().ok_or(E::UnsupportedPlatform)?.lock();
            if !regs.isolated_domains_supported() {
                return Err(E::UnsupportedPlatform);
            }
        }
        let registry = DOMAINS.call_once(|| {
            SpinLock::new(Domains {
                groups: BTreeMap::new(),
                members: BTreeMap::new(),
                ids: 1,
                poisoned: false,
            })
        });
        let mut domains = registry.lock();
        if domains.poisoned {
            return Err(E::InvalidationFailed);
        }
        if members
            .iter()
            .any(|member| domains.members.contains_key(member))
        {
            return Err(E::DomainBusy);
        }
        let did = (1..16)
            .find(|id| domains.ids & (1 << id) == 0)
            .ok_or(E::DomainExhausted)?;
        let domain = Domain {
            root: frame()?,
            tables: BTreeMap::new(),
            next_iova: PAGE_SIZE,
        };
        let root = domain.root.paddr();
        // Reserve every BDF and the DID before changing a hardware context.
        // Holding the registry lock makes the reservation visible atomically.
        domains.ids |= 1 << did;
        domains.groups.insert(location, domain);
        for member in &members {
            domains.members.insert(*member, location);
        }
        // Revoke all former shared contexts as one group operation before
        // publishing any member's private context.
        let mut roots = PAGE_TABLE.get().unwrap().lock();
        for member in &members {
            roots.clear_device_context(*member);
        }
        if !IOMMU_REGS.get().unwrap().lock().invalidate_dma_sync() {
            domains.poisoned = true;
            return Err(E::InvalidationFailed); // reservations and tables quarantined
        }
        for member in &members {
            roots.install_isolated_context(*member, root, did);
        }
        if !IOMMU_REGS.get().unwrap().lock().invalidate_dma_sync() {
            domains.poisoned = true;
            return Err(E::InvalidationFailed);
        }
        Ok(Self {
            location,
            members,
            did,
        })
    }
    pub(crate) fn id(&self) -> u16 {
        self.did
    }
    pub(crate) fn map(&self, paddr: usize, size: usize, mask: DmaMask) -> Result<usize, E> {
        let mut domains = DOMAINS.get().unwrap().lock();
        if domains.poisoned {
            return Err(E::InvalidationFailed);
        }
        let domain = domains
            .groups
            .get_mut(&self.location)
            .ok_or(E::MappingFailed)?;
        let iova = domain.next_iova;
        let end = iova.checked_add(size).ok_or(E::AddressOutOfRange)?;
        // IommuPtConfig's low-canonical three-level window is conservatively 38 bits.
        if size == 0
            || !size.is_multiple_of(PAGE_SIZE)
            || end > (1usize << 38)
            || !mask.contains_range(iova as u64, size)
        {
            return Err(E::AddressOutOfRange);
        }
        let mut mapped = 0;
        let result = (|| {
            for offset in (0..size).step_by(PAGE_SIZE) {
                let address = iova + offset;
                let l3 = (address >> 30) & 511;
                let l2 = (address >> 21) & 511;
                let l1 = (address >> 12) & 511;
                if !domain.tables.contains_key(&(l3, 512)) {
                    let table = frame()?;
                    let address = table.paddr();
                    domain.tables.insert((l3, 512), table);
                    write(&domain.root, l3, address as u64 | 3);
                }
                if !domain.tables.contains_key(&(l3, l2)) {
                    let table = frame()?;
                    let address = table.paddr();
                    domain.tables.insert((l3, l2), table);
                    write(&domain.tables[&(l3, 512)], l2, address as u64 | 3);
                }
                // Legacy second-level leaf: R/W, no scalable EMT or snoop override.
                write(&domain.tables[&(l3, l2)], l1, (paddr + offset) as u64 | 3);
                mapped += PAGE_SIZE;
            }
            Ok(())
        })();
        if result.is_err() {
            domain.clear(iova, mapped);
        } else {
            // Never reuse an IOVA during a domain lifetime, including after unmap.
            domain.next_iova = end;
        }
        fence(Ordering::SeqCst);
        if !IOMMU_REGS.get().unwrap().lock().invalidate_dma_sync() {
            domains.poisoned = true;
            return Err(E::InvalidationFailed);
        }
        result.map(|()| iova)
    }
    pub(crate) fn unmap(&self, iova: usize, size: usize) -> Result<(), E> {
        let mut domains = DOMAINS.get().unwrap().lock();
        if domains.poisoned {
            return Err(E::InvalidationFailed);
        }
        let domain = domains
            .groups
            .get_mut(&self.location)
            .ok_or(E::MappingFailed)?;
        domain.clear(iova, size);
        if !IOMMU_REGS.get().unwrap().lock().invalidate_dma_sync() {
            domains.poisoned = true;
            return Err(E::InvalidationFailed);
        }
        Ok(())
    }
}
/// Validates group shape independently of firmware policy and hardware state.
/// All members must be distinct functions of one slot, include function zero,
/// and share a route prefix (the endpoint hop is necessarily different).
fn validate_group(
    requesters: &[(PciDmaRequester, &[(u8, u8)])],
) -> Result<Vec<PciDeviceLocation>, E> {
    if requesters.is_empty() || requesters.len() > 8 {
        return Err(E::InvalidRequester);
    }
    let mut members = Vec::with_capacity(requesters.len());
    let mut route_prefix: Option<&[(u8, u8)]> = None;
    let first = requesters[0].0;
    for (requester, path) in requesters {
        let member = location(*requester)?;
        if member.bus != first.bus()
            || member.device != first.device()
            || path.is_empty()
            || path.len() > 8
            || path.last() != Some(&(member.device, member.function))
        {
            return Err(E::InvalidRequester);
        }
        let prefix = &path[..path.len() - 1];
        if route_prefix.is_some_and(|expected| expected != prefix) {
            return Err(E::InvalidRequester);
        }
        route_prefix = Some(prefix);
        if members.contains(&member) {
            return Err(E::InvalidRequester);
        }
        members.push(member);
    }
    if !members.iter().any(|member| member.function == 0) {
        return Err(E::InvalidRequester);
    }
    Ok(members)
}

impl Domain {
    fn clear(&self, iova: usize, size: usize) {
        for offset in (0..size).step_by(PAGE_SIZE) {
            let address = iova + offset;
            if let Some(table) = self
                .tables
                .get(&((address >> 30) & 511, (address >> 21) & 511))
            {
                write(table, (address >> 12) & 511, 0);
            }
        }
        // Intermediate page tables deliberately remain alive until detach.
    }
}
impl Drop for DomainHandle {
    fn drop(&mut self) {
        let mut domains = DOMAINS.get().unwrap().lock();
        if domains.poisoned {
            return;
        }
        let mut roots = PAGE_TABLE.get().unwrap().lock();
        for member in &self.members {
            roots.clear_device_context(*member);
        }
        if !IOMMU_REGS.get().unwrap().lock().invalidate_dma_sync() {
            domains.poisoned = true;
            return; // quarantined tables, DID and requester, never silently reclaimed
        }
        // Detached context remains absent. The legacy shared mapper is NOT restored.
        domains.groups.remove(&self.location);
        for member in &self.members {
            domains.members.remove(member);
        }
        domains.ids &= !(1 << self.did);
    }
}

#[cfg(ktest)]
mod tests {
    use super::*;
    use crate::prelude::ktest;

    #[ktest]
    fn rejects_reserved_root_and_requires_route_for_downstream_requesters() {
        let host = PciDmaRequester::new(0, 0, 0, 0).unwrap();
        assert_eq!(location(host).err(), Some(E::InvalidRequester));
        let downstream = PciDmaRequester::new(0, 1, 0, 0).unwrap();
        assert!(location(downstream).is_ok());
        assert_eq!(
            DomainHandle::new(downstream).err(),
            Some(E::InvalidRequester)
        );
        assert!(location(PciDmaRequester::new(0, 0, 31, 0).unwrap()).is_ok());
    }

    #[ktest]
    fn multifunction_groups_validate_shape_and_common_route() {
        let function_zero = PciDmaRequester::new(0, 1, 3, 0).unwrap();
        let function_one = PciDmaRequester::new(0, 1, 3, 1).unwrap();
        let route_zero = [(2, 0), (3, 0)];
        let route_one = [(2, 0), (3, 1)];
        assert_eq!(
            validate_group(&[(function_zero, &route_zero), (function_one, &route_one),])
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            validate_group(&[(function_one, &route_one)]).err(),
            Some(E::InvalidRequester),
            "function zero is mandatory"
        );
        assert_eq!(
            validate_group(&[(function_zero, &route_zero), (function_zero, &route_zero),]).err(),
            Some(E::InvalidRequester),
            "duplicate BDFs are forbidden"
        );
        let other_slot = PciDmaRequester::new(0, 1, 4, 1).unwrap();
        let other_slot_route = [(2, 0), (4, 1)];
        assert_eq!(
            validate_group(&[
                (function_zero, &route_zero),
                (other_slot, &other_slot_route),
            ])
            .err(),
            Some(E::InvalidRequester),
            "all functions must belong to one slot"
        );
        let other_bus = PciDmaRequester::new(0, 2, 3, 1).unwrap();
        assert_eq!(
            validate_group(&[(function_zero, &route_zero), (other_bus, &route_one)]).err(),
            Some(E::InvalidRequester),
            "all functions must belong to one bus"
        );
        let wrong_prefix = [(5, 0), (3, 1)];
        assert_eq!(
            validate_group(&[(function_zero, &route_zero), (function_one, &wrong_prefix),]).err(),
            Some(E::InvalidRequester),
            "members must share the same upstream route"
        );
        let wrong_endpoint = [(2, 0), (3, 2)];
        assert_eq!(
            validate_group(&[(function_zero, &wrong_endpoint)]).err(),
            Some(E::InvalidRequester),
            "route must terminate at the exact requester"
        );
        let too_many: Vec<_> = (0..9)
            .map(|function| {
                (
                    PciDmaRequester::new(0, 1, 3, function % 8).unwrap(),
                    &route_zero[..],
                )
            })
            .collect();
        assert_eq!(validate_group(&too_many).err(), Some(E::InvalidRequester));
    }

    #[ktest]
    fn private_tables_never_share_roots_and_clear_only_leaves() {
        if core::arch::x86_64::__cpuid(1).edx & (1 << 19) == 0 {
            return;
        }
        let mut a = Domain {
            root: frame().unwrap(),
            tables: BTreeMap::new(),
            next_iova: PAGE_SIZE,
        };
        let b = Domain {
            root: frame().unwrap(),
            tables: BTreeMap::new(),
            next_iova: PAGE_SIZE,
        };
        assert_ne!(a.root.paddr(), b.root.paddr());
        let leaf = frame().unwrap();
        write(&leaf, 1, 0x12345003);
        write(&leaf, 2, 0x98765003);
        a.tables.insert((0, 0), leaf);
        a.clear(PAGE_SIZE, PAGE_SIZE);
        assert_eq!(a.tables[&(0, 0)].read_val::<u64>(8).unwrap(), 0);
        assert_eq!(a.tables[&(0, 0)].read_val::<u64>(16).unwrap(), 0x98765003);
        assert_eq!(
            a.tables.len(),
            1,
            "intermediate tables cannot be reclaimed by unmap"
        );
        assert_eq!(b.root.read_val::<u64>(0).unwrap(), 0);
    }
}
