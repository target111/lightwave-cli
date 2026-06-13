//! Screen-capture client for the ambilight preset: grabs frames via the
//! desktop portal, reduces each one to per-box average colors, and
//! streams the boxes over UDP in the preset's packet format (packed
//! little-endian f32 RGB triplets, 0..=1).

#[cfg(not(target_os = "linux"))]
compile_error!(
    "lightwave-ambilight only supports Linux (xdg-desktop-portal + PipeWire); \
     build without the `ambilight` feature on other platforms"
);

mod capture;
mod sampler;

use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::time::MissedTickBehavior;

use capture::{Capture, CaptureOptions};
pub use sampler::Edge;
use sampler::Sampler;

pub struct Config {
    /// Averaged color boxes per packet.
    pub boxes: usize,
    /// Screen edge the LED strip mirrors.
    pub edge: Edge,
    /// Fraction of the screen (0, 1] the sampled band reaches inward.
    pub depth: f32,
    /// How strongly vivid pixels outweigh dull ones (0 = plain mean).
    pub vividness: f32,
    /// Brightness gamma applied to outgoing colors (hue-preserving).
    /// The strip renders dark values far brighter than the monitor's
    /// steep sRGB curve; without this (~2.2) dark scenes glow.
    pub gamma: f32,
    /// Lift box colors to at least this saturation (0 = off). Amplifies
    /// an existing tint only; pure grey stays grey.
    pub min_saturation: f32,
    /// Send boxes in reverse order (strip runs against screen direction).
    pub reverse: bool,
    /// UDP packets per second; also caps the negotiated capture rate.
    pub fps: u32,
    /// Ignore the saved portal permission and show the picker again.
    pub reselect: bool,
    /// UDP target, e.g. "192.168.1.20:5556".
    pub target: String,
}

impl Config {
    fn validate(&self) -> Result<()> {
        if self.boxes == 0 {
            bail!("boxes must be at least 1");
        }

        if !(self.depth > 0.0 && self.depth <= 1.0) {
            bail!("depth must be in (0, 1], got {}", self.depth);
        }

        if !self.vividness.is_finite() || self.vividness < 0.0 {
            bail!(
                "vividness must be a finite number >= 0, got {}",
                self.vividness
            );
        }

        if !self.gamma.is_finite() || self.gamma <= 0.0 {
            bail!("gamma must be a finite positive number, got {}", self.gamma);
        }

        if !(0.0..=1.0).contains(&self.min_saturation) {
            bail!(
                "min-saturation must be in 0..=1, got {}",
                self.min_saturation
            );
        }

        if self.fps == 0 {
            bail!("fps must be at least 1");
        }

        Ok(())
    }
}

/// Captures the screen and streams box colors over UDP at a fixed rate.
///
/// Sending is paced by a timer rather than by frame arrival: compositors
/// only deliver frames on damage, so a static screen produces none, and
/// the preset would otherwise fall into its idle animation. The latest
/// colors are simply repeated until something changes.
pub struct Streamer {
    capture: Capture,
    colors: Arc<Mutex<Option<Vec<[f32; 3]>>>>,
    socket: UdpSocket,
    packet: Vec<u8>,
    period: Duration,
}

impl Streamer {
    pub fn new(config: &Config) -> Result<Self> {
        config.validate()?;

        let sampler = Sampler::new(
            config.boxes,
            config.edge,
            config.depth,
            config.vividness,
            config.gamma,
            config.min_saturation,
            config.reverse,
        );

        let colors = Arc::new(Mutex::new(None));
        let slot = Arc::clone(&colors);
        let capture = Capture::open(
            &CaptureOptions {
                max_fps: config.fps,
                reselect: config.reselect,
            },
            move |frame| {
                let boxes = sampler.sample(&frame);
                if !boxes.is_empty() {
                    *slot.lock().unwrap() = Some(boxes);
                }
            },
        )?;

        let socket = lightwave_core::net::connect_udp(&config.target)?;

        Ok(Self {
            capture,
            colors,
            socket,
            packet: Vec::with_capacity(config.boxes * 12),
            period: Duration::from_secs(1) / config.fps,
        })
    }

    /// Captured stream size in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.capture.size()
    }

    /// Stream packets until Ctrl+C.
    pub fn run(mut self) -> Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("building tokio runtime")?;

        runtime.block_on(async {
            let mut ticker = tokio::time::interval(self.period);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

            let ctrl_c = tokio::signal::ctrl_c();
            tokio::pin!(ctrl_c);

            loop {
                tokio::select! {
                    result = &mut ctrl_c => {
                        return result.context("waiting for Ctrl+C");
                    }
                    _ = ticker.tick() => self.send_frame()?,
                }
            }
        })
    }

    fn send_frame(&mut self) -> Result<()> {
        self.packet.clear();

        {
            let colors = self.colors.lock().unwrap();
            // Nothing to show until the first frame arrives.
            let Some(colors) = colors.as_ref() else {
                return Ok(());
            };

            for color in colors {
                for channel in color {
                    self.packet.extend_from_slice(&channel.to_le_bytes());
                }
            }
        }

        lightwave_core::net::send_packet(&self.socket, &self.packet)
    }
}
