// SPDX-License-Identifier: MPL-2.0
//! Signed-32 relocation preparation coverage; never maps or executes.
use alloc::vec;

use linux_module_elf::{Error as ElfError, Export, SymbolResolver, plan_page_layout};
use ostd::{
    cpu,
    mm::{PAGE_SIZE, module_address::NearTextAddressReservation},
    prelude::*,
};

use super::{
    FIXTURE, MAX_SECTIONS, MAX_SPAN, PreparationLimits, fixture_with_u32_relocation,
    relative_import_fixture,
};
use crate::{Error, NearTextModuleLayout};

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

// Turn the parent's trusted U32 relocation into R_X86_64_32S, then independently
// recover and validate the RELA target and its local section symbol from ELF headers.
pub(super) fn signed_self_fixture() -> (alloc::vec::Vec<u8>, u16, u64, u16) {
    let mut bytes = fixture_with_u32_relocation();
    let changed = bytes.iter().zip(FIXTURE).position(|(a, b)| a != b).unwrap();
    bytes[changed..changed + 4].copy_from_slice(&11u32.to_le_bytes());
    let table = u64_at(&bytes, 40) as usize;
    let count = u16::from_le_bytes(bytes[60..62].try_into().unwrap()) as usize;
    for i in 0..count {
        let sh = table + i * 64;
        if u32_at(&bytes, sh + 4) != 4 {
            continue;
        }
        let off = u64_at(&bytes, sh + 24) as usize;
        let size = u64_at(&bytes, sh + 32) as usize;
        if changed < off || changed >= off + size {
            continue;
        }
        assert_eq!((changed - off) % 24, 8);
        let entry = changed - 8;
        let info = u64_at(&bytes, entry + 8);
        assert_eq!(info as u32, 11);
        let target = u16::try_from(u32_at(&bytes, sh + 44)).unwrap();
        let offset = u64_at(&bytes, entry);
        let symtab = table + u32_at(&bytes, sh + 40) as usize * 64;
        let symidx = (info >> 32) as usize;
        let symbol = u64_at(&bytes, symtab + 24) as usize + symidx * 24;
        let defined = u16::from_le_bytes(bytes[symbol + 6..symbol + 8].try_into().unwrap());
        assert_eq!(bytes[symbol + 4], 3, "expected local STT_SECTION symbol");
        assert_eq!(u64_at(&bytes, symbol + 8), 0);
        assert_eq!(u64_at(&bytes, entry + 16), 0);
        assert_ne!(defined, 0);
        return (bytes, target, offset, defined);
    }
    panic!("changed R_X86_64_32S RELA entry not found in trusted fixture");
}

