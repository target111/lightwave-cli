use anyhow::{Result, bail};
use owo_colors::OwoColorize;
use serde_json::json;

use lightwave_core::{ArgSchema, Client};

pub fn list(c: &Client, json_mode: bool) -> Result<()> {
    let resp = c.list_effects()?;

    if json_mode {
        let effects = resp
            .effects
            .iter()
            .map(|e| {
                json!({
                    "name": &e.name,
                    "description": &e.description,
                })
            })
            .collect::<Vec<_>>();

        crate::commands::print_json(&json!({
            "ok": true,
            "effects": effects,
        }))?;

        return Ok(());
    }

    if resp.effects.is_empty() {
        println!("{}  no effects registered", "✗".red());
        return Ok(());
    }

    println!(
        "{} {} effect{}\n",
        "●".green(),
        resp.effects.len().bold(),
        if resp.effects.len() == 1 { "" } else { "s" }
    );

    let width = resp.effects.iter().map(|e| e.name.len()).max().unwrap_or(0);

    for e in resp.effects {
        println!(
            "  {}  {:<width$}  {}",
            "▸".bright_magenta(),
            e.name.bright_white().bold(),
            e.description.dimmed(),
            width = width
        );
    }

    Ok(())
}

pub fn info(c: &Client, name: &str, json_mode: bool) -> Result<()> {
    let Some(info) = c.effect_info(name)? else {
        bail!("no effect named {name:?}");
    };

    if json_mode {
        let args = info
            .args
            .iter()
            .map(|a| {
                json!({
                    "name": &a.name,
                    "type": &a.arg_type,
                    "default": &a.default,
                    "description": &a.description,
                })
            })
            .collect::<Vec<_>>();

        crate::commands::print_json(&json!({
            "ok": true,
            "name": name,
            "description": &info.description,
            "args": args,
        }))?;

        return Ok(());
    }

    println!(
        "\n  {}  {}",
        "✦".bright_yellow(),
        name.bright_white().bold()
    );
    println!("  {}\n", info.description.italic().dimmed());

    if info.args.is_empty() {
        println!("  {}  no arguments\n", "·".dimmed());
        return Ok(());
    }

    println!("  {}", "Arguments".underline().bold());

    let name_w = info.args.iter().map(|a| a.name.len()).max().unwrap_or(0);

    for a in &info.args {
        print_arg(a, name_w);
    }

    println!();

    Ok(())
}

fn print_arg(a: &ArgSchema, name_w: usize) {
    let (glyph, type_str) = match a.arg_type.as_str() {
        "int" => ("◆", a.arg_type.bright_blue().to_string()),
        "float" => ("◇", a.arg_type.bright_cyan().to_string()),
        "bool" => ("◉", a.arg_type.bright_magenta().to_string()),
        "color" => ("●", a.arg_type.bright_red().to_string()),
        "string" => ("▪", a.arg_type.bright_green().to_string()),
        _ => ("•", a.arg_type.white().to_string()),
    };

    println!(
        "    {} --{:<w$}  {}  {}  {}",
        glyph.bright_yellow(),
        a.name.cyan(),
        format!("({type_str})").dimmed(),
        a.description,
        format!("[default: {}]", a.default).dimmed().italic(),
        w = name_w
    );
}

pub fn running(c: &Client, json_mode: bool) -> Result<()> {
    let running = c.running()?;

    if json_mode {
        let payload = match &running {
            None => json!({
                "ok": true,
                "running": null,
            }),
            Some(r) => json!({
                "ok": true,
                "running": {
                    "name": &r.name,
                    "description": &r.description,
                    "preset": &r.preset,
                    "start_time": &r.start_time,
                    "duration_seconds": r.duration_seconds,
                },
            }),
        };

        crate::commands::print_json(&payload)?;
        return Ok(());
    }

    match running {
        None => println!("  {}  nothing running", "○".dimmed()),
        Some(r) => {
            let title = match &r.preset {
                Some(preset) => format!("{preset} · {}", r.name),
                None => r.name.clone(),
            };
            println!(
                "\n  {} {}  {}",
                "●".bright_green(),
                title.bright_white().bold(),
                format!("({:.1}s)", r.duration_seconds).dimmed()
            );
            println!("  {}", r.description.italic().dimmed());
            println!("  {} started {}\n", "›".dimmed(), r.start_time.dimmed());
        }
    }

    Ok(())
}
