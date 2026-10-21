// SPDX-License-Identifier: MPL-2.0

//! Coupled lifetime in production; temporary single-CPU mapping probes in ktest.

use core::ops::Range;

use super::ModuleStaging;
use crate::{
    Error,
    mm::{Vaddr, module_address::ModuleAddressReservation},
};

/// Holds private staging frames and a virtual reservation in one non-Clone owner.
///
/// Staged bytes use the existing linear alias, NOT `reserved_range()`. There is
/// no production mapping, pointer, permission guarantee, scrubbing, or separate
/// handle API. The ktest-only read probe maps temporarily and removes its alias.
/// The declaration order drops frame handles before releasing the address range.
pub struct ReservedModuleStaging {
    staging: ModuleStaging,
    reservation: ModuleAddressReservation,
}

impl ModuleStaging {
    /// Consume both owners; require nonempty staging fitting in the reservation.
    ///
    /// Joining acquires no additional frames or virtual ranges. On error neither
    /// owner is returned: frame handles drop before the reservation. Existing
    /// reservation bookkeeping retains its non-recoverable heap OOM limitation.
    /// A larger reservation is allowed for a conservative alignment envelope.
    pub fn with_reservation(
        self,
        reservation: ModuleAddressReservation,
    ) -> Result<ReservedModuleStaging, Error> {
        if self.span() == 0 || self.span() > reservation.range().len() {
            drop(self);
            drop(reservation);
            return Err(Error::InvalidArgs);
        }
        Ok(ReservedModuleStaging {
            staging: self,
            reservation,
        })
    }
}

impl ReservedModuleStaging {
    /// Reserved virtual coordinates, not the address of the staging frames.
    pub fn reserved_range(&self) -> Range<Vaddr> {
        self.reservation.range()
    }

    /// Actual staged bytes including their gaps and tails, not envelope slack.
    pub fn span(&self) -> usize {
        self.staging.span()
    }

    /// Privately owned staging pages, which may be fewer than virtual pages.
    pub fn page_count(&self) -> usize {
        self.staging.page_count()
    }

    /// Same bounds/error semantics as ModuleStaging; no read-only MMU promise.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), Error> {
        self.staging.read_bytes(offset, out)
    }

    /// Single-CPU ktest prototype: temporarily map RW/supervisor/non-X, copy via
    /// VA, unmap and locally invalidate TLB before returning. No pointer escapes.
    /// Refuses SMP and reservations above 1024 pages. Mapping/page-table heap
    /// failures can still panic. This is NOT a production or physical NX API.
    #[cfg(ktest)]
    pub fn read_mapped_for_test(&mut self, offset: usize, out: &mut [u8]) -> Result<(), Error> {
        crate::mm::kspace::module_staging_map::read_scoped(
            &self.reservation,
            self.staging.span(),
            self.staging.pages.iter().map(|page| page.0.clone()),
            offset,
            out,
        )
    }

    #[cfg(ktest)]
    pub(crate) fn probe_stale_read_for_test(&mut self, out: &mut [u8]) -> Result<(), Error> {
        crate::mm::kspace::module_staging_map::probe_stale_read(
            &self.reservation,
            self.staging.span(),
            self.staging.pages.iter().map(|page| page.0.clone()),
            out,
        )
    }
}
