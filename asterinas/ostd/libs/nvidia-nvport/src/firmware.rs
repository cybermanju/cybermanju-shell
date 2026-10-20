// SPDX-License-Identifier: MPL-2.0
//! NVIDIA RM firmware ownership backed by an immutable boot `newc` archive.
//!
//! This implements file acquisition/release, not firmware authentication, GSP
//! loading, DMA, or a GPU boot handshake. Only NVIDIA's release-pinned filename
//! selector chooses paths; symlinks, duplicates, malformed and compressed archives
//! fail closed. Firmware bytes remain pinned by the boot archive's static lifetime.

use alloc::alloc::{alloc, dealloc};
use core::{
    alloc::Layout,
    ffi::{CStr, c_char, c_void},
    ptr,
};

use spin::Once;

/// Firmware release required by the linked NVIDIA source.
pub const RELEASE: &str = "615.78.08";
/// Maximum accepted individual firmware payload (128 MiB), not an allocation size.
pub const MAX_FIRMWARE_SIZE: usize = 128 * 1024 * 1024;

/// NVIDIA's firmware image kinds, excluding the VMware-only bindata variant.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirmwareKind {
    /// GSP-RM image.
    Gsp = 0,
    /// Optional GSP log symbols.
    GspLog = 1,
    /// Companion microcode image.
    Ucodes = 2,
}

/// Families recognized by the exact upstream release's filename selector.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChipFamily {
    /// Turing TU10x.
    Tu10x = 1,
    /// Turing TU11x.
    Tu11x = 2,
    /// Ampere GA100.
    Ga100 = 3,
    /// Ampere GA10x.
    Ga10x = 4,
    /// Ada AD10x.
    Ad10x = 5,
    /// Hopper GH100.
    Gh100 = 6,
    /// Blackwell GB10x.
    Gb10x = 8,
    /// Blackwell GB20x.
    Gb20x = 9,
    /// GR10x.
    Gr10x = 10,
    /// GB10y.
    Gb10y = 11,
    /// GB20y.
    Gb20y = 12,
}

/// Firmware retrieval failures; none indicates a running GSP or GPU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirmwareError {
    /// Only uncompressed, non-CRC `newc` archives are accepted by this provider.
    UnsupportedArchive,
    /// A header, filename, size, padding or trailer is invalid or truncated.
    MalformedArchive,
    /// The requested release-specific regular file is absent.
    NotFound,
    /// More than one archive entry resolves to the requested path.
    Duplicate,
    /// A matching entry is not a standalone regular file (no symlink/hardlink following).
    NotRegularFile,
    /// The image is empty or exceeds the provider's explicit size limit.
    InvalidSize,
    /// Another boot archive was already installed.
    AlreadyInstalled,
    /// No immutable boot archive has been registered.
    NotInitialized,
    /// The real handle allocation failed.
    NoMemory,
    /// Firmware acquisition cannot run in interrupt context.
    InvalidContext,
    /// The FFI provider returned bytes inconsistent with the registered image.
    ContractViolation,
}

unsafe extern "C" {
    fn aster_nv_firmware_path(kind: u32, family: u32) -> *const c_char;
    fn aster_nv_firmware_acquire(
        kind: u32,
        family: u32,
        buffer: *mut *const c_void,
        size: *mut u32,
    ) -> *const c_void;
    fn aster_nv_firmware_release(handle: *const c_void);
}

/// Returns NVIDIA's own exact, relative `/lib/firmware` filename for this release.
pub fn filename(kind: FirmwareKind, family: ChipFamily) -> &'static str {
    // SAFETY: The C bridge calls the reviewed static-inline upstream selector
    // with valid enum values and returns an immutable, NUL-terminated literal.
    let path = unsafe { aster_nv_firmware_path(kind as u32, family as u32) };
    // SAFETY: The bridge's result is a static C string for every input.
    unsafe { CStr::from_ptr(path) }
        .to_str()
        .expect("upstream firmware path is ASCII")
}

#[derive(Clone, Copy)]
struct Entry<'a> {
    name: &'a str,
    data: &'a [u8],
    mode: u32,
    links: u32,
}

/// A fully bounds-checked, borrowed `newc` archive. No file data is copied.
pub struct FirmwareArchive<'a> {
    bytes: &'a [u8],
}

