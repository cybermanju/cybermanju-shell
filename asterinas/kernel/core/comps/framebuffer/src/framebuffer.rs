// SPDX-License-Identifier: MPL-2.0

use alloc::{sync::Arc, vec::Vec};
use core::ops::Range;

use ostd::{
    Error, Result,
    boot::boot_info,
    io::IoMem,
    mm::{CachePolicy, HasPaddr, HasSize, PAGE_SIZE, VmIo},
    sync::Mutex,
};
use spin::Once;

use crate::pixel::{Pixel, PixelFormat, RenderedPixel};

#[cfg(ktest)]
#[path = "runtime_tests.rs"]
mod runtime_tests;

/// Maximum number of colormap entries (standard 8-bit palette)
pub const MAX_CMAP_SIZE: usize = 256;

/// The framebuffer used for text or graphical output.
///
/// # Notes
///
/// It is highly recommended to use a synchronization primitive, such as a `SpinLock`, to
/// lock the framebuffer before performing any operation on it.
/// Failing to properly synchronize access can result in corrupted framebuffer content
/// or unspecified behavior during rendering.
#[derive(Debug)]
pub struct FrameBuffer {
    io_mem: IoMem,
    width: usize,
    height: usize,
    line_size: usize,
    byte_len: usize,
    pixel_format: PixelFormat,
    cmap: Mutex<FbCmap>,
}

/// A single entry in the color map with 16-bit color values.
///
/// Linux framebuffer colormap uses 16-bit values (0-65535) for each color channel
/// to support high precision color mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorMapEntry {
    /// Red color value (16-bit)
    pub red: u16,
    /// Green color value (16-bit)
    pub green: u16,
    /// Blue color value (16-bit)
    pub blue: u16,
    /// Transparency value (16-bit)
    pub transp: u16,
}

/// Internal framebuffer colormap structure.
#[derive(Clone, Debug)]
struct FbCmap {
    /// Color map entries
    entries: Vec<ColorMapEntry>,
}

pub static FRAMEBUFFER: Once<Arc<FrameBuffer>> = Once::new();

pub(crate) fn init() {
    let Some(framebuffer_arg) = boot_info().framebuffer_arg else {
        ostd::warn!("Framebuffer not found");
        return;
    };

    if framebuffer_arg.address == 0 {
        ostd::error!("Framebuffer address is zero");
        return;
    }

    // FIXME: There are several pixel formats that have the same BPP. We lost the information
    // during the boot phase, so here we guess the pixel format on a best effort basis.
    let pixel_format = match framebuffer_arg.bpp {
        8 => PixelFormat::Grayscale8,
        16 => PixelFormat::Rgb565,
        24 => PixelFormat::Rgb888,
        32 => PixelFormat::BgrReserved,
        _ => {
            ostd::error!(
                "Unsupported framebuffer pixel format: {} bpp",
                framebuffer_arg.bpp
            );
            return;
        }
    };

    let Some(byte_len) = framebuffer_arg.byte_len() else {
        ostd::error!("Invalid framebuffer geometry or pitch");
        return;
    };
    // Reserve/map the entire aligned cover, retaining the logical physical origin.
    let Some((cover, _)) = mmio_cover(framebuffer_arg.address, byte_len) else {
        ostd::error!("Framebuffer page cover overflow");
        return;
    };
    let framebuffer = {
        let line_size = framebuffer_arg.pitch;
        // Use write-combining for framebuffer to enable faster write operations.
        // Write-combining allows the CPU to combine multiple writes into fewer bus transactions,
        // which is ideal for framebuffer access patterns (sequential writes).
        let Ok(io_mem) = IoMem::acquire_with_cache_policy(cover, CachePolicy::WriteCombining)
        else {
            ostd::error!("Framebuffer MMIO range is unavailable");
            return;
        };

        let Ok(io_mem) = slice_surface_mapping(&io_mem, framebuffer_arg.address, byte_len) else {
            ostd::error!("Framebuffer MMIO cover does not match its surface");
            return;
        };

        let default_cmap = FbCmap {
            entries: Vec::new(),
        };

        FrameBuffer {
            io_mem,
            width: framebuffer_arg.width,
            height: framebuffer_arg.height,
            line_size,
            byte_len,
            pixel_format,
            cmap: Mutex::new(default_cmap),
        }
    };

    framebuffer.clear();
    FRAMEBUFFER.call_once(|| Arc::new(framebuffer));
}

