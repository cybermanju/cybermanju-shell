// SPDX-License-Identifier: MPL-2.0

//! Private NVIDIA OS-service ABI. Only the reviewed NVport memory module calls it.

use alloc::alloc::{alloc, dealloc};
use core::{alloc::Layout, ffi::c_void, ptr};

pub(super) const NV_OK: u32 = 0;
pub(super) const NV_ERR_INVALID_ADDRESS: u32 = 0x1e;
pub(super) const NV_ERR_INVALID_ARGUMENT: u32 = 0x1f;
const NV_ERR_NO_MEMORY: u32 = 0x51;

// The NVIDIA ABI expects ordinary kernel heap memory, not DMA addresses. Keep
// the payload 16-byte aligned and retain its exact Layout for os_free_mem.
const ALIGN: usize = 16;
const HEADER: usize = 16;

fn allocation_layout(size: u64) -> Option<Layout> {
    let size = usize::try_from(size).ok()?;
    if size == 0 {
        return None;
    }
    Layout::from_size_align(size.checked_add(HEADER)?, ALIGN).ok()
}

/// # Safety
/// `address` must be null or an aligned, writable pointer slot, exclusively
/// accessible for this call. Call only after heap initialization in task context.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_alloc_mem(address: *mut *mut c_void, size: u64) -> u32 {
    if address.is_null() {
        return NV_ERR_INVALID_ARGUMENT;
    }
    // SAFETY: The pointer slot is writable by the caller's contract.
    unsafe { address.write(ptr::null_mut()) };
    let Some(layout) = allocation_layout(size) else {
        return NV_ERR_INVALID_ARGUMENT;
    };
    #[cfg(target_os = "none")]
    if ostd::irq::InterruptLevel::current().is_interrupt_context() {
        return NV_ERR_NO_MEMORY;
    }
    // SAFETY: The nonzero checked Layout describes the entire allocation.
    let base = unsafe { alloc(layout) };
    if base.is_null() {
        return NV_ERR_NO_MEMORY;
    }
    // SAFETY: Both the aligned header and output slot are writable. The payload
    // lies in the same allocation and remains live until exactly one free.
    unsafe {
        base.cast::<usize>().write(layout.size());
        address.write(base.add(HEADER).cast());
    }
    NV_OK
}

/// # Safety
/// `address` must be null or the exact live payload returned by os_alloc_mem,
/// with no outstanding accesses or references to that allocation.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_free_mem(address: *mut c_void) {
    if address.is_null() {
        return;
    }
    // SAFETY: The caller returns ownership of the exact allocation; its header
    // is private and still contains the size used by os_alloc_mem.
    unsafe {
        let base = address.cast::<u8>().sub(HEADER);
        let size = base.cast::<usize>().read();
        let layout = Layout::from_size_align_unchecked(size, ALIGN);
        dealloc(base, layout);
    }
}

/// # Safety
/// Source and destination must be valid for `size` bytes, nonoverlapping, with
/// exclusive destination access and no source mutation throughout the call.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_mem_copy(dst: *mut c_void, src: *const c_void, size: u32) -> *mut c_void {
    if size != 0 {
        // SAFETY: The caller provides valid nonoverlapping ranges.
        unsafe { ptr::copy_nonoverlapping(src.cast::<u8>(), dst.cast::<u8>(), size as usize) };
    }
    dst
}

/// # Safety
/// `dst` must be valid and exclusively writable for `size` bytes.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_mem_set(dst: *mut c_void, value: u8, size: u32) -> *mut c_void {
    if size != 0 {
        // SAFETY: The caller grants exclusive access to the destination range.
        unsafe { ptr::write_bytes(dst.cast::<u8>(), value, size as usize) };
    }
    dst
}

/// # Safety
/// Both pointers must be readable for `size` bytes without concurrent mutation.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_mem_cmp(a: *const u8, b: *const u8, size: u32) -> i32 {
    for offset in 0..size as usize {
        // SAFETY: The caller guarantees readable ranges of at least size bytes.
        let (a, b) = unsafe { (a.add(offset).read(), b.add(offset).read()) };
        if a != b {
            return i32::from(a) - i32::from(b);
        }
    }
    0
}

#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
static os_page_size: u64 = ostd::mm::PAGE_SIZE as u64;

// Host tests use the real host allocator, but cannot certify Asterinas usercopy.
// Reject it explicitly there instead of substituting an unchecked memcpy.
#[cfg(not(target_os = "none"))]
#[unsafe(no_mangle)]
static os_page_size: u64 = 4096;

