// SPDX-License-Identifier: MPL-2.0
#[cfg(ktest)]
use super::Signed32LeaseAudit;
use super::Signed32RwMapping;
use crate::{Error, mm::Vaddr};
use core::ops::Range;

/// Read-only, non-executable view of this alias. The existing linear alias remains writable.
pub struct Signed32RoMapping {
    inner: Signed32RwMapping,
}
pub(super) fn wrap(inner: Signed32RwMapping) -> Signed32RoMapping {
    Signed32RoMapping { inner }
}
impl Signed32RoMapping {
    /// Byte span readable through the reserved alias, excluding reservation slack.
    pub fn span(&self) -> usize {
        self.inner.span()
    }
    /// Number of retained, mapped data frames.
    pub fn page_count(&self) -> usize {
        self.inner.page_count()
    }
    /// Exclusive VA envelope, including any absent slack pages; not a pointer.
    pub fn reserved_range(&self) -> Range<Vaddr> {
        self.inner.reserved_range()
    }
    /// Read with bounds checks and a local IRQ guard on the fixed sole CPU.
    /// Data may change through other aliases; this does not expose a snapshot.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), Error> {
        self.inner.read_bytes(offset, out)
    }
    #[cfg(ktest)]
    pub fn audit_for_test(&self) -> Result<Signed32LeaseAudit, Error> {
        self.inner.audit_ro_for_test()
    }
}
