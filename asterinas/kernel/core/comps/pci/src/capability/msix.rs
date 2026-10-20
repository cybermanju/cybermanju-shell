// SPDX-License-Identifier: MPL-2.0

//! MSI-X capability support.

use alloc::vec::Vec;

use ostd::{
    Error, Result,
    io::IoMem,
    irq::IrqLine,
    mm::{HasSize, VmIoOnce},
};

use crate::{
    PciDeviceLocation,
    arch::{MSIX_DEFAULT_MSG_ADDR, construct_remappable_msix_address},
    cfg_space::{BarAccess, Command, PciCommonCfgOffset},
    common_device::{BarManager, PciCommonDevice},
};

const MSIX_ENABLE: u16 = 1 << 15;
const MSIX_FUNCTION_MASK: u16 = 1 << 14;
const MSIX_VECTOR_MASK: u32 = 1;

/// Raw information about MSI-X capability.
#[derive(Debug)]
pub(crate) struct RawCapabilityMsix {
    pub(crate) cap_ptr: u16,
    msg_ctrl: u16,
    pub(crate) table_info: u32,
    pub(crate) pba_info: u32,
}

impl RawCapabilityMsix {
    pub(super) fn parse(dev: &PciCommonDevice, cap_ptr: u16) -> Self {
        let msg_ctrl = dev.location().read16(cap_ptr + 2);
        let table_info = dev.location().read32(cap_ptr + 4);
        let pba_info = dev.location().read32(cap_ptr + 8);

        Self {
            cap_ptr,
            msg_ctrl,
            table_info,
            pba_info,
        }
    }
}

/// MSI-X capability.
///
/// It will acquire the access to the BAR space it uses.
#[derive(Debug)]
pub struct CapabilityMsixData {
    loc: PciDeviceLocation,
    ptr: u16,
    table_size: u16,
    /// MSI-X table entry content:
    /// | Vector Control: u32 | Msg Data: u32 | Msg Upper Addr: u32 | Msg Addr: u32 |
    table_bar: IoMem,
    /// Pending bits table.
    pending_table_bar: IoMem,
    table_offset: usize,
    #[expect(dead_code)]
    pending_table_offset: usize,
    irqs: Vec<Option<IrqLine>>,
}

impl CapabilityMsixData {
    pub(super) fn new(
        loc: &PciDeviceLocation,
        bar_manager: &mut BarManager,
        raw_cap: &RawCapabilityMsix,
    ) -> Result<Self> {
        let pba_bar = match bar_manager
            .bar_mut((raw_cap.pba_info & 0b111) as u8)
            .ok_or(Error::InvalidArgs)?
            .acquire()?
        {
            BarAccess::Memory(io_mem) => io_mem,
            BarAccess::Io(_) => return Err(Error::InvalidArgs),
        };
        let pba_offset = (raw_cap.pba_info & !(0b111u32)) as usize;

        let table_bar = match bar_manager
            .bar_mut((raw_cap.table_info & 0b111) as u8)
            .ok_or(Error::InvalidArgs)?
            .acquire()?
        {
            BarAccess::Memory(io_mem) => io_mem,
            BarAccess::Io(_) => return Err(Error::InvalidArgs),
        };
        let table_offset = (raw_cap.table_info & !(0b111u32)) as usize;

        let table_size = (raw_cap.msg_ctrl & 0x7ff) + 1;
        let table_bytes = usize::from(table_size)
            .checked_mul(16)
            .ok_or(Error::Overflow)?;
        let pba_bytes = usize::from(table_size).div_ceil(64) * 8;
        if table_offset
            .checked_add(table_bytes)
            .ok_or(Error::Overflow)?
            > table_bar.size()
            || pba_offset.checked_add(pba_bytes).ok_or(Error::Overflow)? > pba_bar.size()
        {
            return Err(Error::InvalidArgs);
        }
        let table_bir = raw_cap.table_info & 0b111;
        let pba_bir = raw_cap.pba_info & 0b111;
        if table_bir == pba_bir {
            let table_end = table_offset + table_bytes;
            let pba_end = pba_offset + pba_bytes;
            if table_offset < pba_end && pba_offset < table_end {
                return Err(Error::InvalidArgs);
            }
        }

        // Function-mask and verify before the first MSI-X table write. This prevents
        // partially initialized entries from delivering interrupts.
        let control = loc.read16(raw_cap.cap_ptr + 2) | MSIX_FUNCTION_MASK;
        loc.write16(raw_cap.cap_ptr + 2, control);
        if loc.read16(raw_cap.cap_ptr + 2) & MSIX_FUNCTION_MASK == 0 {
            return Err(Error::IoError);
        }

        // First mask and read back every entry; only then program any message fields.
        for i in 0..table_size {
            let vector_control = table_offset + usize::from(i) * 16 + 12;
            table_bar.write_once(vector_control, &MSIX_VECTOR_MASK)?;
            if table_bar.read_once::<u32>(vector_control)? & MSIX_VECTOR_MASK == 0 {
                return Err(Error::IoError);
            }
        }
        for i in 0..table_size {
            let entry = table_offset + usize::from(i) * 16;
            table_bar.write_once(entry, &MSIX_DEFAULT_MSG_ADDR)?;
            table_bar.write_once(entry + 4, &0_u32)?;
            table_bar.write_once(entry + 8, &0_u32)?;
        }

        // Enable MSI-X while retaining Function Mask until an actual handler is installed.
        loc.write16(raw_cap.cap_ptr + 2, control | MSIX_ENABLE);
        if loc.read16(raw_cap.cap_ptr + 2) & (MSIX_ENABLE | MSIX_FUNCTION_MASK)
            != MSIX_ENABLE | MSIX_FUNCTION_MASK
        {
            return Err(Error::IoError);
        }
        // Disable INTx, but never authorize DMA as a side effect of IRQ acquisition.
        // Zero the adjacent W1C Status half rather than using config write16's RMW.
        loc.write32(
            PciCommonCfgOffset::Command as u16,
            u32::from(
                loc.read16(PciCommonCfgOffset::Command as u16) | Command::INTERRUPT_DISABLE.bits(),
            ),
        );

        let mut irqs = Vec::with_capacity(table_size as usize);
        for _ in 0..table_size {
            irqs.push(None);
        }

        Ok(Self {
            loc: *loc,
            ptr: raw_cap.cap_ptr,
            table_size,
            table_bar,
            pending_table_bar: pba_bar,
            table_offset,
            pending_table_offset: pba_offset,
            irqs,
        })
    }

