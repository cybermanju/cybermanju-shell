// SPDX-License-Identifier: MPL-2.0
//! Fixture-only oracle: numeric page separation is NOT enforced memory protection.
use linux_module_elf::{Error, Export, SymbolResolver, inspect, plan_page_layout};
fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

pub fn check(bytes: &[u8], exports: &[Export<'_>]) {
    let before = bytes.to_vec();
    let inspected = inspect(bytes).unwrap();
    let table = u64_at(bytes, 40) as usize;
    let section = |i: usize| &bytes[table + i * 64..table + (i + 1) * 64];
    let base = 0x200000;
    let layout = plan_page_layout(bytes, base, 16 * 1024 * 1024, 1024)
        .expect("4KiB-separated metadata layout");
    assert_eq!(layout.page_size(), Some(4096));
    let indices: Vec<_> = (1..inspected.section_count)
        .filter(|&i| u64_at(section(usize::from(i)), 8) & 2 != 0)
        .collect();
    assert_eq!(
        layout
            .placements()
            .iter()
            .map(|p| p.section_index)
            .collect::<Vec<_>>(),
        indices
    );
    let mut cursor = u128::from(base);
    let mut occupied = 0;
    for p in layout.placements() {
        let h = section(usize::from(p.section_index));
        let align = u128::from(u64_at(h, 48).max(4096));
        let expected = cursor.div_ceil(align) * align;
        assert_eq!(u128::from(p.address), expected);
        assert_eq!(p.address % 4096, 0);
        assert_eq!(p.size, u64_at(h, 32));
        assert_eq!(p.flags, u64_at(h, 8));
        assert_ne!(p.flags & 5, 5, "no WRITE+EXEC section admitted");
        cursor = if p.size == 0 {
            expected
        } else {
            occupied += 1;
            (expected + u128::from(p.size)).div_ceil(4096) * 4096
        };
    }
    assert_eq!(u128::from(layout.span()), cursor - u128::from(base));
    let resolver = SymbolResolver::new(bytes, &layout, exports)
        .expect("revalidate PAGED policy, not compact policy");
    let prepared = resolver
        .prepare_module(16 * 1024 * 1024, 65_536, 1024)
        .expect("complete data image with page-separated numeric addresses");
    assert_eq!(prepared.sections().len(), layout.placements().len());
    let mut entries = 0;
    for output in prepared.sections() {
        let p = output.placement();
        let header = section(usize::from(p.section_index));
        let mut expected = vec![0; p.size as usize];
        if u32_at(header, 4) != 8 {
            let start = u64_at(header, 24) as usize;
            expected.copy_from_slice(&bytes[start..start + p.size as usize]);
        }
        for index in 1..usize::from(inspected.section_count) {
            let rela = section(index);
            if u32_at(rela, 4) != 4 || u32_at(rela, 44) != u32::from(p.section_index) {
                continue;
            }
            let symtab = u32_at(rela, 40) as u16;
            let start = u64_at(rela, 24) as usize;
            let end = start + u64_at(rela, 32) as usize;
            for entry in bytes[start..end].chunks_exact(24) {
                entries += 1;
                let info = u64_at(entry, 8);
                let kind = info as u32;
                if kind == 0 {
                    continue;
                }
                let symbol = resolver.resolve(symtab, (info >> 32) as u32).unwrap();
                let offset = u64_at(entry, 0) as usize;
                let mut value = i128::from(symbol) + i128::from(u64_at(entry, 16) as i64);
                if matches!(kind, 2 | 4 | 24) {
                    value -= i128::from(p.address) + offset as i128;
                }
                let width = if matches!(kind, 1 | 24) { 8 } else { 4 };
                expected[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
            }
        }
        assert_eq!(
            output.bytes(),
            expected,
            "oracle includes unpatched and opaque NOTE bytes"
        );
    }
    assert_eq!(prepared.relocation_entries(), entries);
    if layout.span() > 0 {
        assert_eq!(
            plan_page_layout(bytes, base, layout.span() - 1, 1024),
            Err(Error::ResourceLimit)
        );
    }
    if let Some(first) = layout.placements().first() {
        let mut writable_code = bytes.to_vec();
        let at = table + usize::from(first.section_index) * 64 + 8;
        writable_code[at..at + 8].copy_from_slice(&(first.flags | 5).to_le_bytes());
        assert_eq!(
            plan_page_layout(&writable_code, base, 16 * 1024 * 1024, 1024),
            Err(Error::WritableExecutableSection)
        );
    }
    assert_eq!(bytes, before);
    // Keep the fixture parser checked even when all ALLOC sections are empty.
    assert_eq!(u16_at(bytes, 18), 62);
    println!(
        "Page-separated preparation: {} sections, {occupied} occupied ranges, {} span bytes, {} payload bytes, {entries} ALLOC RELA entries; page=4096; numeric-only, NOT mapped or executed",
        prepared.sections().len(),
        layout.span(),
        prepared.payload_bytes()
    );
}
