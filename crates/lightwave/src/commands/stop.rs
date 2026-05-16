use anyhow::Result;
use lightwave_core::Client;
use owo_colors::OwoColorize;

pub fn run(c: &Client, json_mode: bool) -> Result<()> {
    c.stop()?;

    if json_mode {
        crate::commands::print_ok_json(serde_json::json!({ "action": "stop" }))?;
    } else {
        println!("  {} stopped", "■".bright_red());
    }

    Ok(())
}
