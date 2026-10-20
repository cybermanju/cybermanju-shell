// SPDX-License-Identifier: MPL-2.0
//! Exclusive, masked MSI-X preparation for pre-driver resource owners.
//!
//! This is not a GPU driver. Activated vectors are deliberately retired, not recycled:
//! OSTD has no cross-CPU APIC/interrupt-remapping drain primitive for delayed messages.
use alloc::vec::Vec;

use ostd::{Error, io::IoMem, irq::IrqLine, mm::VmIoOnce};

use crate::{
    arch::{MSIX_DEFAULT_MSG_ADDR, construct_remappable_msix_address},
    cfg_space::Command,
    mmio::PciMmioGuard,
};

const ENABLE: u16 = 1 << 15;
const FUNCTION_MASK: u16 = 1 << 14;

/// A borrowed MSI-X transaction. All methods and Drop require a current task,
/// enabled preemption and enabled local IRQs (a non-atomic task context).
///
/// Preparation never enables BME. Every allocated entry must have a real handler before
/// activation. No IRQ clone or mutable line escapes. After activation, shutdown requires
/// device-specific stop/completion and worker draining; masking/BME-off alone is not that.
/// Dropping without successful shutdown masks and disconnects handlers, retires vectors,
/// and quarantines the function and its MMIO mappings against reprobe/reuse.
/// Even successful shutdown permanently retires activated IRQ numbers until a real
/// architecture-specific posted-message drain/reclamation protocol is implemented.
/// Neither shutdown nor Drop may run while holding a lock required by a handler:
/// both synchronously wait for callback dispatch readers, and would otherwise deadlock.
#[must_use]
pub struct PciMsixGuard<'a> {
    owner: &'a PciMmioGuard<'a>,
    table: Option<IoMem>,
    pba: Option<IoMem>,
    cap: u16,
    original_control: u16,
    original_command: u16,
    offset: usize,
    pba_offset: usize,
    table_size: u16,
    saved_entries: Vec<[u32; 4]>,
    irqs: Vec<IrqLine>,
    activated: bool,
    closed: bool,
}

fn task_context() -> Result<(), Error> {
    if !ostd::task::atomic_mode::can_sleep() {
        return Err(Error::AccessDenied);
    }
    Ok(())
}

fn checked_range(info: u32, bytes: usize, size: usize) -> Result<(u8, usize), Error> {
    let bir = (info & 7) as u8;
    let offset = (info & !7) as usize;
    if bir >= 6 || offset.checked_add(bytes).ok_or(Error::Overflow)? > size {
        return Err(Error::InvalidArgs);
    }
    Ok((bir, offset))
}