#[ktest]
fn cybermanju_signed32_prepared_runtime() {
    println!("");
    assert_eq!(cpu::num_cpus(), 1);
    println!(
        "CYBERMANJU_SIGNED32_PREPARED:begin:PASS cpus=1 kinds=3 rejected=6 cross_pool_rejected=2"
    );
    let arena = NearTextAddressReservation::signed32_arena_range();
    let base = arena.end.checked_sub(64 * PAGE_SIZE).unwrap();
    let old_arena = NearTextAddressReservation::arena_range();
    assert!(matches!(
        NearTextAddressReservation::new_at(base, PAGE_SIZE, 1),
        Err(ostd::Error::InvalidArgs)
    ));
    assert!(matches!(
        NearTextAddressReservation::new_signed32_at(old_arena.start, PAGE_SIZE, 1),
        Err(ostd::Error::InvalidArgs)
    ));
    let envelope =
        linux_module_elf::page_layout_span_bound(FIXTURE, MAX_SPAN, MAX_SECTIONS).unwrap() as usize;
    let limits = |max_pages| PreparationLimits {
        max_payload_bytes: MAX_SPAN as usize,
        max_relocation_entries: 65_536,
        max_sections: MAX_SECTIONS,
        max_pages,
        max_chunks: MAX_SECTIONS,
    };
    let export = cpu::num_cpus as *const () as usize as u64;
    let mut rejected = 0usize;
    let mut reclaimed = 0usize;
    let mut staged_count = 0usize;

    for kind in [11u32, 2, 4] {
        let (bytes, exports, target, offset, symbol, self_defined) = if kind == 11 {
            let (bytes, target, offset, defined) = signed_self_fixture();
            let layout = plan_page_layout(&bytes, base as u64, MAX_SPAN, MAX_SECTIONS).unwrap();
            let value = layout
                .placements()
                .iter()
                .find(|p| p.section_index == defined)
                .unwrap()
                .address;
            (bytes, alloc::vec::Vec::new(), target, offset, value, true)
        } else {
            let (bytes, name, target, offset) = relative_import_fixture(kind);
            (
                bytes,
                alloc::vec![Export {
                    name,
                    value: export
                }],
                target,
                offset,
                export,
                false,
            )
        };
        let plan = NearTextModuleLayout::new_signed32_at(
            &bytes,
            &exports,
            base,
            MAX_SPAN as usize / PAGE_SIZE,
            MAX_SECTIONS,
            MAX_SECTIONS,
        )
        .unwrap();
        let range = plan.range();
        assert_eq!(
            range,
            base..base + envelope,
            "signed32 layout must own exact requested range"
        );
        let layout = plan.layout();
        let placement = layout
            .placements()
            .iter()
            .find(|p| p.section_index == target)
            .unwrap();
        let field = placement.address + offset;
        let expected = if kind == 11 {
            let signed = (symbol as u32 as i32) as i64 as u64;
            assert_eq!(
                signed, symbol,
                "signed self relocation must be representable"
            );
            (symbol as u32).to_le_bytes()
        } else {
            let delta = i128::from(symbol) - i128::from(field);
            i32::try_from(delta).unwrap().to_le_bytes()
        };
        let page_layout = plan_page_layout(&bytes, base as u64, MAX_SPAN, MAX_SECTIONS).unwrap();
        let resolver = SymbolResolver::new(&bytes, &page_layout, &exports).unwrap();
        let prepared = resolver
            .prepare_module(MAX_SPAN as usize, 65_536, MAX_SECTIONS)
            .unwrap();
        let span = layout.span() as usize;
        let relative = usize::try_from(field - base as u64).unwrap();
        let mut expected_all = vec![0; span];
        for section in prepared.sections() {
            let start = usize::try_from(section.placement().address - base as u64).unwrap();
            expected_all[start..start + section.bytes().len()].copy_from_slice(section.bytes());
        }
        assert_eq!(&expected_all[relative..relative + 4], &expected);
        let staged = plan
            .prepare_and_stage(&bytes, &exports, limits(envelope / PAGE_SIZE))
            .unwrap();
        assert_eq!(
            staged.reserved_range(),
            range,
            "signed32 staged owner must retain original reservation"
        );
        assert_eq!(staged.layout().base(), base as u64);
        assert_eq!(staged.span(), span);
        assert_eq!(staged.page_count(), span / PAGE_SIZE);
        assert!(matches!(
            NearTextAddressReservation::new_signed32_at(
                range.start,
                range.len(),
                range.len() / PAGE_SIZE
            ),
            Err(ostd::Error::NoMemory)
        ));
        let mut actual = vec![0xff; span];
        staged.read_bytes(0, &mut actual).unwrap();
        assert_eq!(
            actual, expected_all,
            "all prepared bytes including gaps and tails"
        );
        assert_eq!(&actual[relative..relative + 4], &expected);
        assert!(matches!(
            NearTextAddressReservation::new_signed32_at(
                range.start,
                range.len(),
                range.len() / PAGE_SIZE
            ),
            Err(ostd::Error::NoMemory)
        ));
        let payload = prepared.payload_bytes();
        let relas = prepared.relocation_entries();
        drop(staged);
        let reclaimed_owner = NearTextAddressReservation::new_signed32_at(
            range.start,
            range.len(),
            range.len() / PAGE_SIZE,
        )
        .unwrap();
        assert_eq!(reclaimed_owner.range(), range);
        drop(reclaimed_owner);
        reclaimed += 1;
        println!(
            "CYBERMANJU_SIGNED32_PREPARED:staged:PASS kind={} base={:#x} reserved={} span={} pages={} payload={} relas={} symbol={:#x} field={:#x} compared={} self_defined={}",
            kind,
            base,
            range.len(),
            span,
            span / PAGE_SIZE,
            payload,
            relas,
            symbol,
            field,
            span,
            self_defined
        );
        staged_count += 1;
    }

    let bad_u32 = fixture_with_u32_relocation();
    assert!(
        matches!(
            NearTextModuleLayout::new_signed32_at(
                &bad_u32,
                &[],
                base,
                MAX_SPAN as usize / PAGE_SIZE,
                MAX_SECTIONS,
                MAX_SECTIONS
            ),
            Err(Error::Plan(ElfError::RelocationOverflow))
        ),
        "unsigned self must fail preflight"
    );
    let reclaim =
        NearTextAddressReservation::new_signed32_at(base, envelope, envelope / PAGE_SIZE).unwrap();
    drop(reclaim);
    reclaimed += 1;
    rejected += 1;
    let old_base = old_arena.end - 64 * PAGE_SIZE;
    let (old_self, _, _, _) = signed_self_fixture();
    assert!(matches!(
        NearTextModuleLayout::new_at(
            &old_self,
            &[],
            old_base,
            MAX_SPAN as usize / PAGE_SIZE,
            MAX_SECTIONS,
            MAX_SECTIONS
        ),
        Err(Error::Plan(ElfError::RelocationOverflow))
    ));
    let old_reclaim =
        NearTextAddressReservation::new_at(old_base, envelope, envelope / PAGE_SIZE).unwrap();
    drop(old_reclaim);
    reclaimed += 1;
    rejected += 1;
    let (pc32, name, _, _) = relative_import_fixture(2);
    assert!(matches!(
        NearTextModuleLayout::new_signed32_at(
            &pc32,
            &[],
            base,
            MAX_SPAN as usize / PAGE_SIZE,
            MAX_SECTIONS,
            MAX_SECTIONS
        ),
        Err(Error::Plan(ElfError::MissingSymbol))
    ));
    let reclaim =
        NearTextAddressReservation::new_signed32_at(base, envelope, envelope / PAGE_SIZE).unwrap();
    drop(reclaim);
    reclaimed += 1;
    rejected += 1;
    let zero = [Export { name, value: 0 }];
    assert!(matches!(
        NearTextModuleLayout::new_signed32_at(
            &pc32,
            &zero,
            base,
            MAX_SPAN as usize / PAGE_SIZE,
            MAX_SECTIONS,
            MAX_SECTIONS
        ),
        Err(Error::Plan(ElfError::RelocationOverflow))
    ));
    let reclaim =
        NearTextAddressReservation::new_signed32_at(base, envelope, envelope / PAGE_SIZE).unwrap();
    drop(reclaim);
    reclaimed += 1;
    rejected += 1;
    let exports = [Export {
        name,
        value: export,
    }];
    assert!(matches!(
        NearTextModuleLayout::new_signed32_at(
            &pc32,
            &exports,
            base,
            MAX_SPAN as usize / PAGE_SIZE,
            MAX_SECTIONS,
            0
        ),
        Err(Error::Plan(ElfError::ResourceLimit))
    ));
    let reclaim =
        NearTextAddressReservation::new_signed32_at(base, envelope, envelope / PAGE_SIZE).unwrap();
    drop(reclaim);
    reclaimed += 1;
    rejected += 1;
    let plan = NearTextModuleLayout::new_signed32_at(
        &pc32,
        &exports,
        base,
        MAX_SPAN as usize / PAGE_SIZE,
        MAX_SECTIONS,
        MAX_SECTIONS,
    )
    .unwrap();
    let range = plan.range();
    assert!(matches!(
        plan.prepare_and_stage(&pc32, &exports, limits(0)),
        Err(Error::Plan(ElfError::ResourceLimit))
    ));
    let reclaim = NearTextAddressReservation::new_signed32_at(
        range.start,
        range.len(),
        range.len() / PAGE_SIZE,
    )
    .unwrap();
    drop(reclaim);
    reclaimed += 1;
    rejected += 1;
    assert_eq!((staged_count, rejected, reclaimed), (3, 6, 9));
    println!(
        "CYBERMANJU_SIGNED32_PREPARED:complete:PASS staged=3 rejected=6 reclaimed=9 compared_full_span=true old_arena_unchanged=true pages_mapped=false linux_export_lookup=false execution=false"
    );
}
