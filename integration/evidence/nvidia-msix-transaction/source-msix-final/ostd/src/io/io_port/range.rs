// SPDX-License-Identifier: MPL-2.0
//! Exclusively owned, bounded ranges of I/O ports.

use alloc::vec::Vec;
use core::mem::size_of;

use super::IoPort;
use crate::{
    Error, Result,
    arch::device::io_port::{PortRead, PortWrite, ReadWriteAccess},
    mm::PodOnce,
};

/// An exclusively allocated, byte-granular I/O-port range.
///
/// Every byte is reserved independently through [`IoPort::acquire`]. If any
/// reservation fails, dropping the partially built vector releases all bytes
/// already reserved. This type deliberately exposes no raw port address.
#[derive(Debug)]
pub struct IoPortRange {
    ports: Vec<IoPort<u8, ReadWriteAccess>>,
}

impl IoPortRange {
    /// Exclusively acquires `size` consecutive ports starting at `base`.
    ///
    /// Port zero, empty ranges, and ranges whose exclusive end cannot be
    /// represented by the allocator's `u16` range arithmetic are rejected.
    /// In particular, this rejects ranges reaching port `0xffff` and any range
    /// ending at `0x1_0000` before issuing allocator reservations.
    pub fn acquire(base: u32, size: u32) -> Result<Self> {
        if base == 0 || size == 0 {
            return Err(Error::InvalidArgs);
        }
        let end = base.checked_add(size).ok_or(Error::Overflow)?;
        if end > u16::MAX as u32 || base > u16::MAX as u32 {
            return Err(Error::InvalidArgs);
        }

        let mut ports = Vec::with_capacity(size as usize);
        for port in base..end {
            // IoPort::acquire reserves exactly this byte. If this fails, `ports`
            // is dropped and its IoPort elements return all prior reservations.
            ports.push(IoPort::<u8, ReadWriteAccess>::acquire(port as u16)?);
        }
        Ok(Self { ports })
    }

    /// Returns the number of exclusively owned ports.
    pub fn size(&self) -> usize {
        self.ports.len()
    }

    /// Reads one POD value using exactly one 8-, 16- or 32-bit PIO operation.
    /// Wider values fail closed; MMIO callers retain their separate 64-bit support.
    pub fn read_pod<T: PodOnce>(&self, offset: usize) -> Result<T> {
        match size_of::<T>() {
            1 => Ok(T::from_bytes(&self.read::<u8>(offset)?.to_ne_bytes())),
            2 => Ok(T::from_bytes(&self.read::<u16>(offset)?.to_ne_bytes())),
            4 => Ok(T::from_bytes(&self.read::<u32>(offset)?.to_ne_bytes())),
            _ => Err(Error::InvalidArgs),
        }
    }

    /// Writes one POD value using exactly one 8-, 16- or 32-bit PIO operation.
    pub fn write_pod<T: PodOnce>(&self, offset: usize, value: T) -> Result<()> {
        let bytes = value.as_bytes();
        match bytes.len() {
            1 => self.write(offset, bytes[0]),
            2 => self.write(offset, u16::from_ne_bytes([bytes[0], bytes[1]])),
            4 => self.write(
                offset,
                u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            ),
            _ => Err(Error::InvalidArgs),
        }
    }

    /// Reads one supported port value after checking that its entire width is
    /// inside this exclusively allocated range.
    pub fn read<T: PortRead>(&self, offset: usize) -> Result<T> {
        let port = self.checked_port::<T>(offset)?;
        // SAFETY: checked_port verifies the complete value width lies within
        // `self.ports`; every byte in that range is exclusively allocated and
        // remains owned for the duration of this borrow.
        Ok(unsafe { T::read_from_port(port) })
    }

    /// Writes one supported port value after checking that its entire width is
    /// inside this exclusively allocated range.
    pub fn write<T: PortWrite>(&self, offset: usize, value: T) -> Result<()> {
        let port = self.checked_port::<T>(offset)?;
        // SAFETY: checked_port verifies the complete value width lies within
        // `self.ports`; every byte in that range is exclusively allocated and
        // remains owned for the duration of this borrow.
        unsafe { T::write_to_port(port, value) };
        Ok(())
    }

    fn checked_port<T>(&self, offset: usize) -> Result<u16> {
        let width = size_of::<T>();
        let end = offset.checked_add(width).ok_or(Error::Overflow)?;
        if !matches!(width, 1 | 2 | 4) || end > self.ports.len() {
            return Err(Error::InvalidArgs);
        }
        let port = self.ports[offset].port();
        // Also ensure the computed last byte is representable. This is
        // redundant with acquire's range validation but keeps the unsafe
        // boundary locally auditable.
        port.checked_add((width - 1) as u16)
            .ok_or(Error::Overflow)?;
        Ok(port)
    }
}

#[cfg(ktest)]
mod tests {
    use super::IoPortRange;
    use crate::{arch::device::io_port::ReadWriteAccess, io::IoPort, prelude::*};

    #[ktest]
    fn conflicts_and_partial_acquisition_roll_back() {
        let held = IoPort::<u8, ReadWriteAccess>::acquire(0xd001).unwrap();
        assert!(IoPortRange::acquire(0xd000, 3).is_err());
        // Failure on the second byte must have released the first byte.
        let first = IoPort::<u8, ReadWriteAccess>::acquire(0xd000).unwrap();
        let last = IoPort::<u8, ReadWriteAccess>::acquire(0xd002).unwrap();
        drop((first, last, held));

        let range = IoPortRange::acquire(0xd010, 2).unwrap();
        assert!(IoPort::<u8, ReadWriteAccess>::acquire(0xd011).is_err());
        drop(range);
        assert!(IoPort::<u8, ReadWriteAccess>::acquire(0xd011).is_ok());
    }

    #[ktest]
    fn invalid_bounds_are_rejected_before_reservation() {
        assert!(IoPortRange::acquire(0xd020, 0).is_err());
        assert!(IoPortRange::acquire(0xfffe, 2).is_err());
        assert!(IoPortRange::acquire(u32::MAX, 2).is_err());
        // Invalid requests perform no I/O and reserve no ports.
        assert!(IoPort::<u8, ReadWriteAccess>::acquire(0xfffe).is_ok());
    }

    #[ktest]
    fn invalid_accesses_fail_before_any_port_instruction() {
        let range = IoPortRange::acquire(0xd030, 4).unwrap();
        assert!(range.read::<u32>(1).is_err());
        assert!(range.write::<u16>(3, 0).is_err());
        assert!(range.read::<u8>(4).is_err());
        assert!(range.read::<u32>(usize::MAX).is_err());
        assert!(range.read_pod::<u64>(0).is_err());
        assert!(range.write_pod(0, 0u64).is_err());
    }
}