/// # Safety
/// `dst` must be a kernel buffer exclusively writable for size bytes. The
/// current task's user address space must stay active for the complete call.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_memcpy_from_user(dst: *mut c_void, src: *const c_void, size: u32) -> u32 {
    if size == 0 {
        return NV_OK;
    }
    if dst.is_null() || src.is_null() {
        return NV_ERR_INVALID_ADDRESS;
    }
    #[cfg(target_os = "none")]
    {
        use ostd::mm::{Fallible, VmReader, VmWriter, io::FallibleVmRead};
        if !ostd::mm::is_in_user_space(src.addr(), size as usize)
            || ostd::irq::InterruptLevel::current().is_interrupt_context()
        {
            return NV_ERR_INVALID_ADDRESS;
        }
        // SAFETY: The user range was validated. The ABI caller provides a live,
        // exclusively writable kernel destination, not a userspace pointer.
        let (mut reader, mut writer) = unsafe {
            (
                VmReader::<Fallible>::from_user_space(src.cast(), size as usize),
                VmWriter::from_kernel_space(dst.cast(), size as usize),
            )
        };
        return match reader.read_fallible(&mut writer) {
            Ok(copied) if copied == size as usize => NV_OK,
            _ => NV_ERR_INVALID_ADDRESS,
        };
    }
    #[cfg(not(target_os = "none"))]
    {
        0x56
    } // NV_ERR_NOT_SUPPORTED: no Asterinas VM on the host.
}

/// # Safety
/// `src` must be a kernel buffer readable for size bytes without concurrent
/// mutation. The task's user address space must stay active for this call.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_memcpy_to_user(dst: *mut c_void, src: *const c_void, size: u32) -> u32 {
    if size == 0 {
        return NV_OK;
    }
    if dst.is_null() || src.is_null() {
        return NV_ERR_INVALID_ADDRESS;
    }
    #[cfg(target_os = "none")]
    {
        use ostd::mm::{Fallible, VmReader, VmWriter, io::FallibleVmWrite};
        if !ostd::mm::is_in_user_space(dst.addr(), size as usize)
            || ostd::irq::InterruptLevel::current().is_interrupt_context()
        {
            return NV_ERR_INVALID_ADDRESS;
        }
        // SAFETY: The destination is a checked user range; the ABI caller
        // guarantees the kernel source is valid and not concurrently mutated.
        let (mut reader, mut writer) = unsafe {
            (
                VmReader::from_kernel_space(src.cast(), size as usize),
                VmWriter::<Fallible>::from_user_space(dst.cast(), size as usize),
            )
        };
        return match writer.write_fallible(&mut reader) {
            Ok(copied) if copied == size as usize => NV_OK,
            _ => NV_ERR_INVALID_ADDRESS,
        };
    }
    #[cfg(not(target_os = "none"))]
    {
        0x56
    } // NV_ERR_NOT_SUPPORTED: no Asterinas VM on the host.
}

#[cfg(any(test, ktest))]
mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn reject_invalid_sizes_and_clear_output() {
        assert!(allocation_layout(0).is_none());
        assert!(allocation_layout(u64::MAX).is_none());
        assert!(allocation_layout(isize::MAX as u64).is_none());
        let mut output = ptr::dangling_mut::<c_void>();
        // SAFETY: output is a valid pointer slot; no allocation is returned.
        unsafe {
            assert_eq!(os_alloc_mem(&mut output, u64::MAX), NV_ERR_INVALID_ARGUMENT);
            assert!(output.is_null());
            assert_eq!(os_alloc_mem(ptr::null_mut(), 8), NV_ERR_INVALID_ARGUMENT);
            os_free_mem(ptr::null_mut());
        }
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn allocation_alignment_and_free() {
        for len in [1, 15, 16, 17, 4095, 4096, 65537] {
            let mut output = ptr::null_mut();
            // SAFETY: The allocation is exclusively owned and freed once.
            unsafe {
                assert_eq!(os_alloc_mem(&mut output, len), NV_OK);
                assert_eq!(output.addr() % ALIGN, 0);
                os_mem_set(output, 0xa5, len as u32);
                assert_eq!(output.cast::<u8>().add(len as usize - 1).read(), 0xa5);
                os_free_mem(output);
            }
        }
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn usercopy_does_not_report_false_success() {
        let mut byte = 0u8;
        // SAFETY: Kernel ranges are valid; invalid user pointers are rejected.
        unsafe {
            assert_eq!(
                os_memcpy_from_user((&mut byte as *mut u8).cast(), ptr::null(), 1),
                NV_ERR_INVALID_ADDRESS
            );
            assert_eq!(
                os_memcpy_to_user(ptr::null_mut(), (&byte as *const u8).cast(), 1),
                NV_ERR_INVALID_ADDRESS
            );
        }
    }
}
