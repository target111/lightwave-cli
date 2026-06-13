use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use serde_json::json;

use lightwave_ambilight::{Config, Edge, Streamer};
use lightwave_core::Client;

#[derive(clap::Args)]
pub struct AmbilightArgs {
    /// Averaged color boxes sent per packet
    #[arg(long, default_value_t = 16)]
    boxes: usize,

    /// Screen edge the strip mirrors: bottom, top, left or right
    #[arg(long, default_value = "bottom")]
    edge: Edge,

    /// Fraction of the screen the sampled band reaches inward from the edge
    #[arg(long, default_value_t = 0.2)]
    depth: f32,

    /// How strongly vivid pixels outweigh dull ones (0 = plain average)
    #[arg(long, default_value_t = 1.0)]
    vividness: f32,

    /// Brightness gamma matching the strip to the screen (1 = raw values)
    #[arg(long, default_value_t = 2.2)]
    gamma: f32,

    /// Lift box colors to at least this saturation; amplifies an existing
    /// tint only, pure grey stays grey (0 = off)
    #[arg(long, default_value_t = 0.0)]
    min_saturation: f32,

    /// Send boxes in reverse order (strip runs against screen direction)
    #[arg(long)]
    reverse: bool,

    /// Color packets sent per second (also caps the capture framerate)
    #[arg(long, default_value_t = 30)]
    fps: u32,

    /// Show the screen picker again instead of reusing the saved permission
    #[arg(long)]
    reselect: bool,

    /// UDP port the ambilight preset listens on
    #[arg(long, default_value_t = 5556)]
    port: u16,

    /// Name of the ambilight preset on the server
    #[arg(long, default_value = "Ambilight")]
    preset: String,

    /// Stream UDP only; don't start/stop the preset (assume it's running)
    #[arg(long)]
    no_start: bool,
}

pub fn run(client: &Client, args: &AmbilightArgs, json_mode: bool) -> Result<()> {
    let target = format!("{}:{}", client.host(), args.port);

    let config = Config {
        boxes: args.boxes,
        edge: args.edge,
        depth: args.depth,
        vividness: args.vividness,
        gamma: args.gamma,
        min_saturation: args.min_saturation,
        reverse: args.reverse,
        fps: args.fps,
        reselect: args.reselect,
        target: target.clone(),
    };

    let streamer = Streamer::new(&config)?;
    let (width, height) = streamer.size();

    if !args.no_start {
        client
            .start(&args.preset, &json!({ "port": args.port }))
            .with_context(|| format!("starting preset {}", args.preset))?;
    }

    if json_mode {
        // First line on stdout confirms the capture and socket are up and the
        // preset was started; streaming begins right after. Consumers can block
        // on it to know initialization succeeded.
        crate::commands::print_json(&json!({
            "event": "start",
            "preset": args.preset,
            "width": width,
            "height": height,
            "target": target,
            "boxes": args.boxes,
            "edge": args.edge.to_string(),
            "fps": args.fps,
        }))?;
    } else {
        println!(
            "\n  {} {}  {}",
            "▦".bright_cyan(),
            format!("{width}×{height}").bright_white().bold(),
            format!("→ udp://{target}").dimmed()
        );
        println!(
            "  {} {} edge · {} boxes · depth {} · vividness {} · gamma {} · {} fps",
            "›".dimmed(),
            args.edge,
            args.boxes,
            args.depth,
            args.vividness,
            args.gamma,
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