fn hex(bytes: &[u8], offset: usize) -> Result<u32, FirmwareError> {
    let field = bytes
        .get(offset..offset + 8)
        .ok_or(FirmwareError::MalformedArchive)?;
    if !field.iter().all(u8::is_ascii_hexdigit) {
        return Err(FirmwareError::MalformedArchive);
    }
    let text = core::str::from_utf8(field).map_err(|_| FirmwareError::MalformedArchive)?;
    u32::from_str_radix(text, 16).map_err(|_| FirmwareError::MalformedArchive)
}

fn align4(value: usize) -> Result<usize, FirmwareError> {
    value
        .checked_add(3)
        .map(|value| value & !3)
        .ok_or(FirmwareError::MalformedArchive)
}

fn next_entry<'a>(bytes: &'a [u8], cursor: &mut usize) -> Result<Entry<'a>, FirmwareError> {
    let start = *cursor;
    let header_end = start
        .checked_add(110)
        .ok_or(FirmwareError::MalformedArchive)?;
    let header = bytes
        .get(start..header_end)
        .ok_or(FirmwareError::MalformedArchive)?;
    if &header[..6] != b"070701" {
        return Err(FirmwareError::UnsupportedArchive);
    }
    // Validate *all* header fields, not just those needed for a lookup.
    for offset in (6..110).step_by(8) {
        hex(header, offset)?;
    }
    if hex(header, 102)? != 0 {
        return Err(FirmwareError::MalformedArchive);
    }
    let name_len = hex(header, 94)? as usize;
    if name_len == 0 || name_len > 4096 {
        return Err(FirmwareError::MalformedArchive);
    }
    let name_end = header_end
        .checked_add(name_len)
        .ok_or(FirmwareError::MalformedArchive)?;
    let name = bytes
        .get(header_end..name_end)
        .ok_or(FirmwareError::MalformedArchive)?;
    let name = CStr::from_bytes_with_nul(name).map_err(|_| FirmwareError::MalformedArchive)?;
    let name = name.to_str().map_err(|_| FirmwareError::MalformedArchive)?;
    let data_start = align4(name_end)?;
    let size = hex(header, 54)? as usize;
    let data_end = data_start
        .checked_add(size)
        .ok_or(FirmwareError::MalformedArchive)?;
    let end = align4(data_end)?;
    let data = bytes
        .get(data_start..data_end)
        .ok_or(FirmwareError::MalformedArchive)?;
    if end > bytes.len() {
        return Err(FirmwareError::MalformedArchive);
    }
    *cursor = end;
    Ok(Entry {
        name,
        data,
        mode: hex(header, 14)?,
        links: hex(header, 38)?,
    })
}

fn canonical_name(mut name: &str) -> Option<&str> {
    // Accept the conventional './' or '/' archive prefix, but never resolve
    // traversal, repeated separators or interior dot components.
    name = name.strip_prefix('/').unwrap_or(name);
    name = name.strip_prefix("./").unwrap_or(name);
    if name.is_empty()
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    Some(name)
}

impl<'a> FirmwareArchive<'a> {
    /// Validates the entire archive before it can supply firmware. Nonzero
    /// trailing data, missing trailer and truncated unrelated entries are errors.
    pub fn new(bytes: &'a [u8]) -> Result<Self, FirmwareError> {
        if !bytes.starts_with(b"070701") {
            return Err(FirmwareError::UnsupportedArchive);
        }
        let mut cursor = 0;
        loop {
            let entry = next_entry(bytes, &mut cursor)?;
            if entry.name == "TRAILER!!!" {
                if !entry.data.is_empty() || bytes[cursor..].iter().any(|&byte| byte != 0) {
                    return Err(FirmwareError::MalformedArchive);
                }
                return Ok(Self { bytes });
            }
        }
    }

    /// Borrows an exact release-specific image. This does not validate its
    /// vendor signature or internal image format; the real RM must do that.
    pub fn get(&self, kind: FirmwareKind, family: ChipFamily) -> Result<&'a [u8], FirmwareError> {
        self.get_path(filename(kind, family))
    }

    fn get_path(&self, relative: &str) -> Result<&'a [u8], FirmwareError> {
        let mut cursor = 0;
        let mut found = None;
        loop {
            let entry = next_entry(self.bytes, &mut cursor)?;
            if entry.name == "TRAILER!!!" {
                return found.ok_or(FirmwareError::NotFound);
            }
            let Some(name) = canonical_name(entry.name) else {
                continue;
            };
            let Some(name) = name.strip_prefix("lib/firmware/") else {
                continue;
            };
            if name != relative {
                continue;
            }
            if found.is_some() {
                return Err(FirmwareError::Duplicate);
            }
            if entry.mode & 0o170000 != 0o100000 || entry.links != 1 {
                return Err(FirmwareError::NotRegularFile);
            }
            if entry.data.is_empty() || entry.data.len() > MAX_FIRMWARE_SIZE {
                return Err(FirmwareError::InvalidSize);
            }
            found = Some(entry.data);
        }
    }
}

