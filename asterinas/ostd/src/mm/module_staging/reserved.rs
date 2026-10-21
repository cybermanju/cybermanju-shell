// SPDX-License-Identifier: MPL-2.0

//! Coupled lifetime only, not a virtual mapping or permission transition.

use core::ops::Range;

use super::ModuleStaging;
use crate::{
    Error,
    mm::{Vaddr, module_address::ModuleAddressReservation},
};

/// Holds private staging frames and a virtual reservation in one non-Clone owner.
///
/// Staged bytes use the existing linear alias, NOT `reserved_range()`. There is
/// no mapping, pointer, permission guarantee, scrubbing, or separate handle API.
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
}
