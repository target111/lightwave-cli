//! Windows backend: captures a monitor through the Windows.Graphics.Capture
//! API via the `windows-capture` crate.

use anyhow::{Context as _, Result};
use std::time::Duration;
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame as WgcFrame;
use windows_capture::graphics_capture_api::{GraphicsCaptureApi, InternalCaptureControl};
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

use super::{CaptureOptions, Frame, PixelFormat};

type OnFrame = Box<dyn FnMut(Frame<'_>) + Send>;
type HandlerError = Box<dyn std::error::Error + Send + Sync>;

/// A running screen capture; frames flow to the callback passed to
/// [`Capture::open`] from a capture-owned thread until drop.
pub struct Capture {
    control: Option<CaptureControl<Handler, HandlerError>>,
    size: (u32, u32),
}

impl Capture {
    pub fn open<F>(options: &CaptureOptions, on_frame: F) -> Result<Self>
    where
        F: FnMut(Frame<'_>) + Send + 'static,
    {
        let monitor = match options.monitor {
            Some(index) => {
                Monitor::from_index(index).with_context(|| format!("finding monitor {index}"))?
            }
            None => Monitor::primary().context("finding the primary monitor")?,
        };

        let size = (
            monitor.width().context("querying monitor width")?,
            monitor.height().context("querying monitor height")?,
        );

        // The session toggles below are version-gated WinRT properties;
        // asking for one on a Windows build that lacks it fails the whole
        // capture, so fall back to the system default where unsupported.
        let cursor = if GraphicsCaptureApi::is_cursor_settings_supported().unwrap_or(false) {
            CursorCaptureSettings::WithoutCursor
        } else {
            CursorCaptureSettings::Default
        };

        let border = if GraphicsCaptureApi::is_border_settings_supported().unwrap_or(false) {
            DrawBorderSettings::WithoutBorder
        } else {
            eprintln!("note: this Windows build always draws a border around captured screens");
            DrawBorderSettings::Default
        };

        // The sender paces UDP packets; this just keeps the compositor
        // from waking us at full refresh rate in between.
        let min_update =
            if GraphicsCaptureApi::is_minimum_update_interval_supported().unwrap_or(false) {
                MinimumUpdateIntervalSettings::Custom(Duration::from_secs(1) / options.max_fps)
            } else {
                MinimumUpdateIntervalSettings::Default
            };

        let settings = Settings::new(
            monitor,
            cursor,
            border,
            SecondaryWindowSettings::Default,
            min_update,
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            Box::new(on_frame) as OnFrame,
        );

        let control =
            Handler::start_free_threaded(settings).context("starting Windows graphics capture")?;

        Ok(Self {
            control: Some(control),
            size,
        })
    }

    /// Captured monitor size in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(control) = self.control.take()
            && let Err(err) = control.stop()
        {
            eprintln!("warning: failed to stop capture cleanly: {err}");
        }
    }
}

struct Handler {
    on_frame: OnFrame,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = OnFrame;
    type Error = HandlerError;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            on_frame: ctx.flags,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut WgcFrame,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let mut buffer = frame.buffer()?;
        let width = buffer.width() as usize;
        let height = buffer.height() as usize;
        let stride = buffer.row_pitch() as usize;

        (self.on_frame)(Frame {
            width,
            height,
            stride,
            format: PixelFormat::Bgra,
            data: buffer.as_raw_buffer(),
        });

        Ok(())
    }
}
