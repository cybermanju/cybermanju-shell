// SPDX-License-Identifier: MPL-2.0

use multiboot2::{
    Builder, EFISdt64Tag, FramebufferField, FramebufferTag, MaybeDynSized, MemoryArea,
    MemoryMapTag, RsdpV1Tag, RsdpV2Tag,
};

use super::*;
use crate::prelude::{ktest, println};

fn memory_builder() -> Builder {
    Builder::new().mmap(MemoryMapTag::new(&[
        MemoryArea::new(0x100000, 0x7fedf000, MemoryAreaType::Available),
        MemoryArea::new(0x7ffdf000, 0x21000, MemoryAreaType::Reserved),
        MemoryArea::new(0xfffc0000, 0x40000, MemoryAreaType::Reserved),
    ]))
}

fn check_reserved_acpi(builder: Builder) {
    let storage = builder.build();
    // SAFETY: The builder owns a valid, aligned MBI throughout this test.
    let info = unsafe { BootInformation::load(storage.as_bytes().as_ptr().cast()) }.unwrap();
    let regions = info.memory_regions(None, None, None);
    let acpi = regions
        .iter()
        .find(|region| (region.base()..region.end()).contains(&0x7ffe2321))
        .unwrap();
    assert_eq!(acpi.typ(), MemoryRegionType::Reclaimable);
    assert!(acpi.typ().is_physical());
    // Do not promote unrelated firmware/ROM reservations.
    let rom = regions
        .iter()
        .find(|region| region.base() == 0xfffc0000)
        .unwrap();
    assert_eq!(rom.typ(), MemoryRegionType::Reserved);
}

#[ktest]
fn reserved_rsdt_remains_in_linear_mapping() {
    check_reserved_acpi(memory_builder().rsdpv1(RsdpV1Tag::new(0, *b"TEST  ", 0, 0x7ffe2300)));
}

#[ktest]
fn reserved_xsdt_remains_in_linear_mapping() {
    check_reserved_acpi(
        memory_builder().rsdpv2(RsdpV2Tag::new(0, *b"TEST  ", 2, 0, 36, 0x7ffe2300, 0)),
    );
}

#[ktest]
fn efi_without_acpi_does_not_promote_reserved_ram() {
    let storage = memory_builder().efi64(EFISdt64Tag::new(0x100000)).build();
    // SAFETY: The builder owns a valid, aligned MBI throughout this test.
    let info = unsafe { BootInformation::load(storage.as_bytes().as_ptr().cast()) }.unwrap();
    assert!(matches!(info.acpi_arg(), BootloaderAcpiArg::NotProvided));
    let regions = info.memory_regions(None, None, None);
    let acpi = regions
        .iter()
        .find(|region| (region.base()..region.end()).contains(&0x7ffe2321))
        .unwrap();
    assert_eq!(acpi.typ(), MemoryRegionType::Reserved);
}

#[ktest]
fn ega_text_is_not_a_pixel_framebuffer() {
    let storage = Builder::new()
        .framebuffer(FramebufferTag::new(
            0xb8000,
            160,
            80,
            25,
            16,
            FramebufferType::Text,
        ))
        .build();
    // SAFETY: The builder owns a valid, aligned MBI throughout this test.
    let info = unsafe { BootInformation::load(storage.as_bytes().as_ptr().cast()) }.unwrap();
    assert!(info.framebuffer_arg().is_none());
}

#[ktest]
fn rgb_framebuffer_preserves_pitch_and_padding() {
    let storage = Builder::new()
        .framebuffer(FramebufferTag::new(
            0x1000,
            16,
            3,
            2,
            32,
            FramebufferType::RGB {
                red: FramebufferField {
                    position: 16,
                    size: 8,
                },
                green: FramebufferField {
                    position: 8,
                    size: 8,
                },
                blue: FramebufferField {
                    position: 0,
                    size: 8,
                },
            },
        ))
        .build();
    // SAFETY: The builder owns a valid, aligned MBI throughout this test.
    let info = unsafe { BootInformation::load(storage.as_bytes().as_ptr().cast()) }.unwrap();
    let fb = info.framebuffer_arg().unwrap();
    assert_eq!(fb.pitch, 16);
    assert_eq!(fb.byte_len(), Some(32));
    let region = MemoryRegion::framebuffer(&fb).unwrap();
    assert_eq!(region.len(), 32);
}

fn fb(
    address: usize,
    width: usize,
    height: usize,
    bpp: usize,
    pitch: usize,
) -> BootloaderFramebufferArg {
    BootloaderFramebufferArg {
        address,
        width,
        height,
        bpp,
        pitch,
    }
}

#[ktest]
fn cybermanju_framebuffer_boot_contract() {
    for bpp in [8, 16, 24, 32] {
        let bytes = bpp / 8;
        let packed = fb(0x1000, 3, 2, bpp, 3 * bytes);
        assert_eq!(packed.byte_len(), Some(6 * bytes));
        let padded = fb(0x1000, 3, 2, bpp, 3 * bytes + 5);
        assert_eq!(padded.byte_len(), Some((3 * bytes + 5) * 2));
        assert_eq!(
            MemoryRegion::framebuffer(&padded).unwrap().len(),
            (3 * bytes + 5) * 2
        );
    }
    let unaligned = fb(0x1001, 3, 2, 32, 16);
    assert_eq!(unaligned.byte_len(), Some(32));
    for invalid in [
        fb(0, 3, 2, 32, 12),
        fb(0x1000, 0, 2, 32, 12),
        fb(0x1000, 3, 0, 32, 12),
        fb(0x1000, 3, 2, 24, 8),
        fb(0x1000, 3, 2, 15, 12),
        fb(usize::MAX, 3, 2, 32, 12),
        fb(0x1000, usize::MAX, 2, 32, 12),
        fb(0x1000, 3, 2, 32, usize::MAX),
        fb(usize::MAX - 15, 3, 2, 32, 16),
    ] {
        assert_eq!(invalid.byte_len(), None);
        assert!(MemoryRegion::framebuffer(&invalid).is_none());
    }
    rgb_framebuffer_preserves_pitch_and_padding();
    ega_text_is_not_a_pixel_framebuffer();
    println!("\nCYBERMANJU_FRAMEBUFFER_CONTRACT:PASS");
}
