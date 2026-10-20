// SPDX-License-Identifier: MPL-2.0

//! Registers and their definition used by IOMMU.

mod capability;
mod command;
mod extended_cap;
mod invalidation;
mod status;

use core::ptr::NonNull;

use bit_field::BitField;
pub(super) use capability::{Capability, CapabilitySagaw};
use command::GlobalCommand;
use extended_cap::ExtendedCapability;
pub(super) use extended_cap::ExtendedCapabilityFlags;
use invalidation::InvalidationRegisters;
use spin::Once;
use status::GlobalStatus;
use volatile::{
    VolatileRef,
    access::{ReadOnly, ReadWrite, WriteOnly},
};

use super::{
    IommuError, dma_remapping::RootTable, interrupt_remapping::IntRemappingTable,
    invalidate::queue::Queue,
};
use crate::{
    arch::{
        iommu::{
            fault,
            invalidate::{
                QUEUE,
                descriptor::{InterruptEntryCache, InvalidationWait},
            },
        },
        kernel::acpi::vtd_policy,
    },
    debug,
    io::{IoMem, IoMemAllocatorBuilder, Sensitive},
    mm::{CachePolicy, PAGE_SIZE},
    sync::{LocalIrqDisabled, SpinLock},
};

#[derive(Clone, Copy, Debug)]
pub(super) struct IommuVersion {
    major: u8,
    minor: u8,
}

impl IommuVersion {
    /// Major version number
    #[expect(dead_code)]
    pub(super) fn major(&self) -> u8 {
        self.major
    }

    /// Minor version number
    #[expect(dead_code)]
    pub(super) fn minor(&self) -> u8 {
        self.minor
    }
}

/// Important registers used by IOMMU.
#[derive(Debug)]
pub(super) struct IommuRegisters {
    // All VolatileRef register views below borrow this permanent owned mapping.
    _mmio: IoMem<Sensitive>,
    version: VolatileRef<'static, u32, ReadOnly>,
    capability: VolatileRef<'static, u64, ReadOnly>,
    extended_capability: VolatileRef<'static, u64, ReadOnly>,
    global_command: VolatileRef<'static, u32, WriteOnly>,
    global_status: VolatileRef<'static, u32, ReadOnly>,
    root_table_address: VolatileRef<'static, u64, ReadWrite>,
    context_command: VolatileRef<'static, u64, ReadWrite>,

    interrupt_remapping_table_addr: VolatileRef<'static, u64, ReadWrite>,

    invalidate: InvalidationRegisters,
    invalidation_failed: bool,
}

impl IommuRegisters {
    /// Reads the version of IOMMU
    #[expect(dead_code)]
    pub(super) fn read_version(&self) -> IommuVersion {
        let version = self.version.as_ptr().read();
        IommuVersion {
            major: version.get_bits(4..8) as u8,
            minor: version.get_bits(0..4) as u8,
        }
    }

    /// Reads the capability of IOMMU
    pub(super) fn read_capability(&self) -> Capability {
        Capability::new(self.capability.as_ptr().read())
    }

    /// Reads the extended Capability of IOMMU
    pub(super) fn read_extended_capability(&self) -> ExtendedCapability {
        ExtendedCapability::new(self.extended_capability.as_ptr().read())
    }

    /// Reads the global Status of IOMMU
    pub(super) fn read_global_status(&self) -> GlobalStatus {
        GlobalStatus::from_bits_truncate(self.global_status.as_ptr().read())
    }

    /// Enables DMA remapping with static RootTable
    pub(super) fn enable_dma_remapping(
        &mut self,
        root_table: &'static SpinLock<RootTable, LocalIrqDisabled>,
    ) {
        // Set root table address
        self.root_table_address
            .as_mut_ptr()
            .write(root_table.lock().root_paddr() as u64);
        self.write_global_command(GlobalCommand::SRTP, true);
        while !self.read_global_status().contains(GlobalStatus::RTPS) {}

        // Enable DMA remapping
        self.write_global_command(GlobalCommand::TE, true);
        while !self.read_global_status().contains(GlobalStatus::TES) {}
    }

