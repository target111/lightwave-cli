use anyhow::Result;
use lightwave_core::{Client, color::parse_hex_rgb};
use owo_colors::OwoColorize;

/// One-request strip summary via GET /api/state: the running effect,
/// brightness, and solid color, without dragging the pixel buffer over
/// the wire — the poll status clients (like the Noctalia plugin) want.
pub fn run(c: &Client, json_mode: bool) -> Result<()> {
    let state = c.state()?;

    if json_mode {
        crate::commands::print_ok_json(serde_json::json!({
            "running": state.running,
            "count": state.count,
            "brightness": state.brightness,
            "color": state.color,
            "lit": state.lit,
        }))?;
        return Ok(());
    }

    match &state.running {
        Some(r) => {
            let title = crate::commands::running_title(r);
            println!(
                "\n  {} {}  {}",
                "●".bright_green(),
                title.bright_white().bold(),
                format!("({:.0}s)", r.duration_seconds).dimmed()
            );
        }
        None => match state.color.as_deref().and_then(parse_hex_rgb) {
            Some([r, g, b]) => println!(
                "\n  {} solid {} {}",
                "●".truecolor(r, g, b),
                format!("#{r:02x}{g:02x}{b:02x}").bright_white().bold(),
                "██".truecolor(r, g, b)
            ),
            None if state.lit => println!("\n  {} lit", "●".bright_yellow()),
            None => println!("\n  {} off", "○".dimmed()),
        },
    }

    let bar = crate::commands::brightness_bar(state.brightness);
    println!(
        "  {} {} LEDs · {} {:>3.0}%\n",
        "›".dimmed(),
        state.count.bold(),
        bar.bright_yellow(),
        state.brightness * 100.0
    );

    Ok(())
}
