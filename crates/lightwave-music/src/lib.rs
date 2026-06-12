mod capture;
mod dsp;

use std::net::UdpSocket;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::time::MissedTickBehavior;

pub use capture::{DeviceInfo, list_devices};

pub struct Config {
    /// Case-insensitive substring match on device names; None = default input.
    pub device: Option<String>,
    /// PipeWire node id or name to capture from (Linux/PipeWire only);
    /// a sink node captures its monitor, i.e. whatever is playing.
    pub target_node: Option<String>,
    /// Capture sample rate in Hz; None = device preference.
    pub sample_rate: Option<u32>,
    /// FFT window size in samples (power of two).
    pub fft_size: usize,
    /// Frequency bins per UDP packet.
    pub bins: usize,
    /// Linear gain applied to bin magnitudes before clamping to 0..=1.
    pub gain: f32,
    /// Analyzed frequency range in Hz.
    pub min_freq: f32,
    pub max_freq: f32,
    /// UDP packets per second.
    pub fps: u32,
    /// UDP target, e.g. "192.168.1.20:5555".
    pub target: String,
}

impl Config {
    fn validate(&self) -> Result<()> {
        if !self.fft_size.is_power_of_two() || self.fft_size < 64 {
            bail!(
                "fft-size must be a power of two >= 64, got {}",
                self.fft_size
            );
        }

        if self.bins == 0 {
            bail!("bins must be at least 1");
        }

        if self.gain <= 0.0 {
            bail!("gain must be positive, got {}", self.gain);
        }

        if self.fps == 0 {
            bail!("fps must be at least 1");
        }

        if self.min_freq <= 0.0 {
            bail!("min-freq must be positive, got {}", self.min_freq);
        }

        Ok(())
    }
}

/// Captures audio, runs the FFT, and streams binned spectra over UDP
/// in the visualizer's packet format (packed little-endian f32, 0..=1).
pub struct Streamer {
    capture: capture::Capture,
    analyzer: dsp::Analyzer,
    socket: UdpSocket,
    samples: Vec<f32>,
    packet: Vec<u8>,
    period: Duration,
}

impl Streamer {
    pub fn new(config: &Config) -> Result<Self> {
        config.validate()?;

        let capture = capture::Capture::open(
            config.device.as_deref(),
            config.target_node.as_deref(),
            config.sample_rate,
            config.fft_size,
        )?;

        let analyzer = dsp::Analyzer::new(
            config.fft_size,
            capture.sample_rate(),
            config.bins,
            config.min_freq,
            config.max_freq,
            config.gain,
        )?;

        let socket = lightwave_core::net::connect_udp(&config.target)?;

        Ok(Self {
            capture,
            analyzer,
            socket,
            samples: vec![0.0; config.fft_size],
            packet: Vec::with_capacity(config.bins * 4),
            period: Duration::from_secs(1) / config.fps,
        })
    }

    pub fn device_name(&self) -> &str {
        self.capture.name()
    }

    pub fn sample_rate(&self) -> u32 {
        self.capture.sample_rate()
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
        self.capture.snapshot(&mut self.samples);
        let bins = self.analyzer.analyze(&self.samples);

        self.packet.clear();
        for &bin in bins {
            self.packet.extend_from_slice(&bin.to_le_bytes());
        }

        lightwave_core::net::send_packet(&self.socket, &self.packet)
    }
}