// Zero, arithmetic overflow and an unrepresentable page end fail before acquisition.
fn mmio_cover(address: usize, byte_len: usize) -> Option<(Range<usize>, usize)> {
    if address == 0 || byte_len == 0 {
        return None;
    }
    let start = address & !(PAGE_SIZE - 1);
    let end = address.checked_add(byte_len)?.checked_add(PAGE_SIZE - 1)? & !(PAGE_SIZE - 1);
    Some((start..end, address - start))
}

// Use the same view construction in production and synthetic MMIO tests.
fn slice_surface_mapping(mapping: &IoMem, address: usize, byte_len: usize) -> Result<IoMem> {
    let (cover, offset) = mmio_cover(address, byte_len).ok_or(Error::InvalidArgs)?;
    if mapping.paddr() != cover.start || mapping.size() != cover.len() {
        return Err(Error::InvalidArgs);
    }
    Ok(mapping.slice(offset..cover.len()))
}

impl FrameBuffer {
    /// Returns the width of the framebuffer in pixels.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Returns the height of the framebuffer in pixels.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Returns the line size of the framebuffer in bytes.
    pub fn line_size(&self) -> usize {
        self.line_size
    }

    /// Declared surface bytes, including row padding, excluding mapping page tails.
    pub fn byte_len(&self) -> usize {
        self.byte_len
    }

    /// View starting exactly at the physical framebuffer origin and extending
    /// to the end of its page cover. It excludes leading bytes but may include
    /// a page tail. Prefer [`Self::surface_io_mem`] for byte I/O. A non-page-aligned
    /// origin is not suitable for page-granular mmap.
    pub fn io_mem(&self) -> &IoMem {
        &self.io_mem
    }

    /// Returns a byte-granular view limited to the declared surface.
    ///
    /// Use this for byte I/O, not page-granular mmap. It shares the existing
    /// mapping/reservation but does not expose the mapped page-tail bytes.
    pub fn surface_io_mem(&self) -> IoMem {
        self.io_mem.slice(0..self.byte_len)
    }

    /// Builds a validated synthetic subsurface over the real boot MMIO for
    /// cross-crate ktests in a disposable, single-CPU VM. Initializes the boot
    /// framebuffer if needed; never available in production or userspace.
    #[cfg(ktest)]
    pub fn test_subsurface(width: usize, height: usize, pitch: usize) -> Result<Self> {
        Self::test_subsurface_at(0, width, height, pitch)
    }

    /// Offset-origin synthetic MMIO view; no new acquisition or physical device mode.
    #[cfg(ktest)]
    pub fn test_subsurface_at(
        offset: usize,
        width: usize,
        height: usize,
        pitch: usize,
    ) -> Result<Self> {
        if FRAMEBUFFER.get().is_none() {
            init();
        }
        let parent = FRAMEBUFFER.get().ok_or(Error::AccessDenied)?;
        let row_len = width
            .checked_mul(parent.pixel_format.nbytes())
            .ok_or(Error::InvalidArgs)?;
        let byte_len = pitch.checked_mul(height).ok_or(Error::InvalidArgs)?;
        let end = offset.checked_add(byte_len).ok_or(Error::InvalidArgs)?;
        if width == 0 || height == 0 || pitch < row_len || end > parent.byte_len {
            return Err(Error::InvalidArgs);
        }
        let address = parent
            .io_mem
            .paddr()
            .checked_add(offset)
            .ok_or(Error::InvalidArgs)?;
        let (cover, _) = mmio_cover(address, byte_len).ok_or(Error::InvalidArgs)?;
        let start = cover
            .start
            .checked_sub(parent.io_mem.paddr())
            .ok_or(Error::InvalidArgs)?;
        let end = cover
            .end
            .checked_sub(parent.io_mem.paddr())
            .ok_or(Error::InvalidArgs)?;
        if end > parent.io_mem.size() {
            return Err(Error::InvalidArgs);
        }
        let mapping = parent.io_mem.slice(start..end);
        Ok(Self {
            io_mem: slice_surface_mapping(&mapping, address, byte_len)?,
            width,
            height,
            line_size: pitch,
            byte_len,
            pixel_format: parent.pixel_format,
            cmap: Mutex::new(FbCmap {
                entries: Vec::new(),
            }),
        })
    }

