// SPDX-License-Identifier: MPL-2.0
//! Checked layout planning for allocatable ELF sections. This records metadata only;
//! it does not allocate section contents, map addresses, or execute module code.
//! Page planning only prevents numeric overlap of occupied 4 KiB pages. Section
//! flags describe intended R/RW/RX use; they do not establish permissions or W^X.

use super::{Error, SECTION_SIZE, inspect, u32_at, u64_at};
use alloc::vec::Vec;

const SHF_ALLOC: u64 = 0x2;
const SHF_WRITE: u64 = 0x1;
const SHF_EXECINSTR: u64 = 0x4;
const SHF_TLS: u64 = 0x400;
const SHT_PROGBITS: u32 = 1;
const SHT_NOTE: u32 = 7;
const SHT_NOBITS: u32 = 8;
const PAGE_SIZE: u64 = 4096;

/// One allocatable section's planned numeric address range.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SectionPlacement {
    pub section_index: u16,
    pub address: u64,
    pub size: u64,
    pub alignment: u64,
    pub flags: u64,
}

/// A metadata-only plan. It does not own or expose module payload bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layout {
    base: u64,
    span: u64,
    placements: Vec<SectionPlacement>,
    page_policy: bool,
}

impl Layout {
    pub fn base(&self) -> u64 {
        self.base
    }
    pub fn span(&self) -> u64 {
        self.span
    }
    pub fn placements(&self) -> &[SectionPlacement] {
        &self.placements
    }
    /// Page size for fixed-page plans; compact plans have no page policy.
    pub fn page_size(&self) -> Option<u64> {
        self.page_policy.then_some(PAGE_SIZE)
    }

    /// Revalidate the ELF against this layout's exact policy and resource bounds.
    pub(crate) fn replan(&self, bytes: &[u8]) -> Result<Layout, Error> {
        if self.page_policy {
            plan_page_layout(bytes, self.base, self.span, self.placements.len())
        } else {
            plan_layout(bytes, self.base, self.span, self.placements.len())
        }
    }
}

/// Plan allocatable sections in section-index order, without loading their data.
pub fn plan_layout(
    bytes: &[u8],
    base: u64,
    max_span: u64,
    max_sections: usize,
) -> Result<Layout, Error> {
    let inspection = inspect(bytes)?;
    let table_offset = usize::try_from(u64_at(bytes, 40)).map_err(|_| Error::SectionTable)?;
    let possible = usize::from(inspection.section_count.saturating_sub(1));
    let capacity = core::cmp::min(possible, max_sections);
    let mut placements = Vec::new();
    placements
        .try_reserve(capacity)
        .map_err(|_| Error::ResourceLimit)?;
    let mut cursor = base;
    for index in 1..inspection.section_count {
        let offset = table_offset
            .checked_add(
                usize::from(index)
                    .checked_mul(SECTION_SIZE)
                    .ok_or(Error::LayoutOverflow)?,
            )
            .ok_or(Error::LayoutOverflow)?;
        let section = bytes
            .get(
                offset
                    ..offset
                        .checked_add(SECTION_SIZE)
                        .ok_or(Error::LayoutOverflow)?,
            )
            .ok_or(Error::SectionTable)?;
        let flags = u64_at(section, 8);
        if flags & SHF_ALLOC == 0 {
            continue;
        }
        let kind = u32_at(section, 4);
        if flags & SHF_TLS != 0 || !matches!(kind, SHT_PROGBITS | SHT_NOTE | SHT_NOBITS) {
            return Err(Error::UnsupportedSection);
        }
        if placements.len() >= max_sections {
            return Err(Error::ResourceLimit);
        }
        let alignment = match u64_at(section, 48) {
            0 => 1,
            value => value,
        };
        let address = cursor
            .checked_add(alignment - 1)
            .ok_or(Error::LayoutOverflow)?
            & !(alignment - 1);
        let size = u64_at(section, 32);
        let end = address.checked_add(size).ok_or(Error::LayoutOverflow)?;
        let span = end.checked_sub(base).ok_or(Error::LayoutOverflow)?;
        if span > max_span {
            return Err(Error::ResourceLimit);
        }
        placements
            .try_reserve(1)
            .map_err(|_| Error::ResourceLimit)?;
        placements.push(SectionPlacement {
            section_index: index,
            address,
            size,
            alignment,
            flags,
        });
        cursor = end;
    }
    let span = cursor.checked_sub(base).ok_or(Error::LayoutOverflow)?;
    if span > max_span {
        return Err(Error::ResourceLimit);
    }
    Ok(Layout {
        base,
        span,
        placements,
        page_policy: false,
    })
}