static ARCHIVE: Once<FirmwareArchive<'static>> = Once::new();

/// Registers real immutable boot archive storage once. Re-registering the exact
/// same slice is idempotent; a different source is rejected, including races.
/// This accepts neither host files nor a fabricated GPU-success callback.
pub fn install_boot_archive(bytes: &'static [u8]) -> Result<(), FirmwareError> {
    let archive = FirmwareArchive::new(bytes)?;
    let installed = ARCHIVE.call_once(|| archive);
    if installed.bytes.as_ptr() != bytes.as_ptr() || installed.bytes.len() != bytes.len() {
        return Err(FirmwareError::AlreadyInstalled);
    }
    Ok(())
}

/// Retrieves a borrowed image from the registered boot archive.
pub fn image(kind: FirmwareKind, family: ChipFamily) -> Result<&'static [u8], FirmwareError> {
    ARCHIVE
        .get()
        .ok_or(FirmwareError::NotInitialized)?
        .get(kind, family)
}

/// An owning lease acquired through the actual RM firmware ABI. The handle
/// cannot be duplicated or manually released by safe callers.
pub struct FirmwareLease {
    handle: ptr::NonNull<c_void>,
    bytes: &'static [u8],
}

impl FirmwareLease {
    /// Acquires an image from the real installed boot archive, not a Linux host.
    pub fn acquire(kind: FirmwareKind, family: ChipFamily) -> Result<Self, FirmwareError> {
        #[cfg(target_os = "none")]
        if ostd::irq::InterruptLevel::current().is_interrupt_context() {
            return Err(FirmwareError::InvalidContext);
        }
        let expected = image(kind, family)?;
        let mut data = ptr::null();
        let mut size = 0;
        // SAFETY: Valid, distinct output slots. This provider does not
        // dereference nv_state_t; a successful handle is immediately owned.
        let handle =
            unsafe { aster_nv_firmware_acquire(kind as u32, family as u32, &mut data, &mut size) };
        let handle = ptr::NonNull::new(handle.cast_mut()).ok_or(FirmwareError::NoMemory)?;
        if data != expected.as_ptr().cast() || size as usize != expected.len() {
            // SAFETY: This is the exact owned handle and no bytes were exposed.
            unsafe { aster_nv_firmware_release(handle.as_ptr()) };
            return Err(FirmwareError::ContractViolation);
        }
        Ok(Self {
            handle,
            bytes: expected,
        })
    }

    /// Exact, immutable firmware file bytes; no signature or GSP boot claim.
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes
    }
}

impl Drop for FirmwareLease {
    fn drop(&mut self) {
        // SAFETY: Unique ownership ensures one release after all borrows end.
        unsafe { aster_nv_firmware_release(self.handle.as_ptr()) };
    }
}

struct Handle {
    bytes: &'static [u8],
}

fn raw_filename(kind: u32, family: u32) -> Option<&'static str> {
    // SAFETY: The bridge validates all raw enum values before the upstream call
    // and returns a static string, with the empty string meaning invalid input.
    let path = unsafe { CStr::from_ptr(aster_nv_firmware_path(kind, family)) };
    let path = path.to_str().ok()?;
    (!path.is_empty()).then_some(path)
}

