from pathlib import Path
R=Path('/workspace/asterinas-gpui/asterinas')
def update(p,a,b):
 p=R/p;s=p.read_text();assert a in s,(p,a);p.write_text(s.replace(a,b,1))
update('ostd/src/mm/dma/device.rs','    InvalidSize,','    InvalidSize,\n    /// A domain already owns this requester.\n    DomainBusy,\n    /// All supported private hardware Domain IDs are owned.\n    DomainExhausted,\n    /// The private mapping is absent or invalid.\n    MappingFailed,\n    /// Hardware invalidation did not complete; resources are quarantined.\n    InvalidationFailed,')
update('ostd/src/mm/dma/mod.rs','mod device;','mod device;\n#[cfg(target_arch = "x86_64")]\nmod isolated;')
update('ostd/src/mm/dma/mod.rs','pub use dma_coherent::DmaCoherent;','#[cfg(target_arch = "x86_64")]\npub use isolated::{IsolatedDmaBuffer, IsolatedDmaContext};\npub use dma_coherent::DmaCoherent;')
update('ostd/src/arch/x86/iommu/dma_remapping/mod.rs','mod context_table;','mod context_table;\npub(super) mod isolated;\npub(crate) use isolated::DomainHandle;')
update('ostd/src/arch/x86/iommu/mod.rs','pub(crate) use dma_remapping::{IommuPtConfig, has_dma_remapping, map, unmap};','pub(crate) use dma_remapping::{DomainHandle, IommuPtConfig, has_dma_remapping, map, unmap};')
update('ostd/src/arch/x86/iommu/dma_remapping/context_table.rs','    fn get_or_create_context_table(&mut self, device_id: PciDeviceLocation) -> &mut ContextTable {','''    /// Clear Present before changing a context. Caller must complete cache/IOTLB
    /// invalidation before freeing its old private table or reusing its DID.
    pub(super) fn clear_device_context(&mut self, device: PciDeviceLocation) {
        let root_paddr = self.root_frame.paddr();
        let table = self.get_or_create_context_table(device);
        let offset = (device.device as usize * 8 + device.function as usize) * 16;
        table.entries_frame.write_val(offset, &0u64).unwrap();
        super::isolated::flush(table.paddr(), PAGE_SIZE);
        table.entries_frame.write_val(offset + 8, &0u64).unwrap();
        super::isolated::flush(table.paddr() + offset, 16);
        super::isolated::flush(root_paddr, PAGE_SIZE);
    }

    /// The entry must already be absent with invalidation completed. The high
    /// word is published/flushed first; the low Present word is published last.
    pub(super) fn install_isolated_context(&mut self, device: PciDeviceLocation, root: Paddr, did: u16) {
        let table = self.get_or_create_context_table(device);
        let offset = (device.device as usize * 8 + device.function as usize) * 16;
        table.entries_frame.write_val(offset + 8, &((u64::from(did) << 8) | 1)).unwrap();
        super::isolated::flush(table.paddr() + offset + 8, 8);
        table.entries_frame.write_val(offset, &(root as u64 | 1)).unwrap();
        super::isolated::flush(table.paddr() + offset, 8);
    }

    fn get_or_create_context_table(&mut self, device_id: PciDeviceLocation) -> &mut ContextTable {''')
update('ostd/src/arch/x86/iommu/registers/capability.rs','    const fn flags(&self) -> CapabilityFlags {','    pub(super) const fn flags(&self) -> CapabilityFlags {')
update('ostd/src/arch/x86/iommu/registers/capability.rs','    const fn domain_support_number(&self) -> u64 {','    pub(super) const fn domain_support_number(&self) -> u64 {')
update('ostd/src/arch/x86/iommu/invalidate/queue.rs','    pub(in crate::arch::iommu) fn tail(&self) -> usize {','''    pub(in crate::arch::iommu) fn flush_for_dma(&self) {
        super::super::dma_remapping::isolated::flush(self.segment.paddr(), self.segment.size());
    }

    pub(in crate::arch::iommu) fn tail(&self) -> usize {''')
update('ostd/src/arch/x86/iommu/invalidate/queue.rs','mm::{FrameAllocOptions, PAGE_SIZE, Segment, VmIo}', 'mm::{FrameAllocOptions, HasSize, PAGE_SIZE, Segment, VmIo}')
update('ostd/src/arch/x86/iommu/registers/mod.rs','    fn global_invalidation(&mut self) {','''    /// Minimum bounded backend: queued invalidation with read/write draining,
    /// no required write-buffer flush, valid architectural DID capacity.
    pub(in crate::arch::iommu) fn isolated_domains_supported(&self) -> bool {
        let cap = self.read_capability();
        let flags = cap.flags();
        self.read_global_status().contains(GlobalStatus::TES | GlobalStatus::QIES)
            && flags.contains(capability::CapabilityFlags::DRD | capability::CapabilityFlags::DWD)
            && !flags.contains(capability::CapabilityFlags::RWBF)
            && cap.domain_support_number() != 7
    }

    /// Synchronous global context-cache + IOTLB invalidation. The conservative
    /// global granularity is deliberate. No ATS endpoints are admitted, so no
    /// device-TLB cache exists in this API's supported subset.
    /// On timeout the queue may still contain commands: callers permanently
    /// poison the backend and retain all possibly reachable physical storage.
    pub(in crate::arch::iommu) fn invalidate_dma_sync(&mut self) -> bool {
        use core::sync::atomic::{Ordering, fence};
        if !self.isolated_domains_supported() { return false; }
        let mut queue = QUEUE.get().unwrap().lock();
        // All users hold IOMMU_REGS; no prior operation is asynchronous. Clear
        // stale ICS before publishing the new descriptor chain.
        self.invalidate.completion_status.as_mut_ptr().write(1);
        queue.append_descriptor(1 | (1 << 4)); // CC, global
        queue.append_descriptor(2 | (1 << 4) | (1 << 6) | (1 << 7)); // IOTLB, global, DW+DR
        queue.append_descriptor(5 | (1 << 4) | (1 << 6)); // wait, IF+FN
        queue.flush_for_dma();
        fence(Ordering::SeqCst);
        self.invalidate.queue_tail.as_mut_ptr().write((queue.tail() << 4) as u64);
        for _ in 0..10_000_000 {
            if self.invalidate.completion_status.as_ptr().read() & 1 != 0 {
                self.invalidate.completion_status.as_mut_ptr().write(1);
                fence(Ordering::SeqCst);
                return true;
            }
            core::hint::spin_loop();
        }
        false
    }

    fn global_invalidation(&mut self) {''')
# Register path must also await IOTLB completion; existing fallback is not used by domains.
update('ostd/src/arch/x86/iommu/registers/mod.rs','            .write(0x9000_0000_0000_0000);','''            .write(0x9000_0000_0000_0000);
        while self.invalidate.iotlb_invalidate.as_ptr().read() & (1 << 63) != 0 {
            core::hint::spin_loop();
        }''')
