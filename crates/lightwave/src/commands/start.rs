use anyhow::{Context, Result, anyhow, bail};
use clap::{Arg, ArgAction, Command};
use owo_colors::OwoColorize;
use serde_json::{Value, json};

use lightwave_core::{
    ArgSchema, Client,
    color::{normalize, parse_hex_rgb},
};

pub fn run(client: &Client, preset: &str, rest: &[String], json_mode: bool) -> Result<()> {
    let info = client
        .preset_info(preset)
        .with_context(|| format!("fetching schema for {preset}"))?;

    // clap stores arg/command identifiers as &'static str; leak the dynamic strings
    let preset_name: &'static str = preset.to_string().leak();
    let about: &'static str = info.description.clone().leak();

    let mut cmd = Command::new(preset_name)
        .no_binary_name(true)
        .about(about)
        .disable_help_subcommand(true)
        .styles(
            clap::builder::Styles::styled()
                .header(anstyle::Style::new().bold().underline())
                .literal(anstyle::AnsiColor::BrightCyan.on_default())
                .placeholder(anstyle::AnsiColor::BrightYellow.on_default()),
        );

    for arg in &info.args {
        cmd = cmd.arg(build_arg(arg)?);
    }

    let matches = match cmd.try_get_matches_from(rest) {
        Ok(matches) => matches,
        Err(err) => {
            let code = if err.use_stderr() { 1 } else { 0 };

            if json_mode && err.use_stderr() {
                crate::commands::print_arg_error_json(err.to_string())?;
            } else {
                err.print().context("printing argument parser message")?;
            }

            std::process::exit(code);
        }
    };

    // Only include flags the user actually set, so the server falls back to its own defaults.
    let mut payload = serde_json::Map::new();

    for arg in &info.args {
        if matches.value_source(&arg.name) == Some(clap::parser::ValueSource::CommandLine) {
            let raw = matches
                .get_one::<String>(&arg.name)
                .ok_or_else(|| anyhow!("missing value for --{}", arg.name))?;

            payload.insert(arg.name.clone(), coerce(&arg.arg_type, raw)?);
        }
    }

    let args = Value::Object(payload);

    client.start(preset, &args)?;

    if json_mode {
        crate::commands::print_ok_json(serde_json::json!({
            "action": "start",
            "preset": preset,
            "args": args,
        }))?;
    } else {
        println!(
            "  {} started {}",
            "▶".bright_green(),
            preset.bright_white().bold()
        );
    }

    Ok(())
}

fn build_arg(arg: &ArgSchema) -> Result<Arg> {
    if arg.name.is_empty() {
        bail!("preset argument name cannot be empty");
    }

    if arg.name.starts_with('-') {
        bail!(
            "invalid preset argument name {:?}: must not start with '-'",
            arg.name
        );
    }

    if arg.name == "help" {
        bail!(
            "invalid preset argument name {:?}: name is reserved",
            arg.name
        );
    }

    if !arg
        .name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!(
            "invalid preset argument name {:?}: expected ASCII letters, numbers, '-' or '_'",
            arg.name
        );
    }

    let name: &'static str = arg.name.clone().leak();
    let help: &'static str = format!("{}  [default: {}]", arg.description, arg.default).leak();

    Ok(Arg::new(name)
        .long(name)
        .help(help)
        .action(ArgAction::Set)
        .required(false))
}

/// Convert a string from clap into the JSON type the server expects.
fn coerce(ty: &str, raw: &str) -> Result<Value> {
    match ty {
        "int" => Ok(json!(
            raw.parse::<i64>()
                .with_context(|| format!("expected int, got {raw:?}"))?
        )),
        "float" => Ok(json!(
            raw.parse::<f64>()
                .with_context(|| format!("expected float, got {raw:?}"))?
        )),
        "bool" => match raw.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(json!(true)),
            "false" | "0" | "no" | "off" => Ok(json!(false)),
            other => bail!("expected bool, got {other:?}"),
        },
        "color" => {
            // Server color fields are (r,g,b) tuples; named colors won't shape-match server-side.
            let hex = normalize(raw)?;
            let [r, g, b] = parse_hex_rgb(&hex)
                .ok_or_else(|| anyhow!("color args must be hex, e.g. #FF0000; got {raw:?}"))?;

            Ok(json!([r, g, b]))
        }
        "string" => Ok(json!(raw)),
        other => {
            eprintln!("warning: unknown arg type {other:?}, sending as string");
            Ok(json!(raw))
        }
    }
}