/// RM firmware acquisition ABI. `nv` is not dereferenced: unlike Linux, this
/// provider has no Linux `struct device` dependency and no per-device fallback.
///
/// # Safety
/// Both output slots must be aligned, writable and nonoverlapping for the call.
/// Call in task context, never in an interrupt or while holding a spinlock.
/// On success the handle must be released exactly once by `nv_put_firmware`,
/// after all foreign accesses to the returned firmware bytes have completed.
#[unsafe(no_mangle)]
unsafe extern "C" fn nv_get_firmware(
    _nv: *mut c_void,
    kind: u32,
    family: u32,
    buffer: *mut *const c_void,
    size: *mut u32,
) -> *const c_void {
    if buffer.is_null() || size.is_null() {
        return ptr::null();
    }
    // SAFETY: Caller grants exclusive access to both output slots.
    unsafe {
        buffer.write(ptr::null());
        size.write(0);
    }
    #[cfg(target_os = "none")]
    if ostd::irq::InterruptLevel::current().is_interrupt_context() {
        return ptr::null();
    }
    let Some(path) = raw_filename(kind, family) else {
        return ptr::null();
    };
    let Some(archive) = ARCHIVE.get() else {
        return ptr::null();
    };
    let Ok(bytes) = archive.get_path(path) else {
        return ptr::null();
    };
    let layout = Layout::new::<Handle>();
    // SAFETY: Nonzero valid layout. Null allocation is reported as failure.
    let handle = unsafe { alloc(layout) }.cast::<Handle>();
    if handle.is_null() {
        return ptr::null();
    }
    // SAFETY: New handle is exclusively owned. Archive bytes are static,
    // nonempty and bounded below u32::MAX by the validated provider.
    unsafe {
        handle.write(Handle { bytes });
        let stored = &*handle;
        buffer.write(stored.bytes.as_ptr().cast());
        size.write(stored.bytes.len() as u32);
    }
    handle.cast()
}

/// Releases one actual RM firmware ownership handle; NULL is harmless.
///
/// # Safety
/// `handle` must be NULL or an unreleased handle returned by `nv_get_firmware`.
/// No foreign accesses associated with this handle may outlive this call.
#[unsafe(no_mangle)]
unsafe extern "C" fn nv_put_firmware(handle: *const c_void) {
    if handle.is_null() {
        return;
    }
    let handle = handle.cast_mut().cast::<Handle>();
    // SAFETY: Caller returns exclusive ownership of the exact live handle.
    unsafe {
        ptr::drop_in_place(handle);
        dealloc(handle.cast(), Layout::new::<Handle>());
    }
}

#[cfg(any(test, ktest))]
mod tests {
    use alloc::{boxed::Box, format, vec::Vec};

    use super::*;

