// SPDX-License-Identifier: MPL-2.0
//! Direct DMA for a PCI requester, restricted to coherent x86-64 systems with
//! no detected IOMMU. This mode does not provide requester isolation: the PCI
//! layer must bind the metadata to a live device borrow, and callers must
//! quiesce all DMA before dropping a buffer.
use crate::{
    Error, irq,
    mm::{
        FrameAllocOptions, HasDaddr, HasPaddr, HasSize, Infallible, PAGE_SIZE, Segment, VmReader,
        VmWriter,
        io::util::{HasVmReaderWriter, VmReaderWriterIdentity},
    },
};

/// Errors returned by direct device-DMA APIs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceDmaError {
    /// Physical memory allocation failed.
    Memory(Error),
    /// Creating a context or buffer requires IRQs enabled and non-interrupt context.
    InvalidContext,
    /// The platform is not a supported coherent, non-IOMMU x86-64 system.
    UnsupportedPlatform,
    /// The PCI requester coordinates are not supported or valid.
    InvalidRequester,
    /// The DMA address mask is invalid.
    InvalidMask,
    /// The allocated physical range exceeds the device address mask.
    AddressOutOfRange,
    /// The requested buffer size is zero or cannot be represented.
    InvalidSize,
    /// A domain already owns this requester.
    DomainBusy,
    /// All supported private hardware Domain IDs are owned.
    DomainExhausted,
    /// The private mapping is absent or invalid.
    MappingFailed,
    /// Hardware invalidation did not complete; resources are quarantined.
    InvalidationFailed,
}

/// A PCI requester identity. It is metadata, not an isolation or authorization
/// mechanism; the PCI layer is responsible for tying it to a device borrow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PciDmaRequester {
    segment: u16,
    bus: u8,
    device: u8,
    function: u8,
}
impl PciDmaRequester {
    /// Creates a requester identity. PCI segment groups are currently unsupported.
    pub fn new(segment: u16, bus: u8, device: u8, function: u8) -> Result<Self, DeviceDmaError> {
        if segment != 0 || device > 31 || function > 7 {
            return Err(DeviceDmaError::InvalidRequester);
        }
        Ok(Self {
            segment,
            bus,
            device,
            function,
        })
    }
    /// Returns the PCI segment group (always zero on supported systems).
    pub const fn segment(&self) -> u16 {
        self.segment
    }
    /// Returns the PCI bus number.
    pub const fn bus(&self) -> u8 {
        self.bus
    }
    /// Returns the PCI device number.
    pub const fn device(&self) -> u8 {
        self.device
    }
    /// Returns the PCI function number.
    pub const fn function(&self) -> u8 {
        self.function
    }
}

/// The inclusive maximum DMA address permitted by a device's address width.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmaMask {
    bits: u8,
    max_address: u64,
}
impl DmaMask {
    /// Creates an address mask from a supported width in the range 1..=64.
    pub fn new(bits: u8) -> Result<Self, DeviceDmaError> {
        if !(1..=64).contains(&bits) {
            return Err(DeviceDmaError::InvalidMask);
        }
        let max_address = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        Ok(Self { bits, max_address })
    }
    /// Returns the number of address bits.
    pub const fn bits(&self) -> u8 {
        self.bits
    }
    /// Returns the largest representable DMA address.
    pub const fn max_address(&self) -> u64 {
        self.max_address
    }
    /// Returns whether the non-empty address range fits this mask.
    ///
    /// The end is inclusive. Zero-sized ranges and overflowing ranges are
    /// rejected, including the `u64::MAX` boundary case unless the final byte
    /// itself is representable.
    pub fn contains_range(&self, start: u64, size: usize) -> bool {
        if size == 0 {
            return false;
        }
        let Some(last) = start.checked_add((size - 1) as u64) else {
            return false;
        };
        last <= self.max_address
    }
}

