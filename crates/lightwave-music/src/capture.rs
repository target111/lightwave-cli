use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, HostId, Sample, SampleFormat, StreamConfig};

pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
}

pub fn list_devices() -> Result<Vec<DeviceInfo>> {
    let host = cpal::default_host();
    let default_name = host.default_input_device().map(|d| device_name(&d));

    // Under the PipeWire backend, output sinks are exposed as input-capable
    // devices, so picking one here captures its monitor (whatever is playing).
    let devices = host
        .input_devices()
        .context("enumerating audio devices")?
        .map(|device| {
            let name = device_name(&device);
            let is_default = Some(&name) == default_name.as_ref();
            DeviceInfo { name, is_default }
        })
        .collect();

    Ok(devices)
}

/// Mono samples from a capture device, kept in a ring holding the most
/// recent `fft_size` of them.
pub struct Capture {
    // Dropping the stream stops the capture callbacks.
    _stream: cpal::Stream,
    ring: Arc<Mutex<Ring>>,
    name: String,
    sample_rate: u32,
}

impl Capture {
    pub fn open(filter: Option<&str>, sample_rate: Option<u32>, fft_size: usize) -> Result<Self> {
        let host = cpal::default_host();

        let device = match filter {
            Some(filter) => find_device(&host, filter)?,
            None => default_device(&host)?,
        };

        let name = device_name(&device);
        let (config, format) = pick_config(&device, sample_rate)?;

        let ring = Arc::new(Mutex::new(Ring {
            samples: vec![0.0; fft_size],
            pos: 0,
        }));

        let stream = build_stream(&device, &config, format, Arc::clone(&ring))?;
        stream.play().context("starting audio capture stream")?;

        Ok(Self {
            _stream: stream,
            ring,
            name,
            sample_rate: config.sample_rate,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn snapshot(&self, out: &mut [f32]) {
        let ring = self.ring.lock().unwrap();
        let (newer, older) = ring.samples.split_at(ring.pos);

        out[..older.len()].copy_from_slice(older);
        out[older.len()..].copy_from_slice(newer);
    }
}

struct Ring {
    samples: Vec<f32>,
    pos: usize,
}

impl Ring {
    fn push(&mut self, sample: f32) {
        self.samples[self.pos] = sample;
        self.pos = (self.pos + 1) % self.samples.len();
    }
}

fn device_name(device: &Device) -> String {
    device
        .description()
        .map(|desc| desc.name().to_string())
        .unwrap_or_else(|_| "<unknown>".to_string())
}

/// The default capture target. On PipeWire that's the default output sink,
/// whose monitor cpal captures when opened as input — so the visualizer
/// follows whatever is playing out of the box. Other backends have no such
/// loopback, so they fall back to the default input (microphone/line-in).
fn default_device(host: &Host) -> Result<Device> {
    if host.id() == HostId::PipeWire
        && let Some(sink) = host.default_output_device()
    {
        return Ok(sink);
    }

    host.default_input_device()
        .ok_or_else(|| anyhow!("no default capture device; try --list-devices"))
}

fn find_device(host: &Host, filter: &str) -> Result<Device> {
    let needle = filter.to_lowercase();

    host.input_devices()
        .context("enumerating audio devices")?
        .find(|device| device_name(device).to_lowercase().contains(&needle))
        .ok_or_else(|| anyhow!("no capture device matching {filter:?}; try --list-devices"))
}

/// Resolves the stream config, keeping the device's preferred sample format
/// (so PipeWire hands us float frames) while honoring a sample-rate override.
/// A sink captured as input reports its config under the output direction,
/// so fall back to that when the device exposes no input config of its own.
fn pick_config(device: &Device, sample_rate: Option<u32>) -> Result<(StreamConfig, SampleFormat)> {
    let default = device
        .default_input_config()
        .or_else(|_| device.default_output_config())
        .context("querying default device config")?;

    let format = default.sample_format();
    let mut config = default.config();

    if let Some(rate) = sample_rate {
        let input = device.supported_input_configs().into_iter().flatten();
        let output = device.supported_output_configs().into_iter().flatten();

        if !input.chain(output).any(|range| range.contains_rate(rate)) {
            bail!("device does not support a sample rate of {rate} Hz");
        }

        config.sample_rate = rate;
    }

    Ok((config, format))
}

fn build_stream(
    device: &Device,
    config: &StreamConfig,
    format: SampleFormat,
    ring: Arc<Mutex<Ring>>,
) -> Result<cpal::Stream> {
    let channels = config.channels as usize;
    let err_fn = |err| eprintln!("audio stream error: {err}");

    // Downmix interleaved frames to mono and feed the ring.
    macro_rules! stream_as {
        ($sample:ty) => {
            device.build_input_stream(
                *config,
                move |data: &[$sample], _: &cpal::InputCallbackInfo| {
                    let mut ring = ring.lock().unwrap();
                    for frame in data.chunks_exact(channels) {
                        let sum: f32 = frame.iter().map(|&s| f32::from_sample(s)).sum();
                        ring.push(sum / channels as f32);
                    }
                },
                err_fn,
                None,
            )
        };
    }

    let stream = match format {
        SampleFormat::F32 => stream_as!(f32),
        SampleFormat::F64 => stream_as!(f64),
        SampleFormat::I8 => stream_as!(i8),
        SampleFormat::I16 => stream_as!(i16),
        SampleFormat::I24 => stream_as!(cpal::I24),
        SampleFormat::I32 => stream_as!(i32),
        SampleFormat::U8 => stream_as!(u8),
        SampleFormat::U16 => stream_as!(u16),
        SampleFormat::U24 => stream_as!(cpal::U24),
        SampleFormat::U32 => stream_as!(u32),
        other => bail!("unsupported sample format {other:?}"),
    };

    stream.context("building audio capture stream")
}
