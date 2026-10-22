// SPDX-License-Identifier: MPL-2.0

//! Fixed-base planning and data-only staging in the x86 near-text address pool.

use core::ops::Range;

use linux_module_elf::{Export, Layout, SymbolResolver, page_layout_span_bound, plan_page_layout};
use ostd::mm::{
    PAGE_SIZE, Vaddr, module_address::NearTextAddressReservation, module_staging::ModuleStaging,
};

use crate::{Error, PreparationLimits, stage_prepared};

/// Owns a near-kernel-text virtual reservation and the layout planned at its base.
///
/// Reserved virtual coordinates are not aliases of the staged frames: staging
/// continues to use OSTD's existing linear frame aliases. This type neither
/// prepares payload nor authorizes execution. Exports are numeric symbol values,
/// not an ABI certificate. Input bytes are borrowed only during each operation;
/// changing them between construction and preparation causes preparation to
/// revalidate the input structurally, but does not provide cryptographic
/// authentication. Existing allocator bookkeeping OOM is outside `Result`.
pub struct NearTextModuleLayout {
    layout: Layout,
    reservation: NearTextAddressReservation,
}

/// Fixed-policy signed-32 owner using only the closed OSTD export registry.
///
/// The registry currently rejects every Linux import (its sole symbol has a
/// Rust-internal ABI). Consequently only local/no-import modules can prepare;
/// this is not Linux ABI approval. Construction and preparation are structural,
/// not cryptographic authentication. Preparation rechecks the current bytes and
/// consumes this owner on every error. OOM in kernel allocator bookkeeping is
/// outside `Result`. This constructor is fixed-base and x86_64-only; it does not
/// search, fall back, accept caller exports or create payload/frames.
pub struct PolicyNearTextModuleLayout {
    inner: NearTextModuleLayout,
    registry: crate::OstdExportRegistry,
}

/// Private staged result of the fixed OSTD-policy path.
pub struct PolicyNearTextPreparedModule {
    inner: NearTextPreparedModule,
}

impl PolicyNearTextModuleLayout {
    /// Claim a real signed-32 reservation at exactly `base` and check the
    /// closed OSTD import policy and relocation reach. No search or fallback.
    /// Only local/no-import modules currently pass the OSTD policy; success is
    /// not Linux ABI approval or cryptographic authentication. Kernel
    /// bookkeeping OOM remains outside `Result`.
    #[cfg(target_arch = "x86_64")]
    pub fn new_signed32_at(
        bytes: &[u8],
        base: Vaddr,
        max_pages: usize,
        max_sections: usize,
        max_entries: usize,
    ) -> Result<Self, Error> {
        if PAGE_SIZE != 4096 {
            return Err(Error::UnsupportedPageSize);
        }
        let max_span = max_pages
            .checked_mul(PAGE_SIZE)
            .ok_or(Error::Plan(linux_module_elf::Error::ResourceLimit))?;
        let bound =
            page_layout_span_bound(bytes, max_span as u64, max_sections).map_err(Error::Plan)?;
        let span = usize::try_from(bound)
            .map_err(|_| Error::Plan(linux_module_elf::Error::ResourceLimit))?;
        if span == 0 {
            return Err(Error::Plan(linux_module_elf::Error::ResourceLimit));
        }

        let reservation = NearTextAddressReservation::new_signed32_at(base, span, max_pages)
            .map_err(Error::AddressSpace)?;
        let layout = plan_page_layout(bytes, reservation.range().start as u64, bound, max_sections)
            .map_err(Error::Plan)?;
        if layout.span() == 0 {
            return Err(Error::Plan(linux_module_elf::Error::ResourceLimit));
        }
        let registry = crate::OstdExportRegistry::new();
        registry
            .linux_symbol_resolver(bytes, &layout)
            .map_err(Error::Plan)?
            .check_relocation_reach(max_entries)
            .map_err(Error::Plan)?;
        let inner = NearTextModuleLayout {
            layout,
            reservation,
        };
        Ok(Self { inner, registry })
    }

