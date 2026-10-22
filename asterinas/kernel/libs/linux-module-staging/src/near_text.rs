// SPDX-License-Identifier: MPL-2.0

//! Fixed-base planning and data-only staging in the x86 near-text address pool.
//!
//! Production APIs never map staged bytes; a temporary mapping exists only in
//! the cfg(all(ktest, x86_64)) audit helper and is torn down before it returns.

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

/// Fixed-policy signed-32 owner using a closed, internally selected registry.
///
/// The default constructor denies every Linux import; the explicit C-shim
/// constructor admits only the fixed `memcmp` shim. Neither policy certifies a
/// complete Linux ABI. Construction and preparation are structural, not
/// cryptographic authentication. Preparation rechecks the current bytes and
/// consumes this owner on every error. Success stages data only; it does not
/// authorize execution, guarantee WX/SMP properties, or create a mapping,
/// pointer, or execution capability. OOM in kernel allocator bookkeeping is
/// outside `Result`. These constructors are fixed-base and x86_64-only; they do
/// not search, fall back, accept caller exports or create payload/frames.
pub struct PolicyNearTextModuleLayout {
    inner: NearTextModuleLayout,
    registry: FixedImportRegistry,
}

enum FixedImportRegistry {
    RustDenied(crate::OstdExportRegistry),
    #[cfg(target_arch = "x86_64")]
    ExplicitC(crate::CShimExportRegistry),
}

impl FixedImportRegistry {
    fn linux_symbol_resolver<'a>(
        &'a self,
        bytes: &'a [u8],
        layout: &'a Layout,
    ) -> Result<SymbolResolver<'a>, linux_module_elf::Error> {
        match self {
            Self::RustDenied(registry) => registry.linux_symbol_resolver(bytes, layout),
            #[cfg(target_arch = "x86_64")]
            Self::ExplicitC(registry) => registry.linux_symbol_resolver(bytes, layout),
        }
    }
}

/// Private staged result of a retained fixed import policy.
pub struct PolicyNearTextPreparedModule {
    inner: NearTextPreparedModule,
}

impl PolicyNearTextModuleLayout {
    /// Claim a real signed-32 reservation at exactly `base` under the default
    /// deny-all Linux import policy. No search or fallback. Success is data-only,
    /// not complete Linux ABI approval or execution/WX/SMP authorization.
    /// Kernel bookkeeping OOM remains outside `Result`.
    #[cfg(target_arch = "x86_64")]
    pub fn new_signed32_at(
        bytes: &[u8],
        base: Vaddr,
        max_pages: usize,
        max_sections: usize,
        max_entries: usize,
    ) -> Result<Self, Error> {
        Self::new_signed32_at_with_registry(
            bytes,
            base,
            max_pages,
            max_sections,
            max_entries,
            FixedImportRegistry::RustDenied(crate::OstdExportRegistry::new()),
        )
    }

    /// Claim a real signed-32 reservation with the opt-in, closed C-shim policy
    /// (currently only `memcmp`). No caller exports or providers are accepted.
    /// Success is data-only, not complete Linux ABI approval or execution/WX/SMP
    /// authorization. No search or fallback; kernel bookkeeping OOM is outside
    /// `Result`.
    #[cfg(target_arch = "x86_64")]
    pub fn new_signed32_c_shims_at(
        bytes: &[u8],
        base: Vaddr,
        max_pages: usize,
        max_sections: usize,
        max_entries: usize,
    ) -> Result<Self, Error> {
        Self::new_signed32_at_with_registry(
            bytes,
            base,
            max_pages,
            max_sections,
            max_entries,
            FixedImportRegistry::ExplicitC(crate::CShimExportRegistry::new()),
        )
    }

