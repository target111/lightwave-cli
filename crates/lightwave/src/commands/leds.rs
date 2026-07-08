use anyhow::{Result, bail};
use lightwave_core::{
    Client,
    color::{normalize, parse_hex_rgb},
};
use owo_colors::OwoColorize;

pub fn state(c: &Client, json_mode: bool) -> Result<()> {
    let state = c.led_state()?;

    // A strip showing one non-black color everywhere is a "solid color" —
    // the state manual controls and plugins care about.
    let uniform = state
        .pixels
        .first()
        .filter(|&&first| first != [0, 0, 0] && state.pixels.iter().all(|&p| p == first));
    let uniform_hex = uniform.map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}"));
    let lit = state.pixels.iter().any(|&p| p != [0, 0, 0]);

    if json_mode {
        crate::commands::print_ok_json(serde_json::json!({
            "count": state.count,
            "brightness": state.brightness,
            "lit": lit,
            "color": uniform_hex,
            "pixels": state.pixels,
        }))?;
        return Ok(());
    }

    let filled = (state.brightness * 20.0).round() as usize;
    let bar = (0..20)
        .map(|i| if i < filled { '█' } else { '░' })
        .collect::<String>();

    println!(
        "\n  {} {} LEDs · {} {:>3.0}%",
        "●".bright_cyan(),
        state.count.bold(),
        bar.bright_yellow(),
        state.brightness * 100.0
    );

    match (uniform, lit) {
        (Some([r, g, b]), _) => println!(
            "  {} solid {} {}",
            "›".dimmed(),
            uniform_hex.unwrap_or_default().bright_white().bold(),
            "██".truecolor(*r, *g, *b)
        ),
        (None, true) => {
            // Sample the strip down to a terminal-width preview.
            let samples = 40.min(state.count);
            let preview = (0..samples)
                .map(|i| {
                    let [r, g, b] = state.pixels[i * state.count / samples];
                    "█".truecolor(r, g, b).to_string()
                })
                .collect::<String>();
            println!("  {} {}", "›".dimmed(), preview);
        }
        (None, false) => println!("  {} off", "›".dimmed()),
    }

    println!();

    Ok(())
}

pub fn set(c: &Client, input: &str, json_mode: bool) -> Result<()> {
    let hex = normalize(input)?;
    c.set_color(&hex)?;

    if json_mode {
        crate::commands::print_ok_json(serde_json::json!({
            "action": "color_set",
            "color": hex,
        }))?;
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
            "action": "brightness",
            "level": level,
        }))?;
        return Ok(());
    }

    let filled = (level * 20.0).round() as usize;
    let bar = (0..20)
        .map(|i| if i < filled { '█' } else { '░' })
        .collect::<String>();

    println!(
        "  {} brightness {} {:>5.0}%",
        "☀".bright_yellow(),
        bar.bright_yellow(),
        level * 100.0
    );

    Ok(())
}

pub fn clear(c: &Client, json_mode: bool) -> Result<()> {
    c.clear()?;

    if json_mode {
        crate::commands::print_ok_json(serde_json::json!({ "action": "clear" }))?;
    } else {
        println!("  {} cleared", "○".dimmed());
    }

    Ok(())
}