/// Plan a metadata-only layout using fixed 4 KiB x86-64 pages. This reserves
/// complete pages numerically for nonempty sections; it does not map pages,
/// enforce real permissions/W^X, execute code, or establish ownership.
/// Base must be page-aligned. Each section starts at max(4096, sh_addralign);
/// inter-section padding and the rounded tail count against max_span. Empty
/// sections reserve no pages and may share addresses, but alignment gaps count.
/// WRITE+EXEC is rejected even for empty ALLOC sections. Other sections imply
/// intended R/RW/RX by their flags, not actual page permissions. This is not a
/// validator for canonical kernel addresses, physical frames or the x86 MMU.
pub fn plan_page_layout(
    bytes: &[u8],
    base: u64,
    max_span: u64,
    max_sections: usize,
) -> Result<Layout, Error> {
    if base % PAGE_SIZE != 0 {
        return Err(Error::PageAlignment);
    }
    // Reuse compact planning for structural/type/count validation, without letting
    // compact byte-span accounting incorrectly constrain page-rounded layouts.
    let mut plan = plan_layout(bytes, base, u64::MAX, max_sections)?;
    let mut cursor = base;
    for item in &mut plan.placements {
        if item.flags & (SHF_WRITE | SHF_EXECINSTR) == (SHF_WRITE | SHF_EXECINSTR) {
            return Err(Error::WritableExecutableSection);
        }
        let alignment = core::cmp::max(item.alignment, PAGE_SIZE);
        let address = cursor
            .checked_add(alignment - 1)
            .ok_or(Error::LayoutOverflow)?
            & !(alignment - 1);
        let end = address
            .checked_add(item.size)
            .ok_or(Error::LayoutOverflow)?;
        let reserved_end = if item.size == 0 {
            address
        } else {
            end.checked_add(PAGE_SIZE - 1)
                .ok_or(Error::LayoutOverflow)?
                & !(PAGE_SIZE - 1)
        };
        let span = reserved_end
            .checked_sub(base)
            .ok_or(Error::LayoutOverflow)?;
        if span > max_span {
            return Err(Error::ResourceLimit);
        }
        item.address = address;
        cursor = reserved_end;
    }
    let span = cursor.checked_sub(base).ok_or(Error::LayoutOverflow)?;
    if span > max_span {
        return Err(Error::ResourceLimit);
    }
    plan.span = span;
    plan.page_policy = true;
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{vec, vec::Vec};

    fn put16(b: &mut [u8], o: usize, v: u16) {
        b[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn put32(b: &mut [u8], o: usize, v: u32) {
        b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn put64(b: &mut [u8], o: usize, v: u64) {
        b[o..o + 8].copy_from_slice(&v.to_le_bytes());
    }
    fn fixture(count: u16) -> Vec<u8> {
        let mut b = vec![0; 64 + usize::from(count) * 64];
        b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        put16(&mut b, 16, 1);
        put16(&mut b, 18, 62);
        put32(&mut b, 20, 1);
        put64(&mut b, 40, 64);
        put16(&mut b, 52, 64);
        put16(&mut b, 58, 64);
        put16(&mut b, 60, count);
        b
    }
    fn section(b: &mut [u8], i: usize, ty: u32, flags: u64, size: u64, align: u64) {
        let o = 64 + i * 64;
        put32(b, o + 4, ty);
        put64(b, o + 8, flags);
        put64(b, o + 32, size);
        put64(b, o + 48, align);
    }

    #[test]
    fn aligns_addresses_and_charges_initial_padding_against_span() {
        let mut b = fixture(3);
        section(&mut b, 1, 1, 2, 3, 1);
        section(&mut b, 2, 1, 2, 2, 8);
        let p = plan_layout(&b, 3, 7, 2).unwrap();
        assert_eq!(p.span(), 7);
        assert_eq!(p.placements()[1].address, 8);
        assert_eq!(plan_layout(&b, 3, 6, 2), Err(Error::ResourceLimit));
    }

    #[test]
    fn nobits_needs_no_file_payload_and_zero_sizes_can_share_address() {
        let mut b = fixture(3);
        section(&mut b, 1, 8, 2, 9, 4);
        put64(&mut b, 64 + 64 + 24, u64::MAX);
        put64(&mut b, 64 + 64 + 32, 9);
        section(&mut b, 2, 1, 2, 0, 16);
        let p = plan_layout(&b, 1, 24, 2).unwrap();
        assert_eq!(p.placements()[0].address, 4);
        assert_eq!(p.placements()[1].address, 16);
        assert_eq!(p.span(), 15);
        assert_eq!(plan_layout(&b, 1, 14, 2), Err(Error::ResourceLimit));
        let mut z = fixture(3);
        section(&mut z, 1, 1, 2, 0, 1);
        section(&mut z, 2, 7, 2, 0, 8);
        let p = plan_layout(&z, 0, 0, 2).unwrap();
        assert_eq!(p.span(), 0);
        assert_eq!(p.placements()[0].address, p.placements()[1].address);
    }

    #[test]
    fn ignores_non_alloc_sections_and_empty_plan_has_zero_span() {
        let mut b = fixture(2);
        section(&mut b, 1, 1, 0, 0, 1);
        let p = plan_layout(&b, u64::MAX, 0, 0).unwrap();
        assert_eq!(p.span(), 0);
        assert!(p.placements().is_empty());
    }

    #[test]
    fn limits_allocatable_count_and_span() {
        let mut b = fixture(3);
        section(&mut b, 1, 1, 2, 1, 1);
        section(&mut b, 2, 1, 2, 1, 1);
        assert_eq!(plan_layout(&b, 0, 2, 1), Err(Error::ResourceLimit));
        assert_eq!(plan_layout(&b, 0, 1, 2), Err(Error::ResourceLimit));
    }

    #[test]
    fn rejects_unsupported_allocatable_types_and_tls() {
        for (ty, flags) in [(3, 2), (8, 2 | SHF_TLS), (1, 2 | SHF_TLS)] {
            let mut b = fixture(2);
            section(&mut b, 1, ty, flags, 0, 1);
            assert_eq!(plan_layout(&b, 0, 0, 1), Err(Error::UnsupportedSection));
        }
    }

    #[test]
    fn checked_alignment_address_and_end_overflows() {
        let mut b = fixture(2);
        section(&mut b, 1, 1, 2, 1, 1);
        assert_eq!(
            plan_layout(&b, u64::MAX, u64::MAX, 1),
            Err(Error::LayoutOverflow)
        );
        section(&mut b, 1, 1, 2, 0, 8);
        assert_eq!(
            plan_layout(&b, u64::MAX - 3, u64::MAX, 1),
            Err(Error::LayoutOverflow)
        );
        section(&mut b, 1, 1, 2, 2, 1);
        assert_eq!(
            plan_layout(&b, u64::MAX - 1, u64::MAX, 1),
            Err(Error::LayoutOverflow)
        );
    }

    #[test]
    fn alignment_zero_means_one_and_order_is_index_order() {
        let mut b = fixture(3);
        section(&mut b, 1, 1, 2, 1, 0);
        section(&mut b, 2, 7, 2, 1, 1);
        let p = plan_layout(&b, 5, 2, 2).unwrap();
        assert_eq!(
            p.placements()
                .iter()
                .map(|s| s.section_index)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(p.placements()[0].alignment, 1);
    }

    #[test]
    fn fuzz_like_mutations_and_every_truncation_do_not_panic() {
        let mut b = fixture(2);
        section(&mut b, 1, 1, 2, 0, 1);
        let original = b.clone();
        for n in 0..b.len() {
            let _ = plan_layout(&b[..n], 0, u64::MAX, 8);
        }
        for i in 0..b.len() {
            for x in [0, 1, 0x7f, 0xff] {
                let mut m = b.clone();
                m[i] = x;
                let _ = plan_layout(&m, 0, u64::MAX, 8);
            }
        }
        assert_eq!(b, original);
    }

    #[test]
    fn planning_never_changes_input_and_reports_copyable_metadata() {
        let mut b = fixture(2);
        section(&mut b, 1, 1, 2, 4, 4);
        let before = b.clone();
        let p = plan_layout(&b, 0, 4, 1).unwrap();
        assert_eq!(b, before);
        let q = p.placements()[0];
        assert_eq!(q, p.placements()[0]);
        assert_eq!(q.flags, 2);
        assert_eq!(q.size, 4);
    }
}