    /// Enables Interrupt Remapping with IntRemappingTable
    pub(super) fn enable_interrupt_remapping(&mut self, table: &'static IntRemappingTable) {
        assert!(
            self.read_extended_capability()
                .flags()
                .contains(ExtendedCapabilityFlags::IR)
        );
        // Set interrupt remapping table address
        self.interrupt_remapping_table_addr
            .as_mut_ptr()
            .write(table.encode());
        self.write_global_command(GlobalCommand::SIRTP, true);
        while !self.read_global_status().contains(GlobalStatus::IRTPS) {}

        // Enable Interrupt Remapping
        self.write_global_command(GlobalCommand::IRE, true);
        while !self.read_global_status().contains(GlobalStatus::IRES) {}

        // Disable Compatibility format interrupts
        if self.read_global_status().contains(GlobalStatus::CFIS) {
            self.write_global_command(GlobalCommand::CFI, false);
            while self.read_global_status().contains(GlobalStatus::CFIS) {}
        }
    }

    pub(super) fn enable_queued_invalidation(&mut self, queue: &Queue) {
        assert!(
            self.read_extended_capability()
                .flags()
                .contains(ExtendedCapabilityFlags::QI)
        );
        self.invalidate.queue_tail.as_mut_ptr().write(0);

        let mut write_value = queue.base_paddr() as u64;
        // By default, we set descriptor width to 128-bit(0)
        let descriptor_width = 0b0;
        write_value |= descriptor_width << 11;

        let write_queue_size = {
            let mut queue_size = queue.size();
            assert!(queue_size.is_power_of_two());
            let mut write_queue_size = 0;

            if descriptor_width == 0 {
                // 2^(write_queue_size + 8) = number of entries = queue_size
                assert!(queue_size >= (1 << 8));
                queue_size >>= 8;
            } else {
                // 2^(write_queue_size + 7) = number of entries = queue_size
                assert!(queue_size >= (1 << 7));
                queue_size >>= 7;
            };

            while queue_size & 0b1 == 0 {
                queue_size >>= 1;
                write_queue_size += 1;
            }
            write_queue_size
        };

        write_value |= write_queue_size;

        self.invalidate.queue_addr.as_mut_ptr().write(write_value);

        // Enable queued invalidation
        self.write_global_command(GlobalCommand::QIE, true);
        while !self.read_global_status().contains(GlobalStatus::QIES) {}

        // Clear the Invalidation Completion Status Register
        self.invalidate.completion_status.as_mut_ptr().write(1);
    }

    // Invalidates Interrupt-remapping cache.
    pub(super) fn invalidate_interrupt_cache(&mut self) {
        assert!(
            !self.invalidation_failed,
            "VT-d invalidation queue is quarantined"
        );
        if !self.read_global_status().contains(GlobalStatus::QIES) {
            self.global_invalidation();
            return;
        }

        let mut queue = QUEUE.get().unwrap().lock();

        // Currently, we don't support asynchronous processing in the queue. Therefore, when we
        // lock the queue, we know that it is empty and that the Invalidation Completion Status
        // Register has been cleared.

        // Construct an Interrupt Entry Cache Invalidate Descriptor.
        queue.append_descriptor(InterruptEntryCache::global_invalidation().0);
        // Construct an Invalidation Wait Descriptor. We need to set the interrupt flag so that the
        // Invalidation Completion Status Register can report the completion status.
        queue.append_descriptor(InvalidationWait::with_interrupt_flag().0);

        // Update the queue tail.
        let tail = queue.tail();
        self.invalidate
            .queue_tail
            .as_mut_ptr()
            .write((tail << 4) as u64);

        // Wait for completion.
        while self.invalidate.completion_status.as_ptr().read() == 0 {}
        // Clear the Invalidation Completion Status Register.
        self.invalidate.completion_status.as_mut_ptr().write(1);
    }

