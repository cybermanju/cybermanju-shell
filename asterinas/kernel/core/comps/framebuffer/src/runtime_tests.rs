// SPDX-License-Identifier: MPL-2.0
//! Real boot framebuffer MMIO in a disposable VGA/TCG VM, not GPU acceleration.
use ostd::{boot::boot_info, mm::VmIo, prelude::*};

use crate::{
    framebuffer,
    pixel::{Pixel, PixelFormat},
};

#[ktest]
fn cybermanju_framebuffer_runtime() {
    let arg = boot_info()
        .framebuffer_arg
        .expect("RGB boot framebuffer required; absence is not a pass");
    assert_eq!(
        arg.bpp, 32,
        "this runtime qualifies the fixed 32-bpp test mode only"
    );
    assert!(arg.width >= 2 && arg.height >= 2);
    let byte_len = arg.byte_len().expect("validated geometry/pitch/range");
    if framebuffer::FRAMEBUFFER.get().is_none() {
        framebuffer::init();
    }
    let fb = framebuffer::FRAMEBUFFER
        .get()
        .expect("real FrameBuffer MMIO acquisition required");
    assert_eq!(
        (fb.width(), fb.height(), fb.line_size(), fb.byte_len()),
        (arg.width, arg.height, arg.pitch, byte_len)
    );
    assert_eq!(fb.pixel_format(), PixelFormat::BgrReserved);
    println!(
        "\nCYBERMANJU_FRAMEBUFFER:boot:PASS width={} height={} bpp={} pitch={} bytes={}",
        arg.width, arg.height, arg.bpp, arg.pitch, byte_len
    );

    let first = Pixel {
        red: 0x12,
        green: 0x34,
        blue: 0x56,
    };
    let second = Pixel {
        red: 0x7a,
        green: 0x8b,
        blue: 0x9c,
    };
    let expected = [0x56, 0x34, 0x12, 0, 0x9c, 0x8b, 0x7a, 0];
    for y in [0, arg.height - 1] {
        let offset = y.checked_mul(arg.pitch).unwrap();
        assert_eq!(fb.calc_offset(0, y).as_usize(), offset);
        assert_eq!(
            fb.calc_offset(1, y).as_usize(),
            offset.checked_add(4).unwrap()
        );
        let mut saved = [0; 8];
        fb.io_mem().read_bytes(offset, &mut saved).unwrap();
        assert_eq!(saved, [0; 8], "clear must cover sampled first/last rows");
        fb.write_pixel_at(fb.calc_offset(0, y), fb.render_pixel(first))
            .unwrap();
        fb.write_pixel_at(fb.calc_offset(1, y), fb.render_pixel(second))
            .unwrap();
        let mut observed = [0; 8];
        fb.io_mem().read_bytes(offset, &mut observed).unwrap();
        assert_eq!(observed, expected);
        fb.write_bytes_at(offset, &saved).unwrap();
        fb.io_mem().read_bytes(offset, &mut observed).unwrap();
        assert_eq!(observed, saved, "sampled pixels restored");
    }
    // A readback from the device aperture does not measure monitor scanout,
    // presentation feedback, color correctness, Vulkan, GPUI or physical FPS.
    println!("CYBERMANJU_FRAMEBUFFER:io_readback:PASS rows=2 row_bytes=8");
}
