//! Screen capture, behind a per-platform seam.
//!
//! Each backend exposes a `Capture` type with the same shape: open a
//! session, deliver `Frame`s to a callback on a backend-owned thread,
//! stop on drop. Code outside this module only ever sees `Frame`, so
//! adding a platform means writing one new backend module.

#[cfg(target_os = "linux")]
mod pipewire;
#[cfg(target_os = "linux")]
pub use pipewire::{Capture, CaptureOptions};

/// A borrowed view of one captured video frame.
pub struct Frame<'a> {
    pub width: usize,
    pub height: usize,
    /// Bytes per row; at least `width * format.bytes_per_pixel()`.
    pub stride: usize,
    pub format: PixelFormat,
    pub data: &'a [u8],
}

/// Pixel layouts the sampler understands; the alpha/padding byte of the
/// 4-byte formats is ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Bgrx,
    Rgbx,
    Bgra,
    Rgba,
    Bgr,
    Rgb,
}

impl PixelFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Bgr | Self::Rgb => 3,
            _ => 4,
        }
    }

    /// Byte offsets of the (R, G, B) channels within one pixel.
    pub fn rgb_offsets(self) -> (usize, usize, usize) {
        match self {
            Self::Bgrx | Self::Bgra | Self::Bgr => (2, 1, 0),
            Self::Rgbx | Self::Rgba | Self::Rgb => (0, 1, 2),
        }
    }
}