impl<'a> PciMsixGuard<'a> {
    pub(crate) fn prepare(owner: &'a PciMmioGuard<'a>, count: u16) -> Result<Self, Error> {
        task_context()?;
        let (cap, original_control, ti, pi) = owner.msix_descriptor().ok_or(Error::InvalidArgs)?;
        let table_size = (original_control & 0x7ff) + 1;
        if count == 0 || count > table_size || original_control & ENABLE != 0 {
            return Err(Error::InvalidArgs);
        }
        let original_command = owner.location().read16(4);
        if original_command & Command::BUS_MASTER.bits() != 0 {
            return Err(Error::AccessDenied);
        }
        let (_, offset) = checked_range(
            ti,
            usize::from(table_size) * 16,
            owner.bar_size((ti & 7) as u8)?,
        )?;
        let (_, pba_offset) = checked_range(
            pi,
            usize::from(table_size).div_ceil(64) * 8,
            owner.bar_size((pi & 7) as u8)?,
        )?;
        if ti & 7 == pi & 7 {
            let te = offset + usize::from(table_size) * 16;
            let pe = pba_offset + usize::from(table_size).div_ceil(64) * 8;
            if offset < pe && pba_offset < te {
                return Err(Error::InvalidArgs);
            }
        }
        owner.claim_msix()?;
        // Nothing may escape after claiming the lease without rollback.
        let result = (|| {
            let table = owner.map_msix_bar((ti & 7) as u8)?;
            let pba = if ti & 7 == pi & 7 {
                table.clone()
            } else {
                owner.map_msix_bar((pi & 7) as u8)?
            };
            for i in 0..usize::from(table_size).div_ceil(32) {
                let pending: u32 = pba.read_once(pba_offset + i * 4)?;
                if pending != 0 {
                    return Err(Error::AccessDenied);
                }
            }
            let mut irqs = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                irqs.push(IrqLine::alloc()?);
            }
            let mut saved_entries = Vec::with_capacity(usize::from(table_size));
            for i in 0..table_size {
                let at = offset + usize::from(i) * 16;
                saved_entries.push([
                    table.read_once(at)?,
                    table.read_once(at + 4)?,
                    table.read_once(at + 8)?,
                    table.read_once(at + 12)?,
                ]);
            }
            let guard = Self {
                owner,
                table: Some(table),
                pba: Some(pba),
                cap,
                original_control,
                original_command,
                offset,
                pba_offset,
                table_size,
                saved_entries,
                irqs,
                activated: false,
                closed: false,
            };
            guard.write_control(original_control | FUNCTION_MASK);
            if guard.owner.location().read16(cap + 2) & FUNCTION_MASK == 0 {
                return Err(Error::IoError);
            }
            for i in 0..table_size {
                guard.write_entry(i, 12, 1)?;
            }
            for (i, irq) in guard.irqs.iter().enumerate() {
                let (addr, data) = match irq.remapping_index() {
                    Some(index) => (construct_remappable_msix_address(u32::from(index)), 0),
                    None => (MSIX_DEFAULT_MSG_ADDR, u32::from(irq.num())),
                };
                guard.write_entry(i as u16, 0, addr)?;
                guard.write_entry(i as u16, 4, 0)?;
                guard.write_entry(i as u16, 8, data)?;
            }
            guard.flush_table()?;
            // Command writes use zero in the adjacent W1C Status half.
            guard
                .owner
                .write_irq_command(original_command | Command::INTERRUPT_DISABLE.bits());
            if guard.owner.location().read16(4)
                != original_command | Command::INTERRUPT_DISABLE.bits()
            {
                return Err(Error::IoError);
            }
            guard.write_control(original_control | ENABLE | FUNCTION_MASK);
            if guard.owner.location().read16(cap + 2) & (ENABLE | FUNCTION_MASK)
                != ENABLE | FUNCTION_MASK
            {
                return Err(Error::IoError);
            }
            Ok(guard)
        })();
        if result.is_err() {
            owner.release_msix();
        }
        result
    }

    fn write_control(&self, control: u16) {
        self.owner.location().write16(self.cap + 2, control);
    }
    fn write_entry(&self, index: u16, field: usize, value: u32) -> Result<(), Error> {
        self.table
            .as_ref()
            .unwrap()
            .write_once(self.offset + usize::from(index) * 16 + field, &value)
    }
    fn flush_table(&self) -> Result<(), Error> {
        let _: u32 = self.table.as_ref().unwrap().read_once(self.offset + 12)?;
        Ok(())
    }
    /// Hardware table capacity, including entries not assigned by this transaction.
    pub fn table_size(&self) -> u16 {
        self.table_size
    }
    /// Number of IRQ lines owned by this live transaction (excluding retired sinks).
    pub fn vector_count(&self) -> u16 {
        self.irqs.len() as u16
    }
    #[cfg(ktest)]
    pub(crate) fn test_vector_masked(&self, index: u16) -> bool {
        assert!(index < self.table_size);
        let control: u32 = self
            .table
            .as_ref()
            .unwrap()
            .read_once(self.offset + usize::from(index) * 16 + 12)
            .unwrap();
        control & 1 != 0
    }
    #[cfg(ktest)]
    pub(crate) fn test_pending(&self, index: u16) -> bool {
        assert!(index < self.table_size);
        let bits: u32 = self
            .pba
            .as_ref()
            .unwrap()
            .read_once(self.pba_offset + usize::from(index / 32) * 4)
            .unwrap();
        bits & (1 << (index % 32)) != 0
    }
    /// Registers a caller-supplied real device handler while function-masked.
    pub fn register_handler<F>(&mut self, index: u16, callback: F) -> Result<(), Error>
    where
        F: Fn(&ostd::arch::trap::TrapFrame) + Send + Sync + 'static,
    {
        task_context()?;
        if self.activated || self.closed {
            return Err(Error::AccessDenied);
        }
        self.irqs
            .get_mut(usize::from(index))
            .ok_or(Error::InvalidArgs)?
            .on_active(callback);
        Ok(())
    }
    /// Unmasks only configured vectors after every vector has an installed handler.
    /// BME remains the device driver's separate DMA decision.
    pub fn activate(&mut self) -> Result<(), Error> {
        task_context()?;
        if self.closed || self.activated || self.irqs.iter().any(IrqLine::is_empty) {
            return Err(Error::AccessDenied);
        }
        // Mark before any unmask: all subsequent error/Drop paths must retire vectors.
        self.activated = true;
        for i in 0..self.vector_count() {
            self.write_entry(i, 12, 0)?;
        }
        self.flush_table()?;
        self.write_control((self.original_control | ENABLE) & !FUNCTION_MASK);
        if self.owner.location().read16(self.cap + 2) & (ENABLE | FUNCTION_MASK) != ENABLE {
            return Err(Error::IoError);
        }
        Ok(())
    }
    /// Masks the whole function with config and MMIO readback. This is NOT a message drain.
    pub fn mask_all(&mut self) -> Result<(), Error> {
        task_context()?;
        if self.closed {
            return Err(Error::AccessDenied);
        }
        self.write_control(self.owner.location().read16(self.cap + 2) | FUNCTION_MASK);
        if self.owner.location().read16(self.cap + 2) & FUNCTION_MASK == 0 {
            return Err(Error::IoError);
        }
        for i in 0..self.table_size {
            self.write_entry(i, 12, 1)?;
        }
        self.flush_table()
    }
    /// Stops producers using the actual driver's protocol, then synchronously removes handlers.
    ///
    /// `quiesce` must stop/ack hardware, complete DMA, and drain all deferred work that uses
    /// these resources. It must not hold locks needed by a handler. IRQ task context only.
    /// Failure retains every resource and allows retry; no success is inferred from masking.
    /// Activated IRQ numbers remain reserved as empty sinks for delayed posted messages.
    pub fn shutdown(&mut self, quiesce: impl FnOnce() -> Result<(), Error>) -> Result<(), Error> {
        self.mask_all()?;
        quiesce()?;
        if self.owner.location().read16(4) & Command::BUS_MASTER.bits() != 0 {
            return Err(Error::AccessDenied);
        }
        // PBA bits cannot be cleared by software writes. A driver must really drain
        // pending sources/delivery (or reset the function) before restoring its table.
        // This is not evidence that an already emitted message reached every CPU.
        for i in 0..usize::from(self.table_size).div_ceil(32) {
            let pending: u32 = self
                .pba
                .as_ref()
                .unwrap()
                .read_once(self.pba_offset + i * 4)?;
            if pending != 0 {
                return Err(Error::IoError);
            }
        }
        self.disconnect();
        self.restore()?;
        self.closed = true;
        Ok(())
    }
    fn disconnect(&mut self) {
        for irq in self.irqs.drain(..) {
            if self.activated {
                irq.retire();
            } else {
                drop(irq);
            }
        }
    }
    fn restore(&mut self) -> Result<(), Error> {
        // Keep the function masked and MSI-X disabled while restoring table contents.
        self.write_control((self.original_control | FUNCTION_MASK) & !ENABLE);
        for (i, saved) in self.saved_entries.iter().enumerate() {
            for field in [0, 4, 8] {
                self.write_entry(i as u16, field, saved[field / 4])?;
            }
            self.write_entry(i as u16, 12, saved[3])?;
        }
        self.flush_table()?;
        self.write_control(self.original_control);
        if self.owner.location().read16(self.cap + 2) != self.original_control {
            return Err(Error::IoError);
        }
        self.owner.write_irq_command(self.original_command);
        if self.owner.location().read16(4) != self.original_command {
            return Err(Error::IoError);
        }
        for (i, saved) in self.saved_entries.iter().enumerate() {
            for field in [0, 4, 8, 12] {
                let actual: u32 = self
                    .table
                    .as_ref()
                    .unwrap()
                    .read_once(self.offset + i * 16 + field)?;
                if actual != saved[field / 4] {
                    return Err(Error::IoError);
                }
            }
        }
        Ok(())
    }
}