    fn entry(out: &mut Vec<u8>, name: &str, bytes: &[u8], mode: u32, links: u32) {
        let fields = [
            1,
            mode,
            0,
            0,
            links,
            0,
            bytes.len() as u32,
            0,
            0,
            0,
            0,
            (name.len() + 1) as u32,
            0,
        ];
        out.extend_from_slice(b"070701");
        for field in fields {
            out.extend_from_slice(format!("{field:08x}").as_bytes());
        }
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        out.extend_from_slice(bytes);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    fn finish(out: &mut Vec<u8>) {
        entry(out, "TRAILER!!!", &[], 0, 1);
    }
    fn path() -> &'static str {
        "lib/firmware/nvidia/615.78.08/gsp_tu10x.bin"
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn upstream_release_and_family_selection() {
        assert_eq!(
            filename(FirmwareKind::Gsp, ChipFamily::Tu10x),
            "nvidia/615.78.08/gsp_tu10x.bin"
        );
        assert_eq!(
            filename(FirmwareKind::Ucodes, ChipFamily::Ga100),
            "nvidia/615.78.08/ucodes_tu10x.bin"
        );
        for family in [
            ChipFamily::Ga10x,
            ChipFamily::Ad10x,
            ChipFamily::Gh100,
            ChipFamily::Gb10x,
            ChipFamily::Gb20x,
            ChipFamily::Gr10x,
            ChipFamily::Gb10y,
            ChipFamily::Gb20y,
        ] {
            assert_eq!(
                filename(FirmwareKind::GspLog, family),
                "nvidia/615.78.08/gsp_log_ga10x.bin"
            );
        }
        for (kind, family) in [(3, 1), (0, 0), (0, 7), (0, 13), (u32::MAX, u32::MAX)] {
            assert_eq!(raw_filename(kind, family), None);
        }
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn archive_exact_image_and_no_cross_release_fallback() {
        let mut bytes = Vec::new();
        entry(
            &mut bytes,
            "lib/firmware/nvidia/615.71.09/gsp_tu10x.bin",
            &[9],
            0o100644,
            1,
        );
        entry(&mut bytes, path(), &[1, 2, 3, 4, 5], 0o100644, 1);
        finish(&mut bytes);
        let archive = FirmwareArchive::new(&bytes).unwrap();
        assert_eq!(
            archive.get(FirmwareKind::Gsp, ChipFamily::Tu10x),
            Ok(&[1, 2, 3, 4, 5][..])
        );
        assert_eq!(
            archive.get(FirmwareKind::Gsp, ChipFamily::Ga10x),
            Err(FirmwareError::NotFound)
        );
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn archive_rejects_every_truncation_and_bad_numeric_header() {
        let mut bytes = Vec::new();
        entry(&mut bytes, path(), &[1, 2, 3], 0o100644, 1);
        finish(&mut bytes);
        for end in 0..bytes.len() {
            assert!(FirmwareArchive::new(&bytes[..end]).is_err(), "{end}");
        }
        bytes[6] = b'g';
        assert!(matches!(
            FirmwareArchive::new(&bytes),
            Err(FirmwareError::MalformedArchive)
        ));
        assert!(matches!(
            FirmwareArchive::new(&[0x1f, 0x8b]),
            Err(FirmwareError::UnsupportedArchive)
        ));
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn archive_rejects_alias_duplicates_links_empty_and_traversal() {
        let mut bytes = Vec::new();
        entry(&mut bytes, path(), &[1], 0o100644, 1);
        entry(&mut bytes, &format!("./{}", path()), &[2], 0o100644, 1);
        finish(&mut bytes);
        assert_eq!(
            FirmwareArchive::new(&bytes)
                .unwrap()
                .get(FirmwareKind::Gsp, ChipFamily::Tu10x),
            Err(FirmwareError::Duplicate)
        );
        for (mode, links, data, error) in [
            (0o120777, 1, &[1][..], FirmwareError::NotRegularFile),
            (0o100644, 2, &[1][..], FirmwareError::NotRegularFile),
            (0o100644, 1, &[][..], FirmwareError::InvalidSize),
        ] {
            let mut bytes = Vec::new();
            entry(&mut bytes, path(), data, mode, links);
            finish(&mut bytes);
            assert_eq!(
                FirmwareArchive::new(&bytes)
                    .unwrap()
                    .get(FirmwareKind::Gsp, ChipFamily::Tu10x),
                Err(error)
            );
        }
        assert_eq!(canonical_name("./lib/../firmware/x"), None);
        assert_eq!(canonical_name("lib//firmware/x"), None);
        assert_eq!(canonical_name("/./lib/firmware/x"), Some("lib/firmware/x"));
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn actual_rm_ffi_handles_own_distinct_leases_of_static_fixture() {
        // Explicit CPU-only ABI fixture, never a signed GSP image or GPU result.
        let mut bytes = Vec::new();
        entry(&mut bytes, path(), &[7, 8, 9, 10], 0o100644, 1);
        finish(&mut bytes);
        let bytes = Box::leak(bytes.into_boxed_slice());
        install_boot_archive(bytes).unwrap();
        assert_eq!(install_boot_archive(bytes), Ok(()));
        let lease = FirmwareLease::acquire(FirmwareKind::Gsp, ChipFamily::Tu10x).unwrap();
        assert_eq!(lease.as_bytes(), &[7, 8, 9, 10]);
        drop(lease);
        let mut other = bytes.to_vec();
        other.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(
            install_boot_archive(Box::leak(other.into_boxed_slice())),
            Err(FirmwareError::AlreadyInstalled)
        );
        let mut first = ptr::null();
        let mut len = 0;
        // SAFETY: Valid output slots; successful handles are freed exactly once.
        unsafe {
            let a = nv_get_firmware(ptr::null_mut(), 0, 1, &mut first, &mut len);
            assert!(!a.is_null());
            assert_eq!(len, 4);
            assert_eq!(
                core::slice::from_raw_parts(first.cast::<u8>(), len as usize),
                &[7, 8, 9, 10]
            );
            assert_eq!((*a.cast::<Handle>()).bytes, &[7, 8, 9, 10]);
            let mut second = ptr::null();
            let b = nv_get_firmware(ptr::null_mut(), 0, 1, &mut second, &mut len);
            assert!(!b.is_null());
            assert_ne!(a, b);
            assert_eq!(first, second);
            nv_put_firmware(a);
            assert_eq!(
                core::slice::from_raw_parts(second.cast::<u8>(), len as usize),
                &[7, 8, 9, 10]
            );
            nv_put_firmware(b);
            nv_put_firmware(ptr::null());
            first = ptr::dangling();
            len = 99;
            assert!(nv_get_firmware(ptr::null_mut(), 0, 4, &mut first, &mut len).is_null());
            assert!(first.is_null());
            assert_eq!(len, 0);
            assert!(nv_get_firmware(ptr::null_mut(), 0, 1, ptr::null_mut(), &mut len).is_null());
        }
    }
}
