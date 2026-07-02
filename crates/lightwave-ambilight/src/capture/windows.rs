//! Windows backend: captures a monitor through the Windows.Graphics.Capture
//! API via the `windows-capture` crate.

use anyhow::{Result, anyhow};
use std::time::Duration;
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame as WgcFrame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
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
            Some(index) => Monitor::from_index(index)
                .map_err(|err| anyhow!("finding monitor {index}: {err}"))?,
            None => {
                Monitor::primary().map_err(|err| anyhow!("finding the primary monitor: {err}"))?
            }
        };

        let size = (
            monitor
                .width()
                .map_err(|err| anyhow!("querying monitor width: {err}"))?,
            monitor
                .height()
                .map_err(|err| anyhow!("querying monitor height: {err}"))?,
        );

        let settings = Settings::new(
            monitor,
            CursorCaptureSettings::WithoutCursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            // The sender paces UDP packets; this just keeps the compositor
            // from waking us at full refresh rate in between.
            MinimumUpdateIntervalSettings::Custom(Duration::from_secs(1) / options.max_fps),
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            Box::new(on_frame) as OnFrame,
        );

        let control = Handler::start_free_threaded(settings)
            .map_err(|err| anyhow!("starting Windows graphics capture: {err}"))?;

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