    /// Returns the size of the MSI-X Table.
    pub fn table_size(&self) -> u16 {
        self.table_size
    }

    /// Programs an interrupt message while leaving its vector masked.
    /// An existing active line cannot be replaced.
    pub fn set_interrupt_vector(&mut self, irq: IrqLine, index: u16) -> Result<()> {
        if index >= self.table_size || self.irqs[index as usize].is_some() {
            return Err(Error::InvalidArgs);
        }

        let vector_control = self.table_offset + usize::from(index) * 16 + 12;
        self.table_bar
            .write_once(vector_control, &MSIX_VECTOR_MASK)?;
        if self.table_bar.read_once::<u32>(vector_control)? & MSIX_VECTOR_MASK == 0 {
            return Err(Error::IoError);
        }

        // If interrupt remapping is enabled, then we need to change the value of the message address.
        if let Some(remapping_index) = irq.remapping_index() {
            let address = construct_remappable_msix_address(remapping_index as u32);

            self.table_bar
                .write_once(self.table_offset + usize::from(index) * 16, &address)?;
            self.table_bar
                .write_once(self.table_offset + usize::from(index) * 16 + 8, &0)?;
        } else {
            self.table_bar.write_once(
                self.table_offset + usize::from(index) * 16 + 8,
                &(irq.num() as u32),
            )?;
        }

        self.irqs[index as usize] = Some(irq);
        Ok(())
    }

    /// Unmasks a vector after a real callback has been registered on its IRQ line.
    pub fn enable_vector(&mut self, index: u16) -> Result<()> {
        let irq = self
            .irqs
            .get(index as usize)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidArgs)?;
        if irq.is_empty() {
            return Err(Error::AccessDenied);
        }

        let vector_control = self.table_offset + usize::from(index) * 16 + 12;
        self.table_bar.write_once(vector_control, &0_u32)?;
        if self.table_bar.read_once::<u32>(vector_control)? & MSIX_VECTOR_MASK != 0 {
            return Err(Error::IoError);
        }

        let control = self.loc.read16(self.ptr + 2) & !MSIX_FUNCTION_MASK;
        self.loc.write16(self.ptr + 2, control);
        if self.loc.read16(self.ptr + 2) & MSIX_FUNCTION_MASK != 0 {
            return Err(Error::IoError);
        }
        Ok(())
    }

    /// Returns a mutable reference to the [`IrqLine`].
    ///
    /// Users can register callbacks using the returned [`IrqLine`] reference.
    pub fn irq_mut(&mut self, index: usize) -> Option<&mut IrqLine> {
        self.irqs.get_mut(index)?.as_mut()
    }

    /// Returns true if the MSI-X Enable bit is set.
    pub fn is_enabled(&self) -> bool {
        let msg_ctrl = self.loc.read16(self.ptr + 2);
        msg_ctrl & 0x8000 != 0
    }
}

impl Drop for CapabilityMsixData {
    fn drop(&mut self) {
        // Legacy owners have no stop/quiesce proof: this does not stop/reset the
        // function, prove posted messages drained, or restore an active function's
        // previous table/control state. NVIDIA should use the newer transactional
        // MSI-X API instead. In a context where synchronously disconnecting callbacks
        // is unsafe, intentionally retain IRQs and MMIO.
        if !ostd::irq::is_local_enabled()
            || ostd::irq::InterruptLevel::current().is_interrupt_context()
        {
            core::mem::forget(core::mem::take(&mut self.irqs));
            core::mem::forget(self.table_bar.clone());
            core::mem::forget(self.pending_table_bar.clone());
            return;
        }

        let control = self.loc.read16(self.ptr + 2) | MSIX_FUNCTION_MASK;
        self.loc.write16(self.ptr + 2, control);
        let mut masked = self.loc.read16(self.ptr + 2) & MSIX_FUNCTION_MASK != 0;
        for i in 0..self.table_size {
            let at = self.table_offset + usize::from(i) * 16 + 12;
            let entry_masked = self
                .table_bar
                .write_once(at, &MSIX_VECTOR_MASK)
                .and_then(|()| self.table_bar.read_once::<u32>(at))
                .is_ok_and(|value| value & MSIX_VECTOR_MASK != 0);
            masked &= entry_masked;
        }

        // Masking is not a physical message drain. Retire each initialized IRQ
        // before the MMIO fields are dropped, leaving late deliveries as sinks.
        for irq in core::mem::take(&mut self.irqs).into_iter().flatten() {
            irq.retire();
        }
        if !masked {
            // A failed mask readback is not proof the device stopped using these mappings.
            core::mem::forget(self.table_bar.clone());
            core::mem::forget(self.pending_table_bar.clone());
        }
    }
}
