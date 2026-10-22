// SPDX-License-Identifier: MPL-2.0

use core::ops::Range;

use super::ModuleAddressReservation;
use crate::{
    Error,
    mm::{
        CachePolicy, Frame, PAGE_SIZE, PageFlags, PageProperty, Vaddr,
        kspace::{KERNEL_PAGE_TABLE, VMALLOC_VADDR_RANGE, kvirt_area::KVirtArea},
        page_prop::PrivilegedPageFlags,
    },
    prelude::*,
    task::disable_preempt,
};

fn assert_disjoint(first: &Range<Vaddr>, second: &Range<Vaddr>) {
    assert!(first.end <= second.start || second.end <= first.start);
}

fn assert_unmapped(range: &Range<Vaddr>) {
    let guard = disable_preempt();
    let page_table = KERNEL_PAGE_TABLE.get().unwrap();
    for addr in range.clone().step_by(PAGE_SIZE) {
        let page = addr..addr + PAGE_SIZE;
        let mut cursor = page_table.cursor(&guard, &page).unwrap();
        assert!(
            cursor.query().unwrap().1.is_none(),
            "reserved page is mapped"
        );
    }
}

#[ktest]
fn cybermanju_module_address_runtime() {
    for (span, budget) in [
        (0, 0),
        (1, 1),
        (PAGE_SIZE - 1, 1),
        (PAGE_SIZE, 0),
        (usize::MAX, usize::MAX),
    ] {
        assert!(matches!(
            ModuleAddressReservation::new(span, budget),
            Err(Error::InvalidArgs)
        ));
    }
    let too_large = VMALLOC_VADDR_RANGE.len().checked_add(PAGE_SIZE).unwrap();
    assert!(matches!(
        ModuleAddressReservation::new(too_large, too_large / PAGE_SIZE),
        Err(Error::NoMemory)
    ));
    println!("\nCYBERMANJU_MODULE_ADDRESS:validation:PASS cases=6");

    let first = ModuleAddressReservation::new(2 * PAGE_SIZE, 2).unwrap();
    let second = ModuleAddressReservation::new(3 * PAGE_SIZE, 3).unwrap();
    assert_eq!((first.span(), first.page_count()), (2 * PAGE_SIZE, 2));
    assert_eq!((second.span(), second.page_count()), (3 * PAGE_SIZE, 3));
    // Exercise the pre-existing KVirtArea path while both reservations live.
    // Empty frame input maps no payload page; the same allocator must exclude it.
    let existing = KVirtArea::map_frames(
        PAGE_SIZE,
        0,
        core::iter::empty::<Frame<()>>(),
        PageProperty {
            flags: PageFlags::RW,
            cache: CachePolicy::Writeback,
            priv_flags: PrivilegedPageFlags::empty(),
        },
    );
    let first_range = first.range();
    let second_range = second.range();
    let existing_range = existing.range();
    for range in [&first_range, &second_range, &existing_range] {
        assert!(!range.is_empty());
        assert!(range.start.is_multiple_of(PAGE_SIZE));
        assert!(range.end.is_multiple_of(PAGE_SIZE));
        assert!(range.start >= VMALLOC_VADDR_RANGE.start);
        assert!(range.end <= VMALLOC_VADDR_RANGE.end);
    }
    assert_disjoint(&first_range, &second_range);
    assert_disjoint(&first_range, &existing_range);
    assert_disjoint(&second_range, &existing_range);
    // A valid request for the entire pool cannot succeed while ranges are held.
    let pool_size = VMALLOC_VADDR_RANGE.len();
    assert!(matches!(
        ModuleAddressReservation::new(pool_size, pool_size / PAGE_SIZE),
        Err(Error::NoMemory)
    ));
    println!(
        "CYBERMANJU_MODULE_ADDRESS:reserve:PASS first_start={:#x} first_end={:#x} second_start={:#x} second_end={:#x} existing_start={:#x} existing_end={:#x} pool_start={:#x} pool_end={:#x}",
        first_range.start,
        first_range.end,
        second_range.start,
        second_range.end,
        existing_range.start,
        existing_range.end,
        VMALLOC_VADDR_RANGE.start,
        VMALLOC_VADDR_RANGE.end
    );
    for range in [&first_range, &second_range, &existing_range] {
        assert_unmapped(range);
    }
    println!("CYBERMANJU_MODULE_ADDRESS:unmapped:PASS pages=6");

    drop(existing);
    drop(second);
    drop(first);
    let reused = ModuleAddressReservation::new(2 * PAGE_SIZE, 2).unwrap();
    assert_eq!(
        reused.range(),
        first_range,
        "released range was not reusable"
    );
    assert_unmapped(&reused.range());
    println!(
        "CYBERMANJU_MODULE_ADDRESS:release:PASS start={:#x} end={:#x}",
        reused.range().start,
        reused.range().end
    );

    let reused_range = reused.range();
    let requested_base = reused_range.start as usize;
    let invalid_requests = [
        (requested_base, 0, 2),
        (requested_base, PAGE_SIZE + 1, 2),
        (requested_base, 2 * PAGE_SIZE, 0),
        (requested_base + 1, PAGE_SIZE, 1),
        (usize::MAX & !(PAGE_SIZE - 1), 2 * PAGE_SIZE, 2),
        (0, 2 * PAGE_SIZE, 2),
        (
            (crate::cpu::num_cpus as usize) & !(PAGE_SIZE - 1),
            2 * PAGE_SIZE,
            2,
        ),
        (VMALLOC_VADDR_RANGE.end - PAGE_SIZE, 2 * PAGE_SIZE, 2),
    ];
    for (base, span, budget) in invalid_requests {
        assert!(matches!(
            ModuleAddressReservation::new_at(base, span, budget),
            Err(Error::InvalidArgs)
        ));
    }

    assert!(matches!(
        ModuleAddressReservation::new_at(requested_base, 2 * PAGE_SIZE, 2),
        Err(Error::NoMemory)
    ));
    let occupied_by_kvirt = KVirtArea::map_frames(
        PAGE_SIZE,
        0,
        core::iter::empty::<Frame<()>>(),
        PageProperty {
            flags: PageFlags::RW,
            cache: CachePolicy::Writeback,
            priv_flags: PrivilegedPageFlags::empty(),
        },
    );
    let kvirt_range = occupied_by_kvirt.range();
    assert!(matches!(
        ModuleAddressReservation::new_at(kvirt_range.start as usize, PAGE_SIZE, 1),
        Err(Error::NoMemory)
    ));
    drop(occupied_by_kvirt);
    println!(
        "CYBERMANJU_MODULE_ADDRESS:requested_validation:PASS cases={}",
        invalid_requests.len()
    );

    drop(reused);
    let requested = ModuleAddressReservation::new_at(requested_base, 2 * PAGE_SIZE, 2).unwrap();
    let requested_range = requested.range();
    assert_eq!(requested_range, reused_range);
    assert_unmapped(&requested_range);
    let conventional = ModuleAddressReservation::new(2 * PAGE_SIZE, 2).unwrap();
    assert_disjoint(&requested_range, &conventional.range());
    drop(conventional);
    drop(requested);
    let reclaimed = ModuleAddressReservation::new_at(requested_base, 2 * PAGE_SIZE, 2).unwrap();
    assert_eq!(reclaimed.range(), reused_range);
    assert_unmapped(&reclaimed.range());
    println!(
        "CYBERMANJU_MODULE_ADDRESS:requested_base:PASS start={:#x} end={:#x} pages=2 conflicts=2 releases=2 conventional_exclusion=1 unmapped=1",
        requested_range.start, requested_range.end
    );
    near_text_reservation_runtime();
}

