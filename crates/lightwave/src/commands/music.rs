use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use serde_json::json;

use lightwave_core::Client;
use lightwave_music::{Config, Streamer};

#[derive(clap::Args)]
pub struct MusicArgs {
    /// Capture device (case-insensitive substring match)
    #[arg(long)]
    device: Option<String>,

    /// List capture devices and exit
    #[arg(long)]
    list_devices: bool,

    /// FFT window size in samples (power of two)
    #[arg(long, default_value_t = 2048)]
    fft_size: usize,

    /// Number of frequency bins sent per packet
    #[arg(long, default_value_t = 32)]
    bins: usize,

    /// Linear gain applied to bin magnitudes
    #[arg(long, default_value_t = 4.0)]
    gain: f32,

    /// Capture sample rate in Hz [default: device preference]
    #[arg(long)]
    sample_rate: Option<u32>,

    /// Spectrum packets sent per second
    #[arg(long, default_value_t = 60)]
    fps: u32,

    /// Lowest analyzed frequency in Hz
    #[arg(long, default_value_t = 40.0)]
    min_freq: f32,

    /// Highest analyzed frequency in Hz
    #[arg(long, default_value_t = 16000.0)]
    max_freq: f32,

    /// UDP port the visualizer preset listens on
    #[arg(long, default_value_t = 5555)]
    port: u16,

    /// Name of the visualizer preset on the server
    #[arg(long, default_value = "MusicVisualizer")]
    preset: String,

    /// Stream UDP only; don't start/stop the preset (assume it's running)
    #[arg(long)]
    no_start: bool,
}

pub fn run(client: &Client, args: &MusicArgs, json_mode: bool) -> Result<()> {
    if args.list_devices {
        return list_devices(json_mode);
    }

    let target = format!("{}:{}", client.host(), args.port);

    let config = Config {
        device: args.device.clone(),
        sample_rate: args.sample_rate,
        fft_size: args.fft_size,
        bins: args.bins,
        gain: args.gain,
        min_freq: args.min_freq,
        max_freq: args.max_freq,
        fps: args.fps,
        target: target.clone(),
    };

    let streamer = Streamer::new(&config)?;

    if !args.no_start {
        client
            .start(&args.preset, &json!({ "port": args.port }))
            .with_context(|| format!("starting preset {}", args.preset))?;
    }

    if json_mode {
        // First line on stdout confirms packets are flowing; consumers can
        // block on it to know the stream came up.
        crate::commands::print_json(&json!({
            "event": "start",
            "preset": args.preset,
            "device": streamer.device_name(),
            "sample_rate": streamer.sample_rate(),
            "target": target,
            "fft_size": args.fft_size,
            "bins": args.bins,
            "fps": args.fps,
        }))?;
    } else {
        println!(
            "\n  {} {}  {}",
            "♪".bright_magenta(),
            streamer.device_name().bright_white().bold(),
            format!("→ udp://{target}").dimmed()
        );
        println!(
            "  {} {} Hz · fft {} · {} bins · gain {} · {} fps",
            "›".dimmed(),
            streamer.sample_rate(),
            args.fft_size,
            args.bins,
            args.gain,
            args.fps
        );
        println!(
            "  {} streaming, press {} to stop\n",
            "▶".bright_green(),
            "Ctrl+C".bright_yellow().bold()
        );
    }

    let result = streamer.run();

    if !args.no_start
        && let Err(err) = client.stop()
    {
        eprintln!("warning: failed to stop preset: {err:#}");
    }

    result?;

    if json_mode {
        // Errors skip this: they surface as the final {"ok":false,...} line.
        crate::commands::print_json(&json!({
            "event": "stop",
            "reason": "interrupt",
        }))?;
    } else {
        println!("  {} stopped", "■".bright_red());
    }

    Ok(())
}

fn list_devices(json_mode: bool) -> Result<()> {
    let devices = lightwave_music::list_devices()?;

    if json_mode {
        let devices = devices
            .iter()
            .map(|name| json!({ "name": name }))
            .collect::<Vec<_>>();

        return crate::commands::print_json(&json!({
            "ok": true,
            "devices": devices,
        }));
    }

    if devices.is_empty() {
        println!("  {} no capture devices found", "✗".red());
        return Ok(());
    }

    println!(
        "\n  {} {} capture device{}\n",
        "●".green(),
        devices.len().bold(),
        if devices.len() == 1 { "" } else { "s" }
    );

    for name in &devices {
        println!("  {}  {}", "▸".bright_magenta(), name);
    }

    println!();

    Ok(())
}
