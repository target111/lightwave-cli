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
    /// Color & brightness controls
    #[command(subcommand)]
    Color(ColorCmd),
}

#[derive(Subcommand)]
enum ColorCmd {
    /// Set a solid color (e.g. #FF0000 or red)
    Set { color: String },
    /// Set brightness 0.0 - 1.0
    Brightness { level: f32 },
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
            Cmd::Color(ColorCmd::Set { color }) => commands::color::set(&client, &color, json),
            Cmd::Color(ColorCmd::Brightness { level }) => {
                commands::color::brightness(&client, level, json)
            }
            Cmd::Color(ColorCmd::Clear) => commands::color::clear(&client, json),
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
