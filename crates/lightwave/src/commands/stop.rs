use anyhow::Result;
use lightwave_core::Client;
use owo_colors::OwoColorize;

pub fn run(c: &Client, json_mode: bool) -> Result<()> {
    let was_running = c.stop()?;

    if json_mode {
        crate::commands::print_ok_json(serde_json::json!({
            "action": "stop",
            "was_running": was_running,
        }))?;
    } else if was_running {
        println!("  {} stopped", "■".bright_red());
    } else {
        println!("  {} nothing was running", "○".dimmed());
    }

    Ok(())
}