    /// Planned layout metadata; borrowing it does not transfer reservation ownership.
    pub fn layout(&self) -> &Layout {
        self.inner.layout()
    }

    /// Exact reserved virtual interval, in the real signed-32 address range.
    pub fn range(&self) -> Range<Vaddr> {
        self.inner.range()
    }

    /// Revalidate current bytes against the same fixed policy and stage data.
    /// This consumes the owner on success or error. No exports or policy are
    /// caller-selectable; preparation still does not authorize execution.
    pub fn prepare_and_stage(
        self,
        bytes: &[u8],
        limits: PreparationLimits,
    ) -> Result<PolicyNearTextPreparedModule, Error> {
        let prepared = self
            .registry
            .linux_symbol_resolver(bytes, &self.inner.layout)
            .map_err(Error::Plan)?
            .prepare_module(
                limits.max_payload_bytes,
                limits.max_relocation_entries,
                limits.max_sections,
            )
            .map_err(Error::Plan)?;
        let staging = stage_prepared(
            &prepared,
            &self.inner.layout,
            limits.max_pages,
            limits.max_chunks,
        )?;
        Ok(PolicyNearTextPreparedModule {
            inner: NearTextPreparedModule {
                layout: self.inner.layout,
                staging,
                reservation: self.inner.reservation,
            },
        })
    }
}

impl PolicyNearTextPreparedModule {
    /// Staged layout metadata only.
    pub fn layout(&self) -> &Layout {
        self.inner.layout()
    }

    /// Exact reserved virtual interval; staging frames remain accessed by aliases.
    pub fn reserved_range(&self) -> Range<Vaddr> {
        self.inner.reserved_range()
    }

    /// Actual staged byte span, including layout gaps and tails.
    pub fn span(&self) -> usize {
        self.inner.span()
    }

    /// Number of private staging pages.
    pub fn page_count(&self) -> usize {
        self.inner.page_count()
    }

    /// Read staged bytes without exposing a pointer or mapping.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), ostd::Error> {
        self.inner.read_bytes(offset, out)
    }
}