fn near_text_reservation_runtime() {
    use super::NearTextAddressReservation;
    use crate::{
        boot::memory_region::MemoryRegionType,
        mm::kspace::{FRAME_METADATA_RANGE, LINEAR_MAPPING_VADDR_RANGE},
    };
    let pool = NearTextAddressReservation::arena_range();
    assert_disjoint(&pool, &VMALLOC_VADDR_RANGE);
    assert_disjoint(&pool, &FRAME_METADATA_RANGE);
    assert_disjoint(&pool, &LINEAR_MAPPING_VADDR_RANGE);
    let code = crate::cpu::num_cpus as *const () as usize;
    let offset = crate::mm::kspace::kernel_loaded_offset();
    let regions = &crate::boot::EARLY_INFO.get().unwrap().memory_regions;
    let mut kernel_start = usize::MAX;
    let mut kernel_end = 0;
    for region in regions
        .iter()
        .filter(|r| r.typ() == MemoryRegionType::Kernel)
    {
        let start = region.base().checked_add(offset).unwrap();
        let end = start.checked_add(region.len()).unwrap();
        assert!(start < end);
        assert_disjoint(&pool, &(start..end));
        kernel_start = kernel_start.min(start);
        kernel_end = kernel_end.max(end);
    }
    assert!(kernel_start <= code && code < kernel_end);
    let base = pool.end - 2 * PAGE_SIZE;
    let first = NearTextAddressReservation::new_at(base, 2 * PAGE_SIZE, 2).unwrap();
    let first_range = first.range();
    assert_eq!(first_range, base..pool.end);
    assert_eq!((first.span(), first.page_count()), (2 * PAGE_SIZE, 2));
    let peer_base = base - 3 * PAGE_SIZE;
    let peer = NearTextAddressReservation::new_at(peer_base, 3 * PAGE_SIZE, 3).unwrap();
    let peer_range = peer.range();
    assert_eq!(peer_range, peer_base..base);
    assert_disjoint(&first_range, &peer_range);
    let old = ModuleAddressReservation::new(PAGE_SIZE, 1).unwrap();
    assert_disjoint(&first_range, &old.range());
    assert_disjoint(&peer_range, &old.range());
    let invalid = [
        (base, 0, 2),
        (base, 1, 2),
        (peer_base + 1, PAGE_SIZE, 1),
        (base, PAGE_SIZE, 0),
        (usize::MAX & !(PAGE_SIZE - 1), 2 * PAGE_SIZE, 2),
        (pool.start - PAGE_SIZE, PAGE_SIZE, 1),
        (pool.end - PAGE_SIZE, 2 * PAGE_SIZE, 2),
        (pool.start, 1025 * PAGE_SIZE, 1025),
        (old.range().start, PAGE_SIZE, 1),
    ];
    for (address, span, budget) in invalid {
        assert!(matches!(
            NearTextAddressReservation::new_at(address, span, budget),
            Err(Error::InvalidArgs)
        ));
    }
    assert!(matches!(
        NearTextAddressReservation::new_at(base + PAGE_SIZE, PAGE_SIZE, 1),
        Err(Error::NoMemory)
    ));
    assert!(matches!(
        NearTextAddressReservation::new_at(peer_base, 3 * PAGE_SIZE, 3),
        Err(Error::NoMemory)
    ));
    assert!(
        matches!(
            ModuleAddressReservation::new_at(base, 2 * PAGE_SIZE, 2),
            Err(Error::InvalidArgs)
        ),
        "old VMALLOC owner must not admit the new arena"
    );
    for range in [&first_range, &peer_range] {
        assert_unmapped(range);
        assert!(
            i32::try_from(code.checked_sub(range.start).unwrap()).is_ok(),
            "real code address must be numerically PC32-near, not a callable export"
        );
    }
    println!(
        "CYBERMANJU_MODULE_ADDRESS:near_text_reserve:PASS start={:#x} end={:#x} peer_start={:#x} peer_end={:#x} pool_start={:#x} pool_end={:#x} kernel_start={:#x} kernel_end={:#x} code={:#x}",
        first_range.start,
        first_range.end,
        peer_range.start,
        peer_range.end,
        pool.start,
        pool.end,
        kernel_start,
        kernel_end,
        code
    );
    drop(peer);
    drop(first);
    let first_again = NearTextAddressReservation::new_at(base, 2 * PAGE_SIZE, 2).unwrap();
    let peer_again = NearTextAddressReservation::new_at(peer_base, 3 * PAGE_SIZE, 3).unwrap();
    assert_eq!(first_again.range(), first_range);
    assert_eq!(peer_again.range(), peer_range);
    assert_unmapped(&first_again.range());
    assert_unmapped(&peer_again.range());
    println!(
        "CYBERMANJU_MODULE_ADDRESS:near_text_checks:PASS invalid_cases=9 conflicts=2 unmapped_pages=5 reclaimed=2 old_pool_rejected=true mapping=false execution=false"
    );
}