    /// Minimum bounded backend: queued invalidation with read/write draining,
    /// no required write-buffer flush, valid architectural DID capacity.
    pub(in crate::arch::iommu) fn isolated_domains_supported(&self) -> bool {
        let cap = self.read_capability();
        let flags = cap.flags();
        self.read_global_status()
            .contains(GlobalStatus::TES | GlobalStatus::QIES)
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
        if self.invalidation_failed || !self.isolated_domains_supported() {
            return false;
        }
        let mut queue = QUEUE.get().unwrap().lock();
        // All users hold IOMMU_REGS; no prior operation is asynchronous. Clear
        // stale ICS before publishing the new descriptor chain.
        self.invalidate.completion_status.as_mut_ptr().write(1);
        queue.append_descriptor(1 | (1 << 4)); // CC, global
        queue.append_descriptor(2 | (1 << 4) | (1 << 6) | (1 << 7)); // IOTLB, global, DW+DR
        queue.append_descriptor(5 | (1 << 4) | (1 << 6)); // wait, IF+FN
        queue.flush_for_dma();
        fence(Ordering::SeqCst);
        self.invalidate
            .queue_tail
            .as_mut_ptr()
            .write((queue.tail() << 4) as u64);
        for _ in 0..10_000_000 {
            if self.invalidate.completion_status.as_ptr().read() & 1 != 0 {
                self.invalidate.completion_status.as_mut_ptr().write(1);
                fence(Ordering::SeqCst);
                return true;
            }
            core::hint::spin_loop();
        }
        self.invalidation_failed = true;
        false
    }

    fn global_invalidation(&mut self) {
        // Set ICC(63) to 1 to requests invalidation and CIRG(62:61) to 01 to indicate global invalidation request.
        self.context_command
            .as_mut_ptr()
            .write(0xA000_0000_0000_0000);

        // Wait for invalidation complete (ICC set to 0).
        let mut value = 0x8000_0000_0000_0000;
        while (value & 0x8000_0000_0000_0000) != 0 {
            value = self.context_command.as_ptr().read();
        }

        // Set IVT(63) to 1 to requests IOTLB invalidation and IIRG(61:60) to 01 to indicate global invalidation request.
        self.invalidate
            .iotlb_invalidate
            .as_mut_ptr()
            .write(0x9000_0000_0000_0000);
        while self.invalidate.iotlb_invalidate.as_ptr().read() & (1 << 63) != 0 {
            core::hint::spin_loop();
        }
    }

    /// Writes value to the global command register. This function will not wait until the command
    /// is serviced. User need to check the global status register.
    fn write_global_command(&mut self, command: GlobalCommand, enable: bool) {
        const ONE_SHOT_STATUS_MASK: u32 = 0x96FF_FFFF;
        let status = self.global_status.as_ptr().read() & ONE_SHOT_STATUS_MASK;
        if enable {
            self.global_command
                .as_mut_ptr()
                .write(status | command.bits());
        } else {
            self.global_command
                .as_mut_ptr()
                .write(status & !command.bits());
        }
    }

