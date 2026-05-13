use lightwave_core::{Client, color::{normalize, parse_hex_rgb}};
use anyhow::{Result, bail};
use owo_colors::OwoColorize;

pub fn set(c: &Client, input: &str, json_mode: bool) -> Result<()> {
    let hex = normalize(input)?;
    c.set_color(&hex)?;
    if json_mode {
        crate::commands::print_ok_json(serde_json::json!({ "action": "color_set", "color": hex }));
        return Ok(());
    }

    if let Some([r, g, b]) = parse_hex_rgb(&hex) {
        println!(
            "  {} color set to {} {}",
            "●".truecolor(r, g, b),
            hex.bright_white().bold(),
            "██".truecolor(r, g, b)
        );
        return Ok(());
    }
    println!(
        "  {} color set to {}",
        "●".bright_white(),
        hex.bright_white().bold()
    );
    Ok(())
}

pub fn brightness(c: &Client, level: f32, json_mode: bool) -> Result<()> {
    if !(0.0..=1.0).contains(&level) {
        bail!("brightness must be between 0.0 and 1.0");
    }
    c.set_brightness(level)?;
    if json_mode {
        crate::commands::print_ok_json(serde_json::json!({
            "action": "brightness", "level": level,
        }));
        return Ok(());
    }

    let filled = (level * 20.0).round() as usize;
    let bar: String = (0..20)
        .map(|i| if i < filled { '█' } else { '░' })
        .collect();
    println!(
        "  {} brightness {} {:>5.0}%",
        "☀".bright_yellow(),
        bar.bright_yellow(),
        (level * 100.0)
    );
    Ok(())
}

pub fn clear(c: &Client, json_mode: bool) -> Result<()> {
    c.clear()?;
    if json_mode {
        crate::commands::print_ok_json(serde_json::json!({ "action": "clear" }));
    } else {
        println!("  {} cleared", "○".dimmed());
    }
    Ok(())
}
