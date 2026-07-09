use anyhow::{Context, Result, anyhow, bail};
use clap::{Arg, ArgAction, Command};
use owo_colors::OwoColorize;
use serde_json::{Value, json};

use lightwave_core::{
    ArgSchema, Client,
    color::{normalize, parse_hex_rgb},
};

pub fn run(client: &Client, name: &str, rest: &[String], json_mode: bool) -> Result<()> {
    // `start` takes an effect or a saved preset; the server forbids the
    // names from colliding, so whichever matches is unambiguous.
    match client.effect_info(name)? {
        Some(info) => start_effect(client, name, &info.description, &info.args, rest, json_mode),
        None => start_preset(client, name, rest, json_mode),
    }
}

/// Base clap command for parsing an effect's dynamic args. Callers add the
/// effect's schema (via `parse_effect_args`) plus any command-specific flags.
pub fn effect_command(name: &str, description: &str) -> Command {
    // clap stores arg/command identifiers as &'static str; leak the dynamic strings
    let name: &'static str = name.to_string().leak();
    let about: &'static str = description.to_string().leak();

    Command::new(name)
        .no_binary_name(true)
        .about(about)
        .disable_help_subcommand(true)
}

/// Add `schema` as `--flags` to `cmd`, parse `rest`, and return the JSON
/// payload of the options the user actually set — unset ones are omitted so
/// the server falls back to its own defaults. The parsed matches are returned
/// too, for callers that registered extra flags. On a parse error this prints
/// clap's message (or a JSON error) and exits the process.
pub fn parse_effect_args(
    mut cmd: Command,
    schema: &[ArgSchema],
    rest: &[String],
    json_mode: bool,
) -> Result<(serde_json::Map<String, Value>, clap::ArgMatches)> {
    for arg in schema {
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

    let mut payload = serde_json::Map::new();

    for arg in schema {
        if matches.value_source(&arg.name) == Some(clap::parser::ValueSource::CommandLine) {
            let raw = matches
                .get_one::<String>(&arg.name)
                .ok_or_else(|| anyhow!("missing value for --{}", arg.name))?;

            payload.insert(arg.name.clone(), coerce(&arg.arg_type, raw)?);
        }
    }

    Ok((payload, matches))
}

fn start_effect(
    client: &Client,
    effect: &str,
    description: &str,
    schema: &[ArgSchema],
    rest: &[String],
    json_mode: bool,
) -> Result<()> {
    let cmd = effect_command(effect, description).styles(
        clap::builder::Styles::styled()
            .header(anstyle::Style::new().bold().underline())
            .literal(anstyle::AnsiColor::BrightCyan.on_default())
            .placeholder(anstyle::AnsiColor::BrightYellow.on_default()),
    );

    let (payload, _) = parse_effect_args(cmd, schema, rest, json_mode)?;
    let args = Value::Object(payload);

    client.start(effect, &args)?;

    if json_mode {
        crate::commands::print_ok_json(serde_json::json!({
            "action": "start",
            "effect": effect,
            "args": args,
        }))?;
    } else {
        println!(
            "  {} started {}",
            "▶".bright_green(),
            effect.bright_white().bold()
        );
    }

    Ok(())
}

fn start_preset(client: &Client, name: &str, rest: &[String], json_mode: bool) -> Result<()> {
    if !rest.is_empty() {
        bail!(
            "no effect named {name:?}; if you meant the preset, note that presets \
             don't take arguments — their options are saved with `lightwave preset save`"
        );
    }

    let Some(status) = client.start_preset(name)? else {
        bail!("no effect or preset named {name:?}");
    };

    let effect = status.effect.unwrap_or_default();

    if json_mode {
        crate::commands::print_ok_json(serde_json::json!({
            "action": "start",
            "effect": effect,
            "preset": name,
        }))?;
    } else {
        println!(
            "  {} started {}  {}",
            "▶".bright_green(),
            name.bright_white().bold(),
            format!("({effect})").dimmed()
        );
    }

    Ok(())
}

fn build_arg(arg: &ArgSchema) -> Result<Arg> {
    if arg.name.is_empty() {
        bail!("effect argument name cannot be empty");
    }

    if arg.name.starts_with('-') {
        bail!(
            "invalid effect argument name {:?}: must not start with '-'",
            arg.name
        );
    }

    if arg.name == "help" {
        bail!(
            "invalid effect argument name {:?}: name is reserved",
            arg.name
        );
    }

    if !arg
        .name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!(
            "invalid effect argument name {:?}: expected ASCII letters, numbers, '-' or '_'",
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