    #[cfg(target_arch = "x86_64")]
    fn new_signed32_at_with_registry(
        bytes: &[u8],
        base: Vaddr,
        max_pages: usize,
        max_sections: usize,
        max_entries: usize,
        registry: FixedImportRegistry,
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
    /// With the known fixed, paginated layout, page/chunk budget rejection has
    /// priority before the resolver examines current bytes or prepares payload.
    /// This is not authentication or proof against physical allocation failure
    /// or OOM; the pipeline retains its other limits and structural checks.
    pub fn prepare_and_stage(
        self,
        bytes: &[u8],
        limits: PreparationLimits,
    ) -> Result<PolicyNearTextPreparedModule, Error> {
        self.prepare_and_stage_with(bytes, limits, stage_prepared)
    }

    fn prepare_and_stage_with<F>(
        self,
        bytes: &[u8],
        limits: PreparationLimits,
        stage: F,
    ) -> Result<PolicyNearTextPreparedModule, Error>
    where
        F: FnOnce(
            &linux_module_elf::PreparedModule,
            &Layout,
            usize,
            usize,
        ) -> Result<ModuleStaging, Error>,
    {
        let span = usize::try_from(self.inner.layout.span())
            .map_err(|_| Error::Plan(linux_module_elf::Error::LayoutOverflow))?;
        if span / PAGE_SIZE > limits.max_pages
            || self.inner.layout.placements().len() > limits.max_chunks
        {
            return Err(Error::Plan(linux_module_elf::Error::ResourceLimit));
        }
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
        let staging = stage(
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

    /// Test-only injection point for exercising staging allocation/copy failures.
    /// This does not simulate real allocator OOM or expose a production hook.
    #[cfg(ktest)]
    pub(crate) fn prepare_and_stage_with_fault(
        self,
        bytes: &[u8],
        limits: PreparationLimits,
        mode: ostd::mm::module_staging::fault_injection::FaultMode,
        stats: &alloc::sync::Arc<ostd::mm::module_staging::fault_injection::FaultStats>,
    ) -> Result<PolicyNearTextPreparedModule, Error> {
        self.prepare_and_stage_with(bytes, limits, |module, layout, pages, chunks| {
            crate::stage_prepared_with_fault(module, layout, pages, chunks, mode, stats)
        })
    }
}

impl PolicyNearTextModuleLayout {
    /// Observe successful staging frame-handle lifetimes for ktests without injecting faults.
    #[cfg(ktest)]
    pub(crate) fn prepare_and_stage_observed_for_test(
        self,
        bytes: &[u8],
        limits: PreparationLimits,
        stats: &alloc::sync::Arc<ostd::mm::module_staging::fault_injection::FaultStats>,
    ) -> Result<PolicyNearTextPreparedModule, Error> {
        self.prepare_and_stage_with(bytes, limits, |module, layout, pages, chunks| {
            crate::stage_prepared_observed(module, layout, pages, chunks, stats)
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

    /// Intended per-page permissions derived from the fixed ELF layout.
    pub fn planned_permissions(
        &self,
        max_pages: usize,
    ) -> Result<linux_module_elf::PagePermissionPlan, linux_module_elf::Error> {
        self.inner.planned_permissions(max_pages)
    }

    /// Read staged bytes without exposing a pointer or mapping.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), ostd::Error> {
        self.inner.read_bytes(offset, out)
    }

    /// Temporarily map staged bytes into the reserved VA for mapper tests only.
    #[cfg(all(ktest, target_arch = "x86_64"))]
    pub(crate) fn read_signed32_mapped_for_test(
        &mut self,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), ostd::Error> {
        self.inner.read_signed32_mapped_for_test(offset, out)
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

    /// Derive metadata-only intended permissions for the checked layout.
    /// This does not map frames or apply page-table permissions.
    pub fn planned_permissions(
        &self,
        max_pages: usize,
    ) -> Result<linux_module_elf::PagePermissionPlan, linux_module_elf::Error> {
        linux_module_elf::plan_page_permissions(&self.layout, max_pages)
    }

    /// Read staged data without exposing a pointer or mapping.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), ostd::Error> {
        self.staging.read_bytes(offset, out)
    }

    /// Temporarily map staged bytes into the reservation for ktest auditing only.
    #[cfg(all(ktest, target_arch = "x86_64"))]
    pub(crate) fn read_signed32_mapped_for_test(
        &mut self,
        offset: usize,
        out: &mut [u8],
    ) -> Result<(), ostd::Error> {
        self.staging
            .read_signed32_mapped_for_test(&mut self.reservation, offset, out)
    }

    /// Consume staged frames and the reservation into OSTD's experimental,
    /// persistent supervisor RW/NX signed-32 mapping. This is an ELF/data
    /// mapping only: allocated pages are not made RX and ELF initializers are
    /// not run. Physical W^X is not provided (the linear RW alias remains),
    /// and OSTD refuses unsupported SMP/topology. This is not production-ready
    /// module loading or execution authorization. On failure OSTD owns cleanup;
    /// this owner is consumed and its remaining layout metadata is dropped.
    #[cfg(target_arch = "x86_64")]
    pub fn into_signed32_rw_nx(self) -> Result<NearTextMappedModule, ostd::Error> {
        let budget = self.page_count();
        self.into_signed32_rw_nx_with_page_budget(budget)
    }

    /// Validate intended permission metadata against `max_pages`, then map all staged
    /// pages RW/NX. A smaller budget does not avoid frames already acquired during staging;
    /// this owner is consumed on failure, with frames dropped before the VA reservation.
    #[cfg(target_arch = "x86_64")]
    pub fn into_signed32_rw_nx_with_page_budget(
        self,
        max_pages: usize,
    ) -> Result<NearTextMappedModule, ostd::Error> {
        self.into_signed32_rw_nx_with(max_pages, |staging, reservation| {
            staging.map_signed32_rw_nx(reservation)
        })
    }

    fn into_signed32_rw_nx_with<F>(
        self,
        max_pages: usize,
        map: F,
    ) -> Result<NearTextMappedModule, ostd::Error>
    where
        F: FnOnce(
            ModuleStaging,
            NearTextAddressReservation,
        ) -> Result<ostd::mm::module_staging::Signed32RwMapping, ostd::Error>,
    {
        let permissions = self
            .planned_permissions(max_pages)
            .map_err(permission_plan_ostd_error)?;
        let Self {
            layout,
            staging,
            reservation,
        } = self;
        let mapping = map(staging, reservation)?;
        Ok(NearTextMappedModule {
            layout,
            mapping,
            permissions,
        })
    }

    /// Test-only injected failure point for OSTD's mapping handoff.
    #[cfg(all(ktest, target_arch = "x86_64"))]
    pub(crate) fn into_signed32_rw_nx_with_fault_for_test(
        self,
        after: usize,
        observed: &mut usize,
    ) -> Result<NearTextMappedModule, ostd::Error> {
        let budget = self.page_count();
        self.into_signed32_rw_nx_with(budget, |staging, reservation| {
            staging.map_signed32_rw_nx_with_fault_for_test(reservation, after, observed)
        })
    }

    /// Test-only probe confirms whether deterministic permission preflight reached the mapper.
    #[cfg(all(ktest, target_arch = "x86_64"))]
    pub(crate) fn into_signed32_rw_nx_with_budget_observed_for_test(
        self,
        max_pages: usize,
        entries: &mut usize,
    ) -> Result<NearTextMappedModule, ostd::Error> {
        self.into_signed32_rw_nx_with(max_pages, |staging, reservation| {
            *entries += 1;
            staging.map_signed32_rw_nx(reservation)
        })
    }
}

/// Owned lease over ELF-staged data at the reserved signed-32 virtual address.
///
/// Every allocated page is mapped supervisor RW/NX; this does not create RX
/// pages or run initializers. The existing linear RW aliases remain, so
/// physical W^X is not established. OSTD refuses unsupported SMP/topology.
/// The API is experimental and not production-ready module loading; it exposes
/// no pointer, writes, execution capability, or constructor.
#[cfg(target_arch = "x86_64")]
pub struct NearTextMappedModule {
    layout: Layout,
    mapping: ostd::mm::module_staging::Signed32RwMapping,
    permissions: linux_module_elf::PagePermissionPlan,
}

#[cfg(target_arch = "x86_64")]
impl NearTextMappedModule {
    /// Layout metadata retained from the data-only ELF staging plan.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Intended ELF page permissions; actual mapped pages remain RW/NX.
    pub fn permission_plan(&self) -> &linux_module_elf::PagePermissionPlan {
        &self.permissions
    }

    /// Exact virtual interval owned by the persistent mapping.
    pub fn reserved_range(&self) -> Range<Vaddr> {
        self.mapping.reserved_range()
    }

    /// Number of bytes mapped, including layout gaps and tails.
    pub fn span(&self) -> usize {
        self.mapping.span()
    }

    /// Number of mapped staging pages.
    pub fn page_count(&self) -> usize {
        self.mapping.page_count()
    }

    /// Read mapped bytes without exposing a pointer or write capability.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), ostd::Error> {
        self.mapping.read_bytes(offset, out)
    }

    /// Audit the lease's permissions and ownership in ktests only.
    #[cfg(ktest)]
    pub(crate) fn audit_for_test(
        &self,
    ) -> Result<ostd::mm::module_staging::Signed32LeaseAudit, ostd::Error> {
        self.mapping.audit_for_test()
    }
}

#[cfg(target_arch = "x86_64")]
fn permission_plan_ostd_error(error: linux_module_elf::Error) -> ostd::Error {
    match error {
        linux_module_elf::Error::ResourceLimit => ostd::Error::NoMemory,
        _ => ostd::Error::InvalidArgs,
    }
}

/// Fixed-policy counterpart of [`NearTextMappedModule`].
#[cfg(target_arch = "x86_64")]
pub struct PolicyNearTextMappedModule {
    inner: NearTextMappedModule,
}

#[cfg(target_arch = "x86_64")]
impl PolicyNearTextMappedModule {
    /// Retained layout metadata.
    pub fn layout(&self) -> &Layout {
        self.inner.layout()
    }

    /// Intended ELF page permissions; actual mapped pages remain RW/NX.
    pub fn permission_plan(&self) -> &linux_module_elf::PagePermissionPlan {
        self.inner.permission_plan()
    }

    /// Exact virtual interval owned by the persistent mapping.
    pub fn reserved_range(&self) -> Range<Vaddr> {
        self.inner.reserved_range()
    }

    /// Number of bytes mapped, including layout gaps and tails.
    pub fn span(&self) -> usize {
        self.inner.span()
    }

    /// Number of mapped staging pages.
    pub fn page_count(&self) -> usize {
        self.inner.page_count()
    }

    /// Read mapped bytes without exposing a pointer or write capability.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), ostd::Error> {
        self.inner.read_bytes(offset, out)
    }

    /// Audit the lease's permissions and ownership in ktests only.
    #[cfg(ktest)]
    pub(crate) fn audit_for_test(
        &self,
    ) -> Result<ostd::mm::module_staging::Signed32LeaseAudit, ostd::Error> {
        self.inner.audit_for_test()
    }
}

impl PolicyNearTextPreparedModule {
    /// Consume staged data into the OSTD signed-32 RW/NX lease without changing
    /// the retained fixed import-policy handoff. It does not certify ABI
    /// completeness, execute initializers, establish physical W^X, or authorize
    /// production module loading/execution; OSTD refuses unsupported topology.
    #[cfg(target_arch = "x86_64")]
    pub fn into_signed32_rw_nx(self) -> Result<PolicyNearTextMappedModule, ostd::Error> {
        let inner = self.inner.into_signed32_rw_nx()?;
        Ok(PolicyNearTextMappedModule { inner })
    }

    /// Consume staged data after validating a page budget; this cannot avoid frames already staged.
    #[cfg(target_arch = "x86_64")]
    pub fn into_signed32_rw_nx_with_page_budget(
        self,
        max_pages: usize,
    ) -> Result<PolicyNearTextMappedModule, ostd::Error> {
        let inner = self.inner.into_signed32_rw_nx_with_page_budget(max_pages)?;
        Ok(PolicyNearTextMappedModule { inner })
    }

    /// Test-only injected failure point for OSTD's mapping handoff.
    #[cfg(all(ktest, target_arch = "x86_64"))]
    pub(crate) fn into_signed32_rw_nx_with_fault_for_test(
        self,
        after: usize,
        observed: &mut usize,
    ) -> Result<PolicyNearTextMappedModule, ostd::Error> {
        let inner = self
            .inner
            .into_signed32_rw_nx_with_fault_for_test(after, observed)?;
        Ok(PolicyNearTextMappedModule { inner })
    }

    /// Test-only observed handoff through the real mapper.
    #[cfg(all(ktest, target_arch = "x86_64"))]
    pub(crate) fn into_signed32_rw_nx_with_budget_observed_for_test(
        self,
        max_pages: usize,
        entries: &mut usize,
    ) -> Result<PolicyNearTextMappedModule, ostd::Error> {
        let inner = self
            .inner
            .into_signed32_rw_nx_with_budget_observed_for_test(max_pages, entries)?;
        Ok(PolicyNearTextMappedModule { inner })
    }
}
