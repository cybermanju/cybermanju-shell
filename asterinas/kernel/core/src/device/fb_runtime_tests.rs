// SPDX-License-Identifier: MPL-2.0
//! Direct FileOps only, not a userspace syscall, ioctl dispatch or mmap test.
use ostd::prelude::{ktest, println};

use super::*;

#[ktest]
fn cybermanju_fbdev_fileops_runtime() {
    let arg = ostd::boot::boot_info()
        .framebuffer_arg
        .expect("real RGB boot framebuffer required");
    assert_eq!(arg.bpp, 32);
    let surface = FrameBuffer::test_subsurface(2, 2, 12).unwrap();
    assert_eq!(surface.byte_len(), 24);
    assert!(surface.io_mem().size() > 32);
    let handle = FbHandle {
        framebuffer: Arc::new(surface),
    };
    let info = handle.collect_fix_screen_info();
    assert_eq!(info.smem_len, 24);
    assert_eq!(info.line_length, 12);
    assert_eq!(info.smem_start, handle.framebuffer.io_mem().paddr() as u64);
    println!("\nCYBERMANJU_FBDEV:surface:PASS bytes=24 pitch=12 smem_len=24 mapping_larger=true");

    let mmio = handle.framebuffer.io_mem();
    let mut saved = [0; 32];
    mmio.read_bytes(0, &mut saved).unwrap();
    mmio.write_bytes(0, &[0x6d; 32]).unwrap();

    // A crossing read is short, not an error, and only advances one byte.
    let mut output = [0x9a; 2];
    {
        let mut writer = VmWriter::from(output.as_mut_slice()).to_fallible();
        assert_eq!(
            handle
                .read_at(23, &mut writer, StatusFlags::empty())
                .unwrap(),
            1
        );
        assert_eq!(writer.avail(), 1);
    }
    assert_eq!(output, [0x6d, 0x9a]);
    // At EOF and at an overflowing-looking offset, neither buffer nor cursor changes.
    for offset in [24, usize::MAX] {
        let mut output = [0x9a; 2];
        {
            let mut writer = VmWriter::from(output.as_mut_slice()).to_fallible();
            assert_eq!(
                handle
                    .read_at(offset, &mut writer, StatusFlags::empty())
                    .unwrap(),
                0
            );
            assert_eq!(writer.avail(), 2);
        }
        assert_eq!(output, [0x9a; 2]);
    }
    let mut empty = [];
    let mut writer = VmWriter::from(empty.as_mut_slice()).to_fallible();
    assert_eq!(
        handle
            .read_at(usize::MAX, &mut writer, StatusFlags::empty())
            .unwrap(),
        0
    );
    assert_eq!(writer.avail(), 0);

    let input = [0xa5, 0xb6];
    let mut reader = VmReader::from(input.as_slice()).to_fallible();
    assert_eq!(
        handle
            .write_at(23, &mut reader, StatusFlags::empty())
            .unwrap(),
        1
    );
    assert_eq!(reader.remain(), 1);
    for offset in [24, usize::MAX] {
        let mut reader = VmReader::from(input.as_slice()).to_fallible();
        assert_eq!(
            handle
                .write_at(offset, &mut reader, StatusFlags::empty())
                .unwrap_err()
                .error(),
            Errno::ENOSPC
        );
        assert_eq!(reader.remain(), 2);
    }
    let mut reader = VmReader::from(&input[..0]).to_fallible();
    assert_eq!(
        handle
            .write_at(usize::MAX, &mut reader, StatusFlags::empty())
            .unwrap(),
        0
    );
    assert_eq!(reader.remain(), 0);

    let mut observed = [0; 32];
    mmio.read_bytes(0, &mut observed).unwrap();
    assert_eq!(&observed[..23], &[0x6d; 23]);
    assert_eq!(observed[23], 0xa5);
    assert_eq!(&observed[24..], &[0x6d; 8]);
    mmio.write_bytes(0, &saved).unwrap();
    mmio.read_bytes(0, &mut observed).unwrap();
    assert_eq!(observed, saved);
    println!("CYBERMANJU_FBDEV:fileops:PASS read_cases=4 write_cases=4 guard_bytes=8 cursors=true");
    println!(
        "CYBERMANJU_FBDEV:scope:PASS direct_fileops=true userspace_syscall=false mmap_tested=false fbdev_init_called=false"
    );
}
