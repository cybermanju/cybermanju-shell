// SPDX-License-Identifier: MPL-2.0

//! Experimental, opt-in NVIDIA 615.78.08 NVport memory integration.
//!
//! This is not a GPU driver. The private C ABI and NVIDIA code extend OSTD's
//! trusted computing base. No PCI registration, DMA, RM startup, or Vulkan
//! acceleration is provided. See the crate README for the exact boundary.

#![no_std]
#![warn(missing_docs)]

extern crate alloc;

#[cfg(target_os = "none")]
mod diagnostics;
pub mod firmware;
mod os;
pub mod runtime;
mod runtime_os;
#[cfg(target_os = "none")]
pub mod sync;

use core::{ffi::c_void, ptr::NonNull};

unsafe extern "C" {
    fn _portMemAllocNonPagedUntracked(len: u64) -> *mut c_void;
    fn _portMemFreeUntracked(ptr: *mut c_void);
    fn portMemCopy(dst: *mut c_void, dst_len: u64, src: *const c_void, src_len: u64)
    -> *mut c_void;
    fn portMemSet(dst: *mut c_void, value: u8, len: u64) -> *mut c_void;
    fn portMemCmp(a: *const c_void, b: *const c_void, len: u64) -> i32;
    fn portMemMove(dst: *mut c_void, dst_len: u64, src: *const c_void, src_len: u64)
    -> *mut c_void;
    fn portMemSetPattern(
        dst: *mut c_void,
        len: u64,
        pattern: *const u8,
        pattern_len: u64,
    ) -> *mut c_void;
    fn portMemExGetPageSize() -> u64;
}

/// A failure reported by the real NVport memory integration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Zero, oversized, empty-pattern, or out-of-bounds operation.
    InvalidArgument,
    /// The kernel/host heap could not allocate the requested storage.
    NoMemory,
    /// The NVIDIA routine returned an unexpected pointer or data.
    ContractViolation,
}

/// Exclusively owned, initialized CPU kernel heap storage allocated through
/// NVIDIA NVport and the real OSTD global heap. This is not GPU/DMA memory.
///
/// Lengths are capped at u32::MAX because the upstream Unix NVport memory
/// functions narrow NvLength to the u32 os_mem_* ABI.
pub struct Buffer {
    ptr: NonNull<u8>,
    len: u32,
}

impl Buffer {
    /// Allocates and zeroes a buffer. Requires initialized heap services.
    pub fn new(len: usize) -> Result<Self, Error> {
        let len = u32::try_from(len)
            .ok()
            .filter(|&len| len > 0)
            .ok_or(Error::InvalidArgument)?;
        // SAFETY: No raw pointer is supplied; the reviewed implementation calls
        // our checked heap allocator and returns null on allocation failure.
        let ptr =
            NonNull::new(unsafe { _portMemAllocNonPagedUntracked(u64::from(len)) }.cast::<u8>())
                .ok_or(Error::NoMemory)?;
        let mut buffer = Self { ptr, len };
        buffer.fill(0)?;
        Ok(buffer)
    }

    /// Returns the initialized, exclusively owned storage as a shared slice.
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: All bytes were initialized before construction completed. The
        // allocation stays live, and no mutation is possible while borrowed.
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len as usize) }
    }

    /// Fills all storage using the NVIDIA routine and OSTD OS-service ABI.
    pub fn fill(&mut self, value: u8) -> Result<(), Error> {
        // SAFETY: The allocation is live and exclusively borrowed for len bytes.
        let result = unsafe { portMemSet(self.ptr.as_ptr().cast(), value, u64::from(self.len)) };
        self.check_pointer(result)
    }

    /// Copies a slice of exactly the buffer length through NVIDIA NVport.
    pub fn copy_from_slice(&mut self, source: &[u8]) -> Result<(), Error> {
        if source.len() != self.len as usize {
            return Err(Error::InvalidArgument);
        }
        // SAFETY: Safe Rust borrowing makes source disjoint from this mutable
        // buffer; both live ranges have the exact checked length.
        let result = unsafe {
            portMemCopy(
                self.ptr.as_ptr().cast(),
                u64::from(self.len),
                source.as_ptr().cast(),
                source.len() as u64,
            )
        };
        self.check_pointer(result)
    }

    /// Compares all bytes using the original NVIDIA comparison routine.
    pub fn equals(&self, other: &[u8]) -> bool {
        if other.len() != self.len as usize {
            return false;
        }
        // SAFETY: Both ranges are initialized and read-only for this borrow.
        unsafe {
            portMemCmp(
                self.ptr.as_ptr().cast(),
                other.as_ptr().cast(),
                u64::from(self.len),
            ) == 0
        }
    }

    /// Repeats a nonempty pattern, including its partial last repetition.
    pub fn fill_pattern(&mut self, pattern: &[u8]) -> Result<(), Error> {
        if pattern.is_empty() || pattern.len() > u32::MAX as usize {
            return Err(Error::InvalidArgument);
        }
        // SAFETY: The source is a nonempty disjoint immutable slice. NVIDIA's
        // loop copies only the remaining length, bounded by our allocation.
        let result = unsafe {
            portMemSetPattern(
                self.ptr.as_ptr().cast(),
                u64::from(self.len),
                pattern.as_ptr(),
                pattern.len() as u64,
            )
        };
        self.check_pointer(result)
    }

    /// Moves bytes within this buffer, including overlapping ranges.
    pub fn copy_within(
        &mut self,
        source: core::ops::Range<usize>,
        destination: usize,
    ) -> Result<(), Error> {
        let count = source
            .end
            .checked_sub(source.start)
            .ok_or(Error::InvalidArgument)?;
        let end = destination
            .checked_add(count)
            .ok_or(Error::InvalidArgument)?;
        if source.end > self.len as usize || end > self.len as usize {
            return Err(Error::InvalidArgument);
        }
        if count == 0 {
            return Ok(());
        }
        // SAFETY: Both ranges are within the same exclusively owned initialized
        // allocation. portMemMove explicitly supports overlap in either direction.
        let (dst, result) = unsafe {
            let dst = self.ptr.as_ptr().add(destination).cast::<c_void>();
            (
                dst,
                portMemMove(
                    dst,
                    count as u64,
                    self.ptr.as_ptr().add(source.start).cast(),
                    count as u64,
                ),
            )
        };
        if result == dst {
            Ok(())
        } else {
            Err(Error::ContractViolation)
        }
    }

    fn check_pointer(&self, result: *mut c_void) -> Result<(), Error> {
        if result == self.ptr.as_ptr().cast() {
            Ok(())
        } else {
            Err(Error::ContractViolation)
        }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: This is the exact live pointer from NVport, uniquely owned by
        // this buffer. No references survive drop and the pointer is freed once.
        unsafe { _portMemFreeUntracked(self.ptr.as_ptr().cast()) };
    }
}

