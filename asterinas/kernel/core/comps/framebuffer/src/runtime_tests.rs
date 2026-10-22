// SPDX-License-Identifier: MPL-2.0
//! Real boot framebuffer MMIO in a disposable VGA/TCG VM, not GPU acceleration.
use alloc::{sync::Arc, vec::Vec};

use ostd::{
    Error,
    boot::boot_info,
    mm::{HasPaddr, HasSize, PAGE_SIZE, VmIo, VmReader, VmWriter},
    prelude::*,
    sync::Mutex,
};

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

    // A synthetic short surface over real MMIO deliberately leaves accessible
    // bytes beyond its declared span. Testing only the page-aligned real mode
    // would let IoMem's own limit hide a missing FrameBuffer-level guard.
    let short = framebuffer::FrameBuffer {
        io_mem: fb.io_mem().clone(),
        width: 2,
        height: 2,
        line_size: 12,
        byte_len: 24,
        pixel_format: PixelFormat::BgrReserved,
        cmap: Mutex::new(framebuffer::FbCmap {
            entries: Vec::new(),
        }),
    };
    let mut before = [0; 16];
    fb.io_mem().read_bytes(16, &mut before).unwrap();
    assert!(matches!(
        short.write_bytes_at(23, &[0xa1, 0xb2]),
        Err(Error::InvalidArgs)
    ));
    assert!(matches!(
        short.write_bytes_at(24, &[0xc3]),
        Err(Error::InvalidArgs)
    ));
    assert!(matches!(
        short.write_pixel_at(short.calc_offset(0, 2), short.render_pixel(first)),
        Err(Error::InvalidArgs)
    ));
    assert!(matches!(
        short.write_bytes_at(usize::MAX, &[0xd4]),
        Err(Error::InvalidArgs)
    ));
    assert!(matches!(
        short.write_bytes_at(25, &[]),
        Err(Error::InvalidArgs)
    ));
    short.write_bytes_at(24, &[]).unwrap();
    let mut after = [0; 16];
    fb.io_mem().read_bytes(16, &mut after).unwrap();
    assert_eq!(
        after, before,
        "rejected writes must not modify even an in-range prefix"
    );
    println!("CYBERMANJU_FRAMEBUFFER:surface_bounds:PASS cases=6 bytes=24");

    // fbdev byte I/O must use this bounded view, not the larger raw mapping.
    // mmap remains page-granular and is intentionally outside this contract.
    let surface = short.surface_io_mem();
    assert_eq!(surface.size(), 24);
    assert_eq!(surface.paddr(), fb.io_mem().paddr());
    let mut saved_view = [0; 16];
    fb.io_mem().read_bytes(16, &mut saved_view).unwrap();
    fb.io_mem().write_bytes(16, &[0x6d; 16]).unwrap();

    let input = [0xa5; 2];
    let mut reader = VmReader::from(input.as_slice()).to_fallible();
    assert!(matches!(
        surface.write_fallible(23, &mut reader),
        Err((Error::InvalidArgs, 0))
    ));
    assert_eq!(reader.remain(), 2);
    let mut output = [0x9a; 2];
    let mut writer = VmWriter::from(output.as_mut_slice()).to_fallible();
    assert!(matches!(
        surface.read_fallible(23, &mut writer),
        Err((Error::InvalidArgs, 0))
    ));
    assert_eq!(writer.avail(), 2);
    drop(writer);
    assert_eq!(output, [0x9a; 2]);
    let mut observed_view = [0; 16];
    fb.io_mem().read_bytes(16, &mut observed_view).unwrap();
    assert_eq!(observed_view, [0x6d; 16], "no prefix write on rejection");

    let mut reader = VmReader::from(&input[..1]).to_fallible();
    assert_eq!(surface.write_fallible(23, &mut reader).unwrap(), 1);
    assert_eq!(reader.remain(), 0);
    let mut output = [0; 1];
    let mut writer = VmWriter::from(output.as_mut_slice()).to_fallible();
    assert_eq!(surface.read_fallible(23, &mut writer).unwrap(), 1);
    assert_eq!(writer.avail(), 0);
    drop(writer);
    assert_eq!(output, [0xa5]);
    fb.io_mem().read_bytes(16, &mut observed_view).unwrap();
    assert_eq!(&observed_view[..7], &[0x6d; 7]);
    assert_eq!(observed_view[7], 0xa5);
    assert_eq!(&observed_view[8..], &[0x6d; 8]);
    fb.io_mem().write_bytes(16, &saved_view).unwrap();
    println!("CYBERMANJU_FRAMEBUFFER:surface_view:PASS cases=4 bytes=24 guard_bytes=8");

    assert!(framebuffer::mmio_cover(0, 1).is_none());
    assert!(framebuffer::mmio_cover(PAGE_SIZE, 0).is_none());
    assert!(framebuffer::mmio_cover(usize::MAX - 4, 8).is_none());
    assert!(framebuffer::mmio_cover(usize::MAX - 16, 1).is_none());
    assert_eq!(
        framebuffer::mmio_cover(PAGE_SIZE, PAGE_SIZE),
        Some((PAGE_SIZE..2 * PAGE_SIZE, 0))
    );
    assert_eq!(
        framebuffer::mmio_cover(PAGE_SIZE - 1, 2),
        Some((0..2 * PAGE_SIZE, PAGE_SIZE - 1))
    );
    const OFFSET: usize = PAGE_SIZE - 3;
    const SPAN: usize = 24;
    const GUARD: usize = 8;
    let shifted = framebuffer::FrameBuffer::test_subsurface_at(OFFSET, 2, 2, 12).unwrap();
    assert_eq!(shifted.io_mem().paddr(), fb.io_mem().paddr() + OFFSET);
    assert_eq!(shifted.surface_io_mem().size(), SPAN);
    assert_ne!(shifted.io_mem().paddr() % PAGE_SIZE, 0);
    let mut saved = [0; SPAN + 2 * GUARD];
    fb.io_mem().read_bytes(OFFSET - GUARD, &mut saved).unwrap();
    fb.io_mem()
        .write_bytes(OFFSET - GUARD, &[0x6d; SPAN + 2 * GUARD])
        .unwrap();
    shifted.clear();
    let mut observed = [0; SPAN + 2 * GUARD];
    fb.io_mem()
        .read_bytes(OFFSET - GUARD, &mut observed)
        .unwrap();
    assert_eq!(&observed[..GUARD], &[0x6d; GUARD]);
    assert_eq!(&observed[GUARD..GUARD + SPAN], &[0; SPAN]);
    assert_eq!(&observed[GUARD + SPAN..], &[0x6d; GUARD]);
    shifted
        .write_pixel_at(shifted.calc_offset(0, 0), shifted.render_pixel(first))
        .unwrap();
    shifted
        .write_pixel_at(shifted.calc_offset(1, 1), shifted.render_pixel(second))
        .unwrap();
    assert!(matches!(
        shifted.write_bytes_at(SPAN - 1, &[0xa1, 0xb2]),
        Err(Error::InvalidArgs)
    ));
    let mut first_pixel = [0; 4];
    let mut last_pixel = [0; 4];
    shifted
        .surface_io_mem()
        .read_bytes(0, &mut first_pixel)
        .unwrap();
    shifted
        .surface_io_mem()
        .read_bytes(16, &mut last_pixel)
        .unwrap();
    assert_eq!(first_pixel, expected[..4]);
    assert_eq!(last_pixel, expected[4..]);
    fb.io_mem()
        .read_bytes(OFFSET - GUARD, &mut observed)
        .unwrap();
    assert_eq!(&observed[..GUARD], &[0x6d; GUARD]);
    assert_eq!(&observed[GUARD + SPAN..], &[0x6d; GUARD]);
    fb.io_mem().write_bytes(OFFSET - GUARD, &saved).unwrap();
    fb.io_mem()
        .read_bytes(OFFSET - GUARD, &mut observed)
        .unwrap();
    assert_eq!(observed, saved);
    println!(
        "CYBERMANJU_FRAMEBUFFER:unaligned_surface:PASS offset={} bytes=24 guard_bytes=8",
        OFFSET
    );

    // Exercise the public console paths on a non-page-sized logical surface.
    // The original console allocated by the larger mapping and panicked here.
    const CONSOLE_BYTES: usize = 36 * 16;
    let console_fb = framebuffer::FrameBuffer {
        io_mem: fb.io_mem().clone(),
        width: 8,
        height: 16,
        line_size: 36,
        byte_len: CONSOLE_BYTES,
        pixel_format: PixelFormat::BgrReserved,
        cmap: Mutex::new(framebuffer::FbCmap {
            entries: Vec::new(),
        }),
    };
    let mut saved = [0; CONSOLE_BYTES + 8];
    fb.io_mem().read_bytes(0, &mut saved).unwrap();
    let mut console = crate::console::FramebufferConsole::new(Arc::new(console_fb));
    for returning_from_graphics in [false, true] {
        if returning_from_graphics {
            console.set_mode(crate::mode::ConsoleMode::Graphics);
        }
        fb.io_mem()
            .write_bytes(0, &[0x6d; CONSOLE_BYTES + 8])
            .unwrap();
        if returning_from_graphics {
            console.set_mode(crate::mode::ConsoleMode::Text);
        } else {
            console.activate();
        }
        let mut observed = [0; CONSOLE_BYTES + 8];
        fb.io_mem().read_bytes(0, &mut observed).unwrap();
        assert_eq!(
            &observed[..CONSOLE_BYTES],
            &[0; CONSOLE_BYTES],
            "flush must cover the surface"
        );
        assert_eq!(
            &observed[CONSOLE_BYTES..],
            &[0x6d; 8],
            "flush must preserve the tail"
        );
    }
    fb.io_mem()
        .write_bytes(0, &[0x6d; CONSOLE_BYTES + 8])
        .unwrap();
    console.send(b"\n\n"); // A segunda linha força scroll com a fonte básica 8x8.
    let mut observed = [0; CONSOLE_BYTES + 8];
    fb.io_mem().read_bytes(0, &mut observed).unwrap();
    assert_eq!(&observed[..CONSOLE_BYTES], &[0; CONSOLE_BYTES]);
    assert_eq!(&observed[CONSOLE_BYTES..], &[0x6d; 8]);
    fb.io_mem().write_bytes(0, &saved).unwrap();
    println!(
        "CYBERMANJU_FRAMEBUFFER:console_flush:PASS cases=3 bytes={} guard_bytes=8",
        CONSOLE_BYTES
    );
}