impl Drop for PciMsixGuard<'_> {
    fn drop(&mut self) {
        // A forgotten transaction leaves the owner's lease set, also causing quarantine.
        let context_ok = task_context().is_ok();
        if !context_ok {
            self.owner.quarantine_irq_resources();
            core::mem::forget(core::mem::take(&mut self.irqs));
            core::mem::forget(self.table.take());
            core::mem::forget(self.pba.take());
            return;
        }
        if !self.closed {
            let masked = context_ok && self.mask_all().is_ok();
            if !self.activated && masked {
                self.disconnect();
                self.closed = self.restore().is_ok();
            }
            if !self.closed {
                self.owner.quarantine_irq_resources();
                if context_ok {
                    self.disconnect();
                } else {
                    core::mem::forget(core::mem::take(&mut self.irqs));
                }
                // Do not unmap tables still potentially reachable by hardware/workers.
                core::mem::forget(self.table.take());
                core::mem::forget(self.pba.take());
            }
        }
        if self.closed {
            self.owner.release_msix();
        }
    }
}

#[cfg(ktest)]
mod tests {
    use ostd::prelude::ktest;

    use super::*;
    #[ktest]
    fn msix_table_and_pba_ranges_fail_closed() {
        assert_eq!(checked_range(7, 16, 4096), Err(Error::InvalidArgs));
        assert_eq!(checked_range(4096, 16, 4096), Err(Error::InvalidArgs));
        assert_eq!(checked_range(4088, 16, 4096), Err(Error::InvalidArgs));
        assert_eq!(checked_range(0, usize::MAX, 4096), Err(Error::InvalidArgs));
        assert_eq!(checked_range(8, usize::MAX, 4096), Err(Error::Overflow));
        assert_eq!(checked_range(4080, 16, 4096), Ok((0, 4080)));
        assert_eq!((0x7ffu16 & 0x7ff) + 1, 2048);
    }
}