/// Runs actual NVIDIA C memory routines against real OS allocation services.
/// This is a CPU-only kernel integration check, never GPU benchmark evidence.
pub fn self_test() -> Result<(), Error> {
    let mut buffer = Buffer::new(257)?;
    if buffer.as_slice().iter().any(|&byte| byte != 0) {
        return Err(Error::ContractViolation);
    }
    buffer.fill_pattern(&[0x12, 0x34, 0x56])?;
    for (index, &byte) in buffer.as_slice().iter().enumerate() {
        if byte != [0x12, 0x34, 0x56][index % 3] {
            return Err(Error::ContractViolation);
        }
    }
    let source = [0x5a; 257];
    buffer.copy_from_slice(&source)?;
    if !buffer.equals(&source) {
        return Err(Error::ContractViolation);
    }
    buffer.copy_within(0..256, 1)?;
    buffer.copy_within(1..257, 0)?;
    if !buffer.equals(&source) {
        return Err(Error::ContractViolation);
    }
    // SAFETY: The C accessor reads our immutable, platform-specific page size.
    let page_size = unsafe { portMemExGetPageSize() };
    #[cfg(target_os = "none")]
    if page_size != ostd::mm::PAGE_SIZE as u64 {
        return Err(Error::ContractViolation);
    }
    #[cfg(not(target_os = "none"))]
    if page_size != 4096 {
        return Err(Error::ContractViolation);
    }
    Ok(())
}

#[cfg(any(test, ktest))]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn upstream_memory_self_test() {
        self_test().unwrap();
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn copy_fill_compare_many_lengths() {
        for len in [1, 2, 3, 15, 16, 17, 255, 256, 257, 4095, 4096, 65537] {
            let source: Vec<u8> = (0..len).map(|index| (index * 37) as u8).collect();
            let mut buffer = Buffer::new(len).unwrap();
            buffer.copy_from_slice(&source).unwrap();
            assert!(buffer.equals(&source));
            assert_eq!(buffer.as_slice(), source);
            buffer.fill(0xa5).unwrap();
            assert!(buffer.as_slice().iter().all(|&byte| byte == 0xa5));
            assert!(!buffer.equals(&[]));
        }
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn overlapping_move_matches_rust_both_directions() {
        for len in 1..80 {
            for offset in 0..len {
                let source: Vec<u8> = (0..len).map(|index| index as u8).collect();
                let mut expected = source.clone();
                let mut buffer = Buffer::new(len).unwrap();
                buffer.copy_from_slice(&source).unwrap();
                expected.copy_within(0..len - offset, offset);
                buffer.copy_within(0..len - offset, offset).unwrap();
                assert_eq!(buffer.as_slice(), expected);
                buffer.copy_from_slice(&source).unwrap();
                expected.copy_from_slice(&source);
                expected.copy_within(offset..len, 0);
                buffer.copy_within(offset..len, 0).unwrap();
                assert_eq!(buffer.as_slice(), expected);
            }
        }
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn patterned_fill_including_partial_repetition() {
        for len in 1..65 {
            let mut buffer = Buffer::new(len).unwrap();
            for pattern in [&[0x88][..], &[1, 2, 3][..], &[4, 5, 6, 7, 8][..]] {
                buffer.fill_pattern(pattern).unwrap();
                for (index, byte) in buffer.as_slice().iter().enumerate() {
                    assert_eq!(*byte, pattern[index % pattern.len()]);
                }
            }
        }
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn invalid_operations_leave_buffer_unchanged() {
        assert!(matches!(Buffer::new(0), Err(Error::InvalidArgument)));
        assert!(matches!(
            Buffer::new(u32::MAX as usize + 1),
            Err(Error::InvalidArgument)
        ));
        let mut buffer = Buffer::new(8).unwrap();
        assert_eq!(buffer.fill_pattern(&[]), Err(Error::InvalidArgument));
        assert_eq!(buffer.copy_from_slice(&[1]), Err(Error::InvalidArgument));
        assert_eq!(buffer.copy_within(0..9, 0), Err(Error::InvalidArgument));
        assert_eq!(buffer.copy_within(0..8, 1), Err(Error::InvalidArgument));
        assert_eq!(
            buffer.copy_within(0..1, usize::MAX),
            Err(Error::InvalidArgument)
        );
        assert_eq!(buffer.as_slice(), &[0; 8]);
    }
}
