use anyhow::{Result, bail};
use clap::{Arg, ArgAction};
use owo_colors::OwoColorize;
use serde_json::{Value, json};

use lightwave_core::Client;

use crate::commands::start::{effect_command, parse_effect_args};

pub fn list(c: &Client, json_mode: bool) -> Result<()> {
    let resp = c.list_presets()?;

    if json_mode {
        crate::commands::print_json(&json!({
            "ok": true,
            "presets": resp.presets,
        }))?;

        return Ok(());
    }

    if resp.presets.is_empty() {
        println!(
            "  {}  no presets saved — create one with {}",
            "○".dimmed(),
            "lightwave preset save <name> <effect> [args]".bright_cyan()
        );
        return Ok(());
    }

    println!(
        "{} {} preset{}\n",
        "●".green(),
        resp.presets.len().bold(),
        if resp.presets.len() == 1 { "" } else { "s" }
    );

    let name_w = resp.presets.iter().map(|p| p.name.len()).max().unwrap_or(0);
    let effect_w = resp
        .presets
        .iter()
        .map(|p| p.effect.len())
        .max()
        .unwrap_or(0);

    for p in resp.presets {
        let detail = if p.description.is_empty() {
            format_args_inline(&p.args)
        } else {
            p.description
        };
        println!(
            "  {}  {:<name_w$}  {:<effect_w$}  {}",
            "▸".bright_magenta(),
            p.name.bright_white().bold(),
            p.effect.bright_cyan(),
            detail.dimmed(),
            name_w = name_w,
            effect_w = effect_w
        );
    }

    Ok(())
}

fn format_args_inline(args: &Value) -> String {
    match args.as_object() {
        Some(map) if !map.is_empty() => map
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" "),
        _ => "defaults".to_string(),
    }
}

pub fn save(
    client: &Client,
    name: &str,
    effect: &str,
    rest: &[String],
    json_mode: bool,
) -> Result<()> {
    let Some(info) = client.effect_info(effect)? else {
        bail!("no effect named {effect:?}");
    };

    if info.args.iter().any(|a| a.name == "description") {
        bail!("effect {effect:?} has an option named 'description', which is reserved");
    }

    let cmd = effect_command(&format!("{name} ({effect})"), &info.description).arg(
        Arg::new("description")
            .long("description")
            .help("Free-text description shown in preset listings")
            .action(ArgAction::Set)
            .required(false),
    );

    let (payload, matches) = parse_effect_args(cmd, &info.args, rest, json_mode)?;

    let description = matches
        .get_one::<String>("description")
        .cloned()
        .unwrap_or_default();

    let record = client.save_preset(name, effect, &Value::Object(payload), &description)?;

    if json_mode {
        crate::commands::print_ok_json(json!({
            "action": "preset_save",
            "preset": record,
        }))?;
    } else {
        println!(
            "  {} saved {}  {}  {}",
            "✔".bright_green(),
            record.name.bright_white().bold(),
            record.effect.bright_cyan(),
            format_args_inline(&record.args).dimmed()
        );
    }

    Ok(())
}

pub fn rm(c: &Client, name: &str, json_mode: bool) -> Result<()> {
    if !c.delete_preset(name)? {
        bail!("no preset named {name:?}");
    }

    if json_mode {
        crate::commands::print_ok_json(json!({
            "action": "preset_rm",
            "preset": name,
        }))?;
    } else {
        println!(
            "  {} removed {}",
            "✔".bright_green(),
            name.bright_white().bold()
        );
    }

    Ok(())
}
