use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, Sample, SampleFormat, SampleRate, SupportedStreamConfig};

pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
}

pub fn list_devices() -> Result<Vec<DeviceInfo>> {
    let host = cpal::default_host();
    let default_name = host.default_input_device().and_then(|d| d.name().ok());

    let mut devices = Vec::new();

    for device in host.devices().context("enumerating audio devices")? {
        if !can_capture(&device) {
            continue;
        }

        let name = device.name().unwrap_or_else(|_| "<unknown>".to_string());
        let is_default = Some(&name) == default_name.as_ref();

        devices.push(DeviceInfo { name, is_default });
    }

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
    pub fn open(
        filter: Option<&str>,
        target_node: Option<&str>,
        sample_rate: Option<u32>,
        fft_size: usize,
    ) -> Result<Self> {
        if let Some(node) = target_node {
            // The PipeWire ALSA plugin links its stream to this node instead
            // of the default source; pointing it at a sink captures the
            // sink's monitor. Ignored by every other backend.
            unsafe { std::env::set_var("PIPEWIRE_NODE", node) };
        }

        let host = cpal::default_host();

        let device = match filter {
            Some(filter) => find_device(&host, filter)?,
            None => host
                .default_input_device()
                .ok_or_else(|| anyhow!("no default capture device; try --list-devices"))?,
        };

        let name = device.name().unwrap_or_else(|_| "<unknown>".to_string());
        let supported = pick_config(&device, sample_rate)?;

        let ring = Arc::new(Mutex::new(Ring {
            samples: vec![0.0; fft_size],
            pos: 0,
        }));

        let stream = build_stream(&device, &supported, Arc::clone(&ring))?;
        stream.play().context("starting audio capture stream")?;

        Ok(Self {
            _stream: stream,
            ring,
            name,
            sample_rate: supported.sample_rate().0,
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

fn can_capture(device: &Device) -> bool {
    device
        .supported_input_configs()
        .is_ok_and(|mut configs| configs.next().is_some())
}

fn find_device(host: &Host, filter: &str) -> Result<Device> {
    let needle = filter.to_lowercase();

    host.devices()
        .context("enumerating audio devices")?
        .find(|device| {
            device
                .name()
                .is_ok_and(|name| name.to_lowercase().contains(&needle))
                && can_capture(device)
        })
        .ok_or_else(|| anyhow!("no capture device matching {filter:?}; try --list-devices"))
}

fn pick_config(device: &Device, sample_rate: Option<u32>) -> Result<SupportedStreamConfig> {
    let Some(rate) = sample_rate else {
        return device
            .default_input_config()
            .context("querying default input config");
    };

    device
        .supported_input_configs()
        .context("querying supported input configs")?
        .find(|range| range.min_sample_rate().0 <= rate && rate <= range.max_sample_rate().0)
        .map(|range| range.with_sample_rate(SampleRate(rate)))
        .ok_or_else(|| anyhow!("device does not support a sample rate of {rate} Hz"))
}

fn build_stream(
    device: &Device,
    supported: &SupportedStreamConfig,
    ring: Arc<Mutex<Ring>>,
) -> Result<cpal::Stream> {
    let config = supported.config();
    let channels = config.channels as usize;
    let err_fn = |err| eprintln!("audio stream error: {err}");

    // Downmix interleaved frames to mono and feed the ring.
    macro_rules! stream_as {
        ($sample:ty) => {
            device.build_input_stream(
                &config,
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

    let stream = match supported.sample_format() {
        SampleFormat::F32 => stream_as!(f32),
        SampleFormat::F64 => stream_as!(f64),
        SampleFormat::I16 => stream_as!(i16),
        SampleFormat::I32 => stream_as!(i32),
        SampleFormat::U16 => stream_as!(u16),
        SampleFormat::U8 => stream_as!(u8),
        other => bail!("unsupported sample format {other:?}"),
    };

    stream.context("building audio capture stream")
}