/// A direct-DMA context. It does not create an IOMMU domain or isolate devices.
pub struct DirectDmaContext {
    requester: PciDmaRequester,
    mask: DmaMask,
}
impl DirectDmaContext {
    /// Creates a context only when the platform has no detected IOMMU/remapping.
    pub fn new(requester: PciDmaRequester, mask: DmaMask) -> Result<Self, DeviceDmaError> {
        if !irq::is_local_enabled() || irq::InterruptLevel::current().is_interrupt_context() {
            return Err(DeviceDmaError::InvalidContext);
        }
        if requester.segment != 0 || requester.device > 31 || requester.function > 7 {
            return Err(DeviceDmaError::InvalidRequester);
        }
        if !direct_mode_supported() {
            return Err(DeviceDmaError::UnsupportedPlatform);
        }
        Ok(Self { requester, mask })
    }
    /// Returns the requester metadata associated with this context.
    pub const fn requester(&self) -> &PciDmaRequester {
        &self.requester
    }
    /// Returns the DMA address mask associated with this context.
    pub const fn mask(&self) -> &DmaMask {
        &self.mask
    }
    /// Allocates zeroed, contiguous, coherent memory usable for direct DMA.
    ///
    /// The caller must stop/quiesce device DMA before dropping the returned buffer.
    pub fn alloc_coherent(&self, nframes: usize) -> Result<DirectDmaBuffer<'_>, DeviceDmaError> {
        if !irq::is_local_enabled() || irq::InterruptLevel::current().is_interrupt_context() {
            return Err(DeviceDmaError::InvalidContext);
        }
        if nframes == 0 {
            return Err(DeviceDmaError::InvalidSize);
        }
        let size = nframes
            .checked_mul(PAGE_SIZE)
            .ok_or(DeviceDmaError::InvalidSize)?;
        if !direct_mode_supported() {
            return Err(DeviceDmaError::UnsupportedPlatform);
        }
        let segment = FrameAllocOptions::new()
            .alloc_segment(nframes)
            .map_err(DeviceDmaError::Memory)?;
        if !self.mask.contains_range(segment.paddr() as u64, size) {
            // `segment` is dropped here, returning the allocation to the frame allocator.
            return Err(DeviceDmaError::AddressOutOfRange);
        }
        Ok(DirectDmaBuffer {
            segment,
            _context: self,
        })
    }
}

/// An owned DMA buffer. `daddr()` equals the physical address only because this
/// API is enabled solely for coherent direct-DMA mode without an IOMMU.
pub struct DirectDmaBuffer<'a> {
    segment: Segment<()>,
    _context: &'a DirectDmaContext,
}
impl HasDaddr for DirectDmaBuffer<'_> {
    fn daddr(&self) -> usize {
        self.segment.paddr()
    }
}
impl HasSize for DirectDmaBuffer<'_> {
    fn size(&self) -> usize {
        self.segment.size()
    }
}
impl HasVmReaderWriter for DirectDmaBuffer<'_> {
    type Types = VmReaderWriterIdentity;
    fn reader(&self) -> VmReader<'_, Infallible> {
        <Segment<()> as HasVmReaderWriter>::reader(&self.segment)
    }
    fn writer(&self) -> VmWriter<'_, Infallible> {
        <Segment<()> as HasVmReaderWriter>::writer(&self.segment)
    }
}

fn direct_mode_supported() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        // TDX is the CVM implementation currently supported by this OSTD.
        let is_cvm = crate::arch::if_tdx_enabled!({ true } else { false });
        !is_cvm && crate::arch::iommu::direct_dma_supported()
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

#[cfg(ktest)]
mod test {
    use super::*;
    use crate::prelude::ktest;

    #[ktest]
    fn requester_identity_and_validation() {
        let requester = PciDmaRequester::new(0, 0x12, 3, 7).unwrap();
        assert_eq!(
            (
                requester.segment(),
                requester.bus(),
                requester.device(),
                requester.function()
            ),
            (0, 0x12, 3, 7)
        );
        assert_eq!(
            PciDmaRequester::new(1, 0, 0, 0),
            Err(DeviceDmaError::InvalidRequester)
        );
        assert_eq!(
            PciDmaRequester::new(0, 0, 32, 0),
            Err(DeviceDmaError::InvalidRequester)
        );
        assert_eq!(
            PciDmaRequester::new(0, 0, 0, 8),
            Err(DeviceDmaError::InvalidRequester)
        );
    }

