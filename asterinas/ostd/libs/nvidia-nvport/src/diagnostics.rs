// SPDX-License-Identifier: MPL-2.0

//! Bounded diagnostics for the unmodified NVIDIA Unix-kernel NVport sources.

use core::ffi::c_char;

/// Maximum number of input bytes read from an NVIDIA diagnostic C string.
const MAX_C_STRING_BYTES: usize = 256;

/// Sink for fully formatted diagnostics. It never interprets format strings.
///
/// # Safety
/// A nonzero `length` requires that many immutable readable bytes at `message`.
/// The C formatter's fixed output budget is 1024 bytes; larger lengths halt.
#[unsafe(no_mangle)]
unsafe extern "C" fn aster_nv_log_bytes(message: *const c_char, length: u32) {
    assert!(
        length <= 1024,
        "NVIDIA formatted log exceeds its ABI budget"
    );
    assert!(
        !message.is_null() || length == 0,
        "NVIDIA formatted log has a null pointer"
    );
    let bytes = if length == 0 {
        &[][..]
    } else {
        // SAFETY: Caller guarantees this bounded immutable readable range.
        unsafe { core::slice::from_raw_parts(message.cast::<u8>(), length as usize) }
    };
    if let Ok(text) = core::str::from_utf8(bytes) {
        ostd::early_println!("nvidia-nvport: {}", text);
    } else {
        ostd::early_print!("nvidia-nvport: <C bytes ");
        for byte in bytes {
            ostd::early_print!("\\x{byte:02x}");
        }
        ostd::early_println!(">");
    }
}

/// Logs a bounded C string from NVIDIA's `out_string` callback.
///
/// The callback contract requires `message` to point to a readable NUL-terminated
/// C string. To keep accidental unterminated strings bounded, at most 256 bytes
/// are read and the log explicitly marks truncation. Valid UTF-8 is printed as
/// text; other byte sequences are escaped losslessly as `\\xNN` bytes.
///
/// # Safety
/// `message` must be non-null and point to readable memory through its NUL
/// terminator, or through at least 256 readable bytes if there is no terminator
/// within that bound.
#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn out_string(message: *const c_char) {
    if message.is_null() {
        ostd::early_println!("nvidia-nvport: out_string(<null>)");
        return;
    }

    let mut len = 0;
    let mut terminated = false;
    while len < MAX_C_STRING_BYTES {
        // SAFETY: The callback contract guarantees readable bytes through the
        // terminator, or at least MAX_C_STRING_BYTES bytes for a bounded string.
        let byte = unsafe { message.cast::<u8>().add(len).read() };
        if byte == 0 {
            terminated = true;
            break;
        }
        len += 1;
    }

    // SAFETY: Every byte in this bounded range was just read under the pointer
    // validity contract, and the immutable C string remains readable for the call.
    let bytes = unsafe { core::slice::from_raw_parts(message.cast::<u8>(), len) };
    match core::str::from_utf8(bytes) {
        Ok(text) if terminated => ostd::early_println!("nvidia-nvport: {}", text),
        Ok(text) => ostd::early_println!(
            "nvidia-nvport: {} [truncated after {} bytes]",
            text,
            MAX_C_STRING_BYTES
        ),
        Err(_) => {
            // Hex-escape every byte without allocation. This preserves arbitrary
            // C-string bytes in a printable, fixed-size diagnostic.
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let mut escaped = [0u8; MAX_C_STRING_BYTES * 4];
            for (index, &byte) in bytes.iter().enumerate() {
                let offset = index * 4;
                escaped[offset..offset + 4].copy_from_slice(&[
                    b'\\',
                    b'x',
                    HEX[(byte >> 4) as usize],
                    HEX[(byte & 0x0f) as usize],
                ]);
            }
            // SAFETY: The escaped buffer consists exclusively of ASCII bytes.
            let escaped = unsafe { core::str::from_utf8_unchecked(&escaped[..len * 4]) };
            if terminated {
                ostd::early_println!("nvidia-nvport: <C bytes {}>", escaped);
            } else {
                ostd::early_println!(
                    "nvidia-nvport: <C bytes {}> [truncated after {} bytes]",
                    escaped,
                    MAX_C_STRING_BYTES
                );
            }
        }
    }
}

/// Fails closed when NVIDIA requests a debug breakpoint.
///
/// This is intentionally not a no-op: reaching a breakpoint in the unmodified
/// NVport code is treated as a kernel-fatal invariant/debug failure.
#[unsafe(no_mangle)]
pub(crate) extern "C" fn os_dbg_breakpoint() -> ! {
    panic!("nvidia-nvport: upstream requested os_dbg_breakpoint");
}

#[cfg(ktest)]
mod tests {
    use core::ffi::{c_char, c_int};
    unsafe extern "C" {
        fn aster_nv_snprintf(
            buffer: *mut c_char,
            capacity: u32,
            format: *const c_char,
            ...
        ) -> c_int;
        fn nv_printf(level: u32, format: *const c_char, ...) -> c_int;
    }

    #[ostd::prelude::ktest]
    fn actual_freestanding_formatter_through_c_abi() {
        let mut bytes = [0u8; 128];
        // SAFETY: Buffer and C strings are valid; variadic arguments exactly
        // match the format's promoted C integer/pointer types on x86-64.
        let count = unsafe {
            aster_nv_snprintf(
                bytes.as_mut_ptr().cast(),
                bytes.len() as u32,
                c"%#08x|%+lld|%.3s|%zu|%%".as_ptr(),
                42u32,
                7i64,
                c"abcdef".as_ptr(),
                12usize,
            )
        };
        let expected = b"0x00002a|+7|abc|12|%";
        assert_eq!(count as usize, expected.len());
        assert_eq!(&bytes[..expected.len()], expected);
        assert_eq!(bytes[expected.len()], 0);
        let mut short = [0u8; 5];
        let count =
            unsafe { aster_nv_snprintf(short.as_mut_ptr().cast(), 5, c"%u".as_ptr(), 123456u32) };
        assert_eq!(count, 6);
        assert_eq!(&short, b"1234\0");
        let mut untouched = 77i32;
        let count = unsafe {
            aster_nv_snprintf(
                bytes.as_mut_ptr().cast(),
                128,
                c"%n".as_ptr(),
                &mut untouched as *mut i32,
            )
        };
        assert_eq!(count, -1);
        assert_eq!(untouched, 77);
        let count = unsafe {
            nv_printf(
                u32::MAX,
                c"native formatter CPU-only check=%u".as_ptr(),
                17u32,
            )
        };
        assert_eq!(count as usize, b"native formatter CPU-only check=17".len());
    }
}
