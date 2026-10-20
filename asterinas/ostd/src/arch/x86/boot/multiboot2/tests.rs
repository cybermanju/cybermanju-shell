// SPDX-License-Identifier: MPL-2.0

use multiboot2::{
    Builder, EFISdt64Tag, FramebufferTag, MaybeDynSized, MemoryArea, MemoryMapTag, RsdpV1Tag,
    RsdpV2Tag,
};

use super::*;
use crate::prelude::ktest;

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
