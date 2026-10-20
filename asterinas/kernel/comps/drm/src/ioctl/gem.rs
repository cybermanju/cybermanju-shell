// SPDX-License-Identifier: MPL-2.0

use aster_core::prelude::*;
use ostd::mm::PAGE_SIZE;

use crate::{file::DrmFile, ioctl::ioctl_defs};

fn dumb_buffer_layout(width: u32, height: u32, bpp: u32) -> Result<(u32, u32)> {
    if width == 0 || height == 0 || bpp == 0 {
        return_errno_with_message!(
            Errno::EINVAL,
            "the dumb-buffer dimensions and bits per pixel must be nonzero"
        );
    }

    // Match the generic DRM dumb-buffer layout: round each pixel up to whole bytes
    // before calculating the row stride (not all optional packed-color layouts).
    if bpp > u32::MAX - 8 {
        return_errno_with_message!(Errno::EINVAL, "the dumb-buffer bpp overflows u32");
    }
    let bytes_per_pixel = bpp.div_ceil(8);
    let Some(pitch) = width.checked_mul(bytes_per_pixel) else {
        return_errno_with_message!(Errno::EINVAL, "the dumb-buffer pitch overflows u32");
    };

    // The page-aligned size must fit in u32 even though the UAPI exposes it as u64.
    let Some(size) = pitch
        .checked_mul(height)
        .and_then(|size| size.checked_next_multiple_of(PAGE_SIZE as u32))
    else {
        return_errno_with_message!(
            Errno::EINVAL,
            "the dumb-buffer size or page alignment overflows u32"
        );
    };

    Ok((pitch, size))
}

impl DrmFile {
    pub(super) fn drm_gem_close(&self, cmd: ioctl_defs::GemClose) -> Result<i32> {
        let args = cmd.read()?;
        self.remove_gem_object(args.handle)?;

        Ok(0)
    }

    pub(super) fn drm_mode_create_dumb(&self, cmd: ioctl_defs::ModeCreateDumb) -> Result<i32> {
        let mut args = cmd.read()?;
        let (pitch, size) = dumb_buffer_layout(args.width, args.height, args.bpp)?;

        let gem_ops = self.device().as_gem_ops().ok_or(Errno::EOPNOTSUPP)?;
        let gem_object = gem_ops.create_dumb(size as usize)?;

        args.pitch = pitch;
        args.size = gem_object.size() as u64;

        args.handle = self.add_gem_object(gem_object)?;
        if let Err(err) = cmd.write(&args) {
            let _ = self.remove_gem_object(args.handle);
            return Err(err);
        }

        Ok(0)
    }

    pub(super) fn drm_mode_map_dumb(&self, cmd: ioctl_defs::ModeMapDumb) -> Result<i32> {
        let mut args = cmd.read()?;
        args.offset = self.map_gem_handle(args.handle)?;
        cmd.write(&args)?;

        Ok(0)
    }

    pub(super) fn drm_mode_destroy_dumb(&self, cmd: ioctl_defs::ModeDestroyDumb) -> Result<i32> {
        let args = cmd.read()?;
        self.remove_gem_object(args.handle)?;

        Ok(0)
    }
}

#[cfg(ktest)]
mod tests {
    use ostd::prelude::*;

    use super::{PAGE_SIZE, dumb_buffer_layout};

    #[ktest]
    fn dumb_buffer_rows_round_pixel_size_up_before_multiplying_width() {
        for (bpp, expected_pitch) in [(15, 16), (30, 32), (24, 24), (32, 32)] {
            let (pitch, _) = dumb_buffer_layout(8, 1, bpp).unwrap();
            assert_eq!(pitch, expected_pitch, "bpp={bpp}");
        }
    }

    #[ktest]
    fn dumb_buffer_layout_rejects_zero_dimensions_and_bpp() {
        assert!(dumb_buffer_layout(0, 1, 8).is_err());
        assert!(dumb_buffer_layout(1, 0, 8).is_err());
        assert!(dumb_buffer_layout(1, 1, 0).is_err());
    }

    #[ktest]
    fn dumb_buffer_layout_rejects_pitch_and_size_overflow() {
        assert!(dumb_buffer_layout(u32::MAX, 1, 16).is_err());
        assert!(dumb_buffer_layout(2, u32::MAX, 8).is_err());
        assert!(dumb_buffer_layout(1, 1, u32::MAX - 7).is_err());
    }

    #[ktest]
    fn dumb_buffer_layout_rounds_page_size_and_checks_u32_boundary() {
        let (pitch, size) = dumb_buffer_layout(8, 1, 15).unwrap();
        assert_eq!(pitch, 16);
        assert_eq!(size, PAGE_SIZE as u32);

        let max_page_aligned_size = u32::MAX - (PAGE_SIZE as u32 - 1);
        let (_, size) = dumb_buffer_layout(1, max_page_aligned_size - 1, 8).unwrap();
        assert_eq!(size, max_page_aligned_size);
        assert!(dumb_buffer_layout(1, max_page_aligned_size + 1, 8).is_err());
    }
}

/// `struct drm_gem_close` in Linux.
///
/// Reference: <https://elixir.bootlin.com/linux/v6.17/source/include/uapi/drm/drm.h#L600-L605>.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod)]
pub(super) struct DrmGemClose {
    handle: u32,
    pad: u32,
}

/// `struct drm_mode_create_dumb` in Linux.
///
/// Reference: <https://elixir.bootlin.com/linux/v6.17/source/include/uapi/drm/drm_mode.h#L1057-L1079>.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod)]
pub(super) struct DrmModeCreateDumb {
    height: u32,
    width: u32,
    bpp: u32,
    flags: u32,
    handle: u32,
    pitch: u32,
    size: u64,
}

/// `struct drm_mode_map_dumb` in Linux.
///
/// Reference: <https://elixir.bootlin.com/linux/v6.17/source/include/uapi/drm/drm_mode.h#L1081-L1092>.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod)]
pub(super) struct DrmModeMapDumb {
    handle: u32,
    pad: u32,
    offset: u64,
}

/// `struct drm_mode_destroy_dumb` in Linux.
///
/// Reference: <https://elixir.bootlin.com/linux/v6.17/source/include/uapi/drm/drm_mode.h#L1094-L1096>.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod)]
pub(super) struct DrmModeDestroyDumb {
    handle: u32,
}