    /// Creates an instance from base address
    fn new(io_mem_builder: &IoMemAllocatorBuilder) -> Option<Self> {
        let base_address = match vtd_policy::validated_register_base() {
            Ok(base) => base,
            Err(error) => {
                crate::warn!("VT-d topology unavailable or unsupported: {error:?}");
                return None;
            }
        };
        debug!("validated single segment-zero INCLUDE_ALL DRHD: {base_address:#x}");

        let base_address = usize::try_from(base_address).ok()?;
        if !base_address.is_multiple_of(PAGE_SIZE) {
            crate::warn!("IOMMU register base is not page aligned");
            return None;
        }
        let end = base_address.checked_add(PAGE_SIZE)?;
        let mmio = io_mem_builder.reserve(base_address..end, CachePolicy::Uncacheable);
        let base = NonNull::new(mmio.base() as *mut u8).unwrap();

        // SAFETY: CAP/ECAP are within the first mapped page of the trusted DRHD MMIO
        // aperture. The mapping is owned here and retained with all register views.
        let (capability, extended) = unsafe {
            (
                Capability::new(
                    VolatileRef::new_read_only(base.add(0x08).cast::<u64>())
                        .as_ptr()
                        .read(),
                ),
                ExtendedCapability::new(
                    VolatileRef::new_read_only(base.add(0x10).cast::<u64>())
                        .as_ptr()
                        .read(),
                ),
            )
        };
        // The present backend owns one page, not an arbitrary capability-selected
        // aperture. Reject unsupported layouts BEFORE constructing dereferenceable
        // fault/IOTLB views. Supporting larger register windows requires a separate
        // measured reservation; do not walk beyond this real MMIO mapping.
        if !register_window_fits(
            capability.fault_recording_register_offset() as usize,
            capability.fault_recording_number() as usize + 1,
            extended.iotlb_register_offset() as usize,
        ) {
            crate::warn!("IOMMU fault/IOTLB register layout exceeds supported MMIO window");
            return None;
        }

        // SAFETY:
        // - We trust the ACPI tables (as well as the DRHD in them), from which the base address is
        //   obtained, so it is a valid IOMMU base address.
        // - The sensitive reservation owns the actual uncacheable virtual mapping.
        // - All dynamically selected register offsets were checked against its size.
        let iommu_regs = unsafe {
            fault::init(base);

            Self {
                _mmio: mmio,
                version: VolatileRef::new_read_only(base.cast::<u32>()),
                capability: VolatileRef::new_read_only(base.add(0x08).cast::<u64>()),
                extended_capability: VolatileRef::new_read_only(base.add(0x10).cast::<u64>()),
                global_command: VolatileRef::new_restricted(
                    WriteOnly,
                    base.add(0x18).cast::<u32>(),
                ),
                global_status: VolatileRef::new_read_only(base.add(0x1C).cast::<u32>()),
                root_table_address: VolatileRef::new(base.add(0x20).cast::<u64>()),
                context_command: VolatileRef::new(base.add(0x28).cast::<u64>()),

                interrupt_remapping_table_addr: VolatileRef::new(base.add(0xb8).cast::<u64>()),

                invalidate: InvalidationRegisters::new(base),
                invalidation_failed: false,
            }
        };

        debug!("registers: {:#x?}", iommu_regs);
        debug!("capability: {:#x?}", iommu_regs.read_capability());
        debug!(
            "extend capability: {:#x?}",
            iommu_regs.read_extended_capability()
        );

        Some(iommu_regs)
    }
}

pub(super) static IOMMU_REGS: Once<SpinLock<IommuRegisters, LocalIrqDisabled>> = Once::new();

pub(super) fn init(io_mem_builder: &IoMemAllocatorBuilder) -> Result<(), IommuError> {
    let iommu_regs = IommuRegisters::new(io_mem_builder).ok_or(IommuError::NoIommu)?;
    IOMMU_REGS.call_once(|| SpinLock::new(iommu_regs));
    Ok(())
}

fn register_window_fits(
    fault_offset_units: usize,
    fault_count: usize,
    iotlb_offset_units: usize,
) -> bool {
    let fault_end = fault_offset_units.checked_mul(16).and_then(|offset| {
        fault_count
            .checked_mul(16)
            .and_then(|size| offset.checked_add(size))
    });
    let iotlb_end = iotlb_offset_units
        .checked_mul(16)
        .and_then(|offset| offset.checked_add(16));
    fault_count != 0
        && fault_end.is_some_and(|end| end <= PAGE_SIZE)
        && iotlb_end.is_some_and(|end| end <= PAGE_SIZE)
}

#[cfg(ktest)]
mod tests {
    use super::register_window_fits;
    use crate::prelude::ktest;

    #[ktest]
    fn register_window_rejects_overflow_and_out_of_mapping_views() {
        assert!(register_window_fits(0x20, 1, 0xf));
        assert!(register_window_fits(255, 1, 255));
        assert!(!register_window_fits(255, 2, 255));
        assert!(!register_window_fits(0x20, 1, 256));
        assert!(!register_window_fits(0x20, 0, 0xf));
        assert!(!register_window_fits(usize::MAX, 1, 0xf));
        assert!(!register_window_fits(0x20, usize::MAX, 0xf));
        assert!(!register_window_fits(0x20, 1, usize::MAX));
    }
}
