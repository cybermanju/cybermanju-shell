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
    // Exercise mmap admission only, not Mappable::map, VMAR or a syscall.
    cybermanju_fbdev_unaligned_fileops_runtime();
    println!(
        "CYBERMANJU_FBDEV:scope:PASS direct_fileops=true userspace_syscall=false mmap_tested=false fbdev_init_called=false"
    );
}

fn cybermanju_fbdev_unaligned_fileops_runtime() {
    let arg = ostd::boot::boot_info()
        .framebuffer_arg
        .expect("real RGB boot framebuffer required");
    assert_eq!(arg.bpp, 32);
    let offset = PAGE_SIZE - 3;
    let surface = FrameBuffer::test_subsurface_at(offset, 2, 2, 12).unwrap();
    assert_eq!(surface.byte_len(), 24);
    assert_eq!(
        surface.io_mem().paddr() as usize,
        FRAMEBUFFER.get().unwrap().io_mem().paddr() as usize + offset
    );
    let handle = FbHandle {
        framebuffer: Arc::new(surface),
    };
    let info = handle.collect_fix_screen_info();
    assert_eq!(
        info.smem_start as usize,
        handle.framebuffer.io_mem().paddr() as usize
    );
    assert_eq!(info.smem_len, 24);

    let parent = FRAMEBUFFER.get().unwrap().io_mem();
    let mut saved = [0; 40];
    parent.read_bytes(offset - 8, &mut saved).unwrap();
    parent.write_bytes(offset - 8, &[0x5c; 40]).unwrap();

    // Four bytes beginning three bytes before the page boundary cross it.
    let input = [0x11, 0x22, 0x33, 0x44];
    let mut reader = VmReader::from(input.as_slice()).to_fallible();
    assert_eq!(
        handle
            .write_at(0, &mut reader, StatusFlags::empty())
            .unwrap(),
        4
    );
    assert_eq!(reader.remain(), 0);
    let mut output = [0; 4];
    let mut writer = VmWriter::from(output.as_mut_slice()).to_fallible();
    assert_eq!(
        handle
            .read_at(0, &mut writer, StatusFlags::empty())
            .unwrap(),
        4
    );
    assert_eq!(output, input);

    // EOF reads and writes are truncated without touching the trailing sentinel.
    let tail = [0xa1, 0xb2, 0xc3, 0xd4];
    let mut reader = VmReader::from(tail.as_slice()).to_fallible();
    assert_eq!(
        handle
            .write_at(22, &mut reader, StatusFlags::empty())
            .unwrap(),
        2
    );
    assert_eq!(reader.remain(), 2);
    let mut output = [0xee; 4];
    let mut writer = VmWriter::from(output.as_mut_slice()).to_fallible();
    assert_eq!(
        handle
            .read_at(22, &mut writer, StatusFlags::empty())
            .unwrap(),
        2
    );
    assert_eq!(output, [0xa1, 0xb2, 0xee, 0xee]);

    let request = FileMmapRequest::new(true);
    assert_eq!(
        handle
            .mappable(request)
            .err()
            .expect("unaligned mmap must be rejected")
            .error(),
        Errno::EINVAL
    );

    let mut observed_guards = [0; 40];
    parent.read_bytes(offset - 8, &mut observed_guards).unwrap();
    assert_eq!(&observed_guards[..8], &[0x5c; 8]);
    assert_eq!(&observed_guards[32..], &[0x5c; 8]);
    parent.write_bytes(offset - 8, &saved).unwrap();
    let mut restored = [0; 40];
    parent.read_bytes(offset - 8, &mut restored).unwrap();
    assert_eq!(restored, saved);
    println!("CYBERMANJU_FBDEV:unaligned_fileops:PASS cases=4 bytes=24 mmap_rejected=true");
}