    #[ktest]
    fn masks_cover_boundaries_and_overflow() {
        assert_eq!(DmaMask::new(0), Err(DeviceDmaError::InvalidMask));
        assert_eq!(DmaMask::new(65), Err(DeviceDmaError::InvalidMask));
        let mask = DmaMask::new(64).unwrap();
        assert_eq!(mask.bits(), 64);
        assert_eq!(mask.max_address(), u64::MAX);
        assert!(mask.contains_range(u64::MAX, 1));
        assert!(!mask.contains_range(u64::MAX, 2));
        assert!(!mask.contains_range(u64::MAX - 4, 6));
        assert!(!mask.contains_range(0, 0));
        assert!(
            DmaMask::new(32)
                .unwrap()
                .contains_range(u32::MAX as u64 - 3, 4)
        );
        assert!(
            !DmaMask::new(32)
                .unwrap()
                .contains_range(u32::MAX as u64 - 3, 5)
        );
    }

    #[ktest]
    fn allocation_requires_enabled_interrupts_and_supported_platform() {
        let requester = PciDmaRequester::new(0, 0, 0, 0).unwrap();
        let mask = DmaMask::new(64).unwrap();
        if !direct_mode_supported() {
            assert_eq!(
                DirectDmaContext::new(requester, mask).err(),
                Some(DeviceDmaError::UnsupportedPlatform)
            );
            return;
        }
        let context = DirectDmaContext::new(requester, mask).unwrap();
        assert_eq!(
            context.alloc_coherent(0).err(),
            Some(DeviceDmaError::InvalidSize)
        );
        assert_eq!(
            context.alloc_coherent(usize::MAX).err(),
            Some(DeviceDmaError::InvalidSize)
        );
        let irq_guard = irq::disable_local();
        assert_eq!(
            DirectDmaContext::new(requester, mask).err(),
            Some(DeviceDmaError::InvalidContext)
        );
        assert_eq!(
            context.alloc_coherent(1).err(),
            Some(DeviceDmaError::InvalidContext)
        );
        drop(irq_guard);
        let buffer = context.alloc_coherent(1).unwrap();
        assert_eq!(buffer.size(), PAGE_SIZE);
        let mut bytes = alloc::vec![0xA5; PAGE_SIZE];
        let mut reader = <DirectDmaBuffer<'_> as HasVmReaderWriter>::reader(&buffer);
        assert_eq!(
            reader.read(&mut VmWriter::from(bytes.as_mut_slice())),
            PAGE_SIZE
        );
        assert!(bytes.iter().all(|byte| *byte == 0));
        let source = alloc::vec![0x5A; PAGE_SIZE];
        let mut writer = <DirectDmaBuffer<'_> as HasVmReaderWriter>::writer(&buffer);
        assert_eq!(
            writer.write(&mut VmReader::from(source.as_slice())),
            PAGE_SIZE
        );
        let mut result = alloc::vec![0; PAGE_SIZE];
        let mut reader = <DirectDmaBuffer<'_> as HasVmReaderWriter>::reader(&buffer);
        assert_eq!(
            reader.read(&mut VmWriter::from(result.as_mut_slice())),
            PAGE_SIZE
        );
        assert!(result.iter().all(|byte| *byte == 0x5A));
    }

    #[ktest]
    fn tiny_mask_rejection_releases_allocations() {
        if !direct_mode_supported() {
            return;
        }
        let context = DirectDmaContext::new(
            PciDmaRequester::new(0, 0, 0, 0).unwrap(),
            DmaMask::new(1).unwrap(),
        )
        .unwrap();
        for _ in 0..8 {
            assert_eq!(
                context.alloc_coherent(1).err(),
                Some(DeviceDmaError::AddressOutOfRange)
            );
        }
    }
}
