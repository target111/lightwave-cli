use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use lightwave_core::api;

mod commands;

#[derive(Parser)]
#[command(name = "lightwave", version, about = "CLI for LightWave-Server")]
struct Cli {
    /// Server base URL (overrides LIGHTWAVE_URL)
    #[arg(long, global = true)]
    server: Option<String>,

    /// Emit machine-readable JSON instead of pretty output
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List all available presets
    Presets,
    /// Show info + args for a preset
    Info { preset: String },
    /// Show currently running preset
    Running,
    /// Start a preset. Pass --help after preset name for its args.
    #[command(disable_help_flag = true)]
    Start {
        preset: String,
        /// Effect-specific args, parsed dynamically from the server schema
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
    /// Stop the running preset
    Stop,
    /// Capture audio and stream its spectrum to the music visualizer
    #[cfg(feature = "music")]
    Music(commands::music::MusicArgs),
    /// Capture the screen and stream edge colors to the ambilight preset
    #[cfg(feature = "ambilight")]
    Ambilight(commands::ambilight::AmbilightArgs),
    /// Global brightness control
    Brightness { level: f32 },
    /// Color controls
    #[command(subcommand)]
    Color(ColorCmd),
}

#[derive(Subcommand)]
enum ColorCmd {
    /// Set a solid color (e.g. #FF0000 or red)
    Set { color: String },
    /// Clear (off)
    Clear,
}

fn main() -> Result<()> {
    let Cli { server, json, cmd } = Cli::parse();

    let base = server
        .or_else(|| std::env::var("LIGHTWAVE_URL").ok())
        .unwrap_or_else(|| "http://localhost:8080".to_string());

    let result = (|| -> Result<()> {
        let client = api::Client::new(&base)
            .with_context(|| format!("initializing LightWave client for {base}"))?;

        match cmd {
            Cmd::Presets => commands::presets::list(&client, json),
            Cmd::Info { preset } => commands::presets::info(&client, &preset, json),
            Cmd::Running => commands::presets::running(&client, json),
            Cmd::Start { preset, rest } => commands::start::run(&client, &preset, &rest, json),
            Cmd::Stop => commands::stop::run(&client, json),
            #[cfg(feature = "music")]
            Cmd::Music(args) => commands::music::run(&client, &args, json),
            #[cfg(feature = "ambilight")]
            Cmd::Ambilight(args) => commands::ambilight::run(&client, &args, json),
            Cmd::Brightness { level } => commands::leds::brightness(&client, level, json),
            Cmd::Color(ColorCmd::Set { color }) => commands::leds::set(&client, &color, json),
            Cmd::Color(ColorCmd::Clear) => commands::leds::clear(&client, json),
        }
    })();

    if let Err(err) = result {
        if json {
            commands::print_error_json(format!("{err:#}"))?;
            std::process::exit(1);
        }

        return Err(err);
    }

    Ok(())
}
