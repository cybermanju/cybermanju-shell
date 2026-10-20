// SPDX-License-Identifier: MPL-2.0

//! Device I/O access and corresponding allocator.
//!
//! This module allows device drivers to access the device I/O they need
//! through _allocators_. There are two types of device I/O:
//!  - `IoMem` for memory I/O (MMIO).
//!  - `IoPort` for port I/O (PIO).

pub(crate) mod io_mem;

pub use self::io_mem::IoMem;
#[cfg_attr(target_arch = "loongarch64", expect(unused_imports))]
pub(crate) use self::io_mem::{IoMemAllocatorBuilder, Sensitive};

cfg_select! {
    target_arch = "x86_64" => {
        mod io_port;

        pub use self::io_port::IoPort;
        pub use self::io_port::IoPortRange;
        pub(crate) use self::io_port::{reserve_io_port_range, sensitive_io_port, RawIoPortRange};
    }
    _ => {},
}

// PCI configuration code is built on architectures without x86 PIO too. Keep
// its BAR access type available there, but fail closed: only x86_64 has an I/O
// port allocator and this fallback never attempts port access.
#[cfg(not(target_arch = "x86_64"))]
#[derive(Clone, Debug)]
/// Unsupported-architecture placeholder that never performs port I/O.
pub struct IoPortRange;

#[cfg(not(target_arch = "x86_64"))]
impl IoPortRange {
    /// Fails closed because this architecture has no port allocator.
    pub fn acquire(_base: u32, _size: u32) -> crate::Result<Self> {
        Err(crate::Error::IoError)
    }

    /// Fails closed because this architecture has no port I/O instructions.
    pub fn read_pod<T: crate::mm::PodOnce>(&self, _offset: usize) -> crate::Result<T> {
        Err(crate::Error::IoError)
    }

    /// Fails closed because this architecture has no port I/O instructions.
    pub fn write_pod<T: crate::mm::PodOnce>(&self, _offset: usize, _value: T) -> crate::Result<()> {
        Err(crate::Error::IoError)
    }
}

/// Initializes the static allocator based on builder.
///
/// # Safety
///
/// User must ensure that:
///
/// 1. All the memory that belong to the system device have been removed
///    by calling the `remove` function.
///
/// 2. All the port I/O regions belonging to the system device are defined
///    using the macros `sensitive_io_port` and `reserve_io_port_range`.
///
/// 3. `MAX_IO_PORT` defined in `crate::arch::io` is guaranteed not to
///    exceed the maximum value specified by architecture.
pub(crate) unsafe fn init(io_mem_builder: IoMemAllocatorBuilder) {
    // SAFETY: The safety is upheld by the caller.
    unsafe { io_mem::init(io_mem_builder) };

    // SAFETY: The safety is upheld by the caller.
    #[cfg(target_arch = "x86_64")]
    unsafe {
        io_port::init()
    };
}