impl NearTextModuleLayout {
    /// Reserve and preflight a fixed base; there is no search or fallback.
    ///
    /// ELF/envelope validation occurs before the claim. After claiming, the
    /// layout is replanned and all relocations are resolved and checked for
    /// arithmetic reach under `max_entries` before an owner is returned; missing
    /// exports and unsupported relocation reach therefore release the claim.
    /// `max_pages` is a checked 4 KiB page budget including alignment gaps and
    /// tails, and `max_sections` bounds layout bookkeeping. This constructor does
    /// not prepare payload or allocate frames. OOM in existing reservation
    /// bookkeeping is not recoverable through this `Result`.
    pub fn new_at(
        bytes: &[u8],
        exports: &[Export<'_>],
        base: Vaddr,
        max_pages: usize,
        max_sections: usize,
        max_entries: usize,
    ) -> Result<Self, Error> {
        if PAGE_SIZE != 4096 {
            return Err(Error::UnsupportedPageSize);
        }
        let max_span = max_pages
            .checked_mul(PAGE_SIZE)
            .ok_or(Error::Plan(linux_module_elf::Error::ResourceLimit))?;
        let bound =
            page_layout_span_bound(bytes, max_span as u64, max_sections).map_err(Error::Plan)?;
        let span = usize::try_from(bound)
            .map_err(|_| Error::Plan(linux_module_elf::Error::ResourceLimit))?;
        if span == 0 {
            return Err(Error::Plan(linux_module_elf::Error::ResourceLimit));
        }

        let reservation = NearTextAddressReservation::new_at(base, span, max_pages)
            .map_err(Error::AddressSpace)?;
        Self::new_with_reservation(
            bytes,
            exports,
            reservation,
            bound,
            max_sections,
            max_entries,
        )
    }

    /// Reserve and preflight a fixed base in the separate signed-32 pool.
    ///
    /// The contract and limits are identical to [`Self::new_at`], with no
    /// search or fallback to the legacy near-text pool.
    #[cfg(target_arch = "x86_64")]
    pub fn new_signed32_at(
        bytes: &[u8],
        exports: &[Export<'_>],
        base: Vaddr,
        max_pages: usize,
        max_sections: usize,
        max_entries: usize,
    ) -> Result<Self, Error> {
        if PAGE_SIZE != 4096 {
            return Err(Error::UnsupportedPageSize);
        }
        let max_span = max_pages
            .checked_mul(PAGE_SIZE)
            .ok_or(Error::Plan(linux_module_elf::Error::ResourceLimit))?;
        let bound =
            page_layout_span_bound(bytes, max_span as u64, max_sections).map_err(Error::Plan)?;
        let span = usize::try_from(bound)
            .map_err(|_| Error::Plan(linux_module_elf::Error::ResourceLimit))?;
        if span == 0 {
            return Err(Error::Plan(linux_module_elf::Error::ResourceLimit));
        }
        let reservation = NearTextAddressReservation::new_signed32_at(base, span, max_pages)
            .map_err(Error::AddressSpace)?;
        Self::new_with_reservation(
            bytes,
            exports,
            reservation,
            bound,
            max_sections,
            max_entries,
        )
    }

    /// Try only a bounded prefix of page-aligned signed-32 pool candidates.
    ///
    /// `window` is a caller-supplied filter, not proof of current suitability.
    /// Each real reservation is replanned and its current relocation constraints
    /// revalidated; the common constructor preflights reach for all relocations.
    /// Only an occupied candidate advances. This prefix search is not exhaustive:
    /// no candidate or exhausted prefix returns `AddressSpace(NoMemory)`, not
    /// proof that the pool is full. No payload, frames, mapping, pointer or
    /// execution capability is created. Existing bookkeeping heap OOM is outside
    /// this `Result`.
    #[cfg(target_arch = "x86_64")]
    pub fn new_signed32_in_relative32_window(
        bytes: &[u8],
        exports: &[Export<'_>],
        window: linux_module_elf::Relative32BaseWindow,
        max_pages: usize,
        max_sections: usize,
        max_entries: usize,
        max_candidates: usize,
    ) -> Result<Self, Error> {
        if max_candidates == 0 || max_candidates > 64 {
            return Err(Error::Plan(linux_module_elf::Error::ResourceLimit));
        }
        if PAGE_SIZE != 4096 {
            return Err(Error::UnsupportedPageSize);
        }
        let max_span = max_pages
            .checked_mul(PAGE_SIZE)
            .ok_or(Error::Plan(linux_module_elf::Error::ResourceLimit))?;
        let bound =
            page_layout_span_bound(bytes, max_span as u64, max_sections).map_err(Error::Plan)?;
        let span = usize::try_from(bound)
            .map_err(|_| Error::Plan(linux_module_elf::Error::ResourceLimit))?;
        if span == 0 {
            return Err(Error::Plan(linux_module_elf::Error::ResourceLimit));
        }

        let pool = NearTextAddressReservation::signed32_arena_range();
        let max_base = pool
            .end
            .checked_sub(span)
            .ok_or(Error::AddressSpace(ostd::Error::NoMemory))?;
        let Some(filtered) = window.restrict(pool.start as u64, max_base as u64) else {
            return Err(Error::AddressSpace(ostd::Error::NoMemory));
        };
        let mut candidate = filtered
            .first_aligned(PAGE_SIZE as u64)
            .ok_or(Error::AddressSpace(ostd::Error::NoMemory))?;
        for attempt in 0..max_candidates {
            let base = Vaddr::try_from(candidate)
                .map_err(|_| Error::AddressSpace(ostd::Error::InvalidArgs))?;
            match NearTextAddressReservation::new_signed32_at(base, span, max_pages) {
                Ok(reservation) => {
                    let owner = Self::new_with_reservation(
                        bytes,
                        exports,
                        reservation,
                        bound,
                        max_sections,
                        max_entries,
                    )?;
                    let resolver =
                        SymbolResolver::new(bytes, owner.layout(), exports).map_err(Error::Plan)?;
                    let fresh = resolver
                        .relative32_placement_constraints(max_entries)
                        .map_err(Error::Plan)?;
                    if !fresh
                        .window()
                        .is_some_and(|current| current.contains(candidate))
                    {
                        return Err(Error::Plan(linux_module_elf::Error::RelocationOverflow));
                    }
                    return Ok(owner);
                }
                Err(ostd::Error::NoMemory) => (),
                Err(error) => return Err(Error::AddressSpace(error)),
            }
            if attempt + 1 == max_candidates {
                break;
            }
            candidate = candidate
                .checked_add(PAGE_SIZE as u64)
                .filter(|next| filtered.contains(*next))
                .ok_or(Error::AddressSpace(ostd::Error::NoMemory))?;
        }
        Err(Error::AddressSpace(ostd::Error::NoMemory))
    }

    fn new_with_reservation(
        bytes: &[u8],
        exports: &[Export<'_>],
        reservation: NearTextAddressReservation,
        bound: u64,
        max_sections: usize,
        max_entries: usize,
    ) -> Result<Self, Error> {
        let layout = plan_page_layout(bytes, reservation.range().start as u64, bound, max_sections)
            .map_err(Error::Plan)?;
        if layout.span() == 0 {
            return Err(Error::Plan(linux_module_elf::Error::ResourceLimit));
        }
        let resolver = SymbolResolver::new(bytes, &layout, exports).map_err(Error::Plan)?;
        resolver
            .check_relocation_reach(max_entries)
            .map_err(Error::Plan)?;
        Ok(Self {
            layout,
            reservation,
        })
    }

    /// Layout planned at the owned near-text base; not a mapping certificate.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Entire exact virtual reservation, not a location for staging frame aliases.
    pub fn range(&self) -> Range<Vaddr> {
        self.reservation.range()
    }

    /// Revalidate, resolve and prepare the supplied bytes, then copy them to private frames.
    ///
    /// Consumes this owner on success or error. Preparation repeats ELF, symbol,
    /// relocation, payload, RELA-entry and section-limit checks against the current
    /// input. Any failure drops the reservation and all acquired frames; no partial
    /// owner escapes. This is structural validation, not cryptographic authentication,
    /// and creates no mapping, permission guarantee, pointer or execution capability.
    pub fn prepare_and_stage(
        self,
        bytes: &[u8],
        exports: &[Export<'_>],
        limits: PreparationLimits,
    ) -> Result<NearTextPreparedModule, Error> {
        let prepared = SymbolResolver::new(bytes, &self.layout, exports)
            .map_err(Error::Plan)?
            .prepare_module(
                limits.max_payload_bytes,
                limits.max_relocation_entries,
                limits.max_sections,
            )
            .map_err(Error::Plan)?;
        let staged = stage_prepared(&prepared, &self.layout, limits.max_pages, limits.max_chunks)?;
        Ok(NearTextPreparedModule {
            layout: self.layout,
            staging: staged,
            reservation: self.reservation,
        })
    }
}

/// Data-only staged frames coupled to their near-text reservation.
///
/// Fields are deliberately ordered so frame handles are dropped before the
/// reserved virtual range. Neither field exposes a mapping or pointer.
pub struct NearTextPreparedModule {
    layout: Layout,
    staging: ModuleStaging,
    reservation: NearTextAddressReservation,
}

impl NearTextPreparedModule {
    /// Metadata only; this does not transfer reservation ownership.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Exact reserved virtual interval; staging frames remain accessed via linear aliases.
    pub fn reserved_range(&self) -> Range<Vaddr> {
        self.reservation.range()
    }

    /// Actual staged byte span, including layout gaps and tails.
    pub fn span(&self) -> usize {
        self.staging.span()
    }

    /// Number of private staging pages, not reserved virtual pages.
    pub fn page_count(&self) -> usize {
        self.staging.page_count()
    }

    /// Read staged data without exposing a pointer or mapping.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), ostd::Error> {
        self.staging.read_bytes(offset, out)
    }
}