    /// Returns the pixel format of the framebuffer.
    pub fn pixel_format(&self) -> PixelFormat {
        self.pixel_format
    }

    /// Renders the pixel according to the pixel format of the framebuffer.
    pub fn render_pixel(&self, pixel: Pixel) -> RenderedPixel {
        pixel.render(self.pixel_format)
    }

    /// Calculates the offset of a pixel at the specified position.
    pub fn calc_offset(&self, x: usize, y: usize) -> PixelOffset<'_> {
        PixelOffset {
            fb: self,
            offset: (x * self.pixel_format.nbytes() + y * self.line_size) as isize,
        }
    }

    /// Writes a pixel at the specified position.
    pub fn write_pixel_at(&self, offset: PixelOffset, pixel: RenderedPixel) -> Result<()> {
        self.write_bytes_at(offset.as_usize(), pixel.as_slice())
    }

    /// Writes raw bytes within the declared surface, including row padding.
    ///
    /// Rejects overflow and out-of-surface writes before any I/O, even when the
    /// page-rounded MMIO mapping contains accessible bytes after the surface.
    /// An empty write is accepted at the exact end, but not beyond it.
    pub fn write_bytes_at(&self, offset: usize, bytes: &[u8]) -> Result<()> {
        if offset
            .checked_add(bytes.len())
            .is_none_or(|end| end > self.byte_len)
        {
            return Err(Error::InvalidArgs);
        }
        self.io_mem.write_bytes(offset, bytes)
    }

    /// Clears the framebuffer with default color (black).
    pub fn clear(&self) {
        // Avoid allocating a second complete surface during boot. The validated
        // byte length includes row padding but does not authorize page-tail I/O.
        let zeros = [0u8; PAGE_SIZE];
        for offset in (0..self.byte_len).step_by(PAGE_SIZE) {
            let len = core::cmp::min(PAGE_SIZE, self.byte_len - offset);
            self.write_bytes_at(offset, &zeros[..len]).unwrap();
        }
    }

    /// Sets color map entries starting from the given index.
    ///
    /// For efifb devices, hardware color map is not supported, so we maintain
    /// an in-memory map for software emulation.
    pub fn set_color_map(&self, start: usize, entries: &[ColorMapEntry]) -> Result<()> {
        if start > MAX_CMAP_SIZE || entries.len() > MAX_CMAP_SIZE - start {
            return Err(Error::InvalidArgs);
        }

        let mut cmap = self.cmap.lock();
        let required_len = start + entries.len();

        // Ensure the colormap has enough space
        if cmap.entries.len() < required_len {
            cmap.entries.resize(
                required_len,
                ColorMapEntry {
                    red: 0,
                    green: 0,
                    blue: 0,
                    transp: 0,
                },
            );
        }

        // Copy the entries
        cmap.entries[start..start + entries.len()].copy_from_slice(entries);

        Ok(())
    }

    /// Gets color map entries from the given range.
    pub fn get_color_map(&self, start: usize, len: usize) -> Option<Vec<ColorMapEntry>> {
        let cmap = self.cmap.lock();

        if start >= cmap.entries.len() || len > cmap.entries.len() - start {
            return None;
        }

        Some(cmap.entries[start..start + len].to_vec())
    }
}

/// The offset of a pixel in the framebuffer.
#[derive(Clone, Copy, Debug)]
pub struct PixelOffset<'a> {
    fb: &'a FrameBuffer,
    offset: isize,
}

impl PixelOffset<'_> {
    /// Adds the specified delta to the x coordinate.
    pub fn x_add(&mut self, x_delta: isize) {
        let delta = x_delta * self.fb.pixel_format.nbytes() as isize;
        self.offset += delta;
    }

    /// Adds the specified delta to the y coordinate.
    pub fn y_add(&mut self, y_delta: isize) {
        let delta = y_delta * self.fb.line_size as isize;
        self.offset += delta;
    }

    /// Returns the offset value as a `usize`.
    pub fn as_usize(&self) -> usize {
        self.offset as usize
    }
}
