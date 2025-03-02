use clap::{Parser, Subcommand};
use colored::Colorize;
use std::error::Error;

use crate::api::{ApiError, LightWaveClient};
use crate::utils::{format_error, format_time, format_value, parse_params};

#[derive(Parser)]
#[command(
    author,
    version,
    about = "LightWave LED control client",
    long_about = "Command-line client for controlling LightWave LED server"
)]
pub struct Cli {
    #[arg(
        short,
        long,
        help = "API server URL (can also be set with LIGHTWAVE_URL environment variable)"
    )]
    pub url: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Effect management commands
    Effects {
        #[command(subcommand)]
        action: EffectCommands,
    },
    /// LED control commands
    Leds {
        #[command(subcommand)]
        action: LedCommands,
    },
    /// Get the current system status
    Status,
}

#[derive(Subcommand)]
pub enum EffectCommands {
    /// List all available effects
    List,
    /// Get info about the currently running effect
    Running,
    /// Get detailed info about a specific effect
    Info {
        /// Name of the effect
        name: String,
    },
    /// Start an effect
    Start {
        /// Name of the effect to start
        name: String,
        /// Parameters for the effect (key=value format)
        #[arg(short, long)]
        param: Vec<String>,
    },
    /// Stop the currently running effect
    Stop,
}

#[derive(Subcommand)]
pub enum LedCommands {
    /// Set the color of the LEDs
    Color {
        /// Color in any format (hex, rgb, hsl, hsv, etc.)
        color: String,
    },
    /// Set the brightness of the LEDs (0.0 - 1.0)
    Brightness {
        /// Brightness value in float range 0.0 - 1.0
        brightness: f32,
    },
    /// Turn off all LEDs
    Clear,
}

pub fn handle_command(cli: Cli) -> Result<(), Box<dyn Error>> {
    // Create API client
    let client = match &cli.url {
        Some(url) => LightWaveClient::with_base_url(url)?,
        None => LightWaveClient::new()?,
    };

    match cli.command {
        Commands::Effects { action } => handle_effect_commands(&client, action),
        Commands::Leds { action } => handle_led_commands(&client, action),
        Commands::Status => handle_status(&client),
    }
}

fn handle_effect_commands(client: &LightWaveClient, action: EffectCommands) -> Result<(), Box<dyn Error>> {
    match action {
        EffectCommands::List => {
            let resp = client.list_effects()?;

            println!("\n{}\n", "Available Effects:".bold().underline());
            for effect in &resp.effects {
                println!(
                    "• {} - {}",
                    effect.name.green().bold(),
                    effect.description.trim()
                );
            }
            println!(
                "\n{} {}\n",
                "Total:".bold(),
                resp.effects.len().to_string().cyan()
            );
        }
        EffectCommands::Running => {
            let resp = client.get_effect_status()?;

            if resp.running {
                println!("\n{}\n", "Running Effect:".bold().underline());
                println!("• {}: {}", "Name".bold(), resp.name.unwrap_or_default().green());
                println!(
                    "• {}: {}",
                    "Description".bold(),
                    resp.description.unwrap_or_default()
                );

                if let Some(params) = resp.parameters {
                    println!("• {}:", "Parameters".bold());
                    for (key, value) in params {
                        println!("  - {}: {}", key.cyan(), format_value(&value));
                    }
                }

                if let Some(start_time) = &resp.start_time {
                    println!("• {}: {}", "Started".bold(), start_time);
                }

                if let Some(runtime) = resp.runtime {
                    println!("• {}: {}", "Runtime".bold(), format_time(runtime).cyan());
                }
                println!();
            } else {
                println!("\n{}\n", "No effect is currently running.".yellow());
            }
        }
        EffectCommands::Info { name } => {
            match client.get_effect_info(&name) {
                Ok(resp) => {
                    println!("\n{}: {}\n", "Effect".bold().underline(), resp.name.green().bold());
                    println!("• {}: {}", "Description".bold(), resp.description);

                    if !resp.parameters.is_empty() {
                        println!("\n{}\n", "Parameters:".bold().underline());
                        for param in &resp.parameters {
                            println!("• {}: {}", param.name.green().bold(), param.description);
                            println!("  - {}: {}", "Type".bold(), param.param_type.cyan());
                            println!("  - {}: {}", "Default".bold(), format_value(&param.default));

                            if let Some(min) = &param.min_value {
                                println!("  - {}: {}", "Min Value".bold(), format_value(min));
                            }

                            if let Some(max) = &param.max_value {
                                println!("  - {}: {}", "Max Value".bold(), format_value(max));
                            }

                            if let Some(options) = &param.options {
                                println!(
                                    "  - {}: {}",
                                    "Options".bold(),
                                    options
                                        .iter()
                                        .map(|o| o.yellow().to_string())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                );
                            }
                            println!();
                        }
                    } else {
                        println!("\n{}\n", "No parameters available.".yellow());
                    }
                }
                Err(e) => {
                    if let ApiError::ApiResponseError(_msg, status) = &e {
                        if *status == reqwest::StatusCode::NOT_FOUND {
                            eprintln!("{}", format!("Effect '{}' not found", name).red());
                            // List available effects to help the user
                            match client.list_effects() {
                                Ok(effects) => {
                                    println!("\n{}\n", "Available effects:".bold());
                                    for effect in &effects.effects {
                                        println!("• {}", effect.name.green());
                                    }
                                    println!();
                                }
                                Err(_) => {}
                            }
                            return Err(e.into());
                        }
                    }
                    return Err(e.into());
                }
            }
        }
        EffectCommands::Start { name, param } => {
            let parameters = parse_params(&param);
            match client.start_effect(&name, parameters) {
                Ok(_) => println!("{} {}", "Started effect".green(), name.cyan().bold()),
                Err(e) => {
                    if let ApiError::ApiResponseError(_msg, status) = &e {
                        if *status == reqwest::StatusCode::NOT_FOUND {
                            eprintln!("{}", format!("Effect '{}' not found", name).red());
                            // List available effects to help the user
                            match client.list_effects() {
                                Ok(effects) => {
                                    println!("\n{}\n", "Available effects:".bold());
                                    for effect in &effects.effects {
                                        println!("• {}", effect.name.green());
                                    }
                                    println!();
                                }
                                Err(_) => {}
                            }
                        } else {
                            eprintln!("{}", format_error(&e));
                        }
                        return Err(e.into());
                    } else {
                        return Err(e.into());
                    }
                }
            }
        }
        EffectCommands::Stop => match client.stop_effect() {
            Ok(_) => println!("{}", "Effect stopped successfully.".green()),
            Err(e) => {
                if let ApiError::ApiResponseError(_msg, status) = &e {
                    if *status == reqwest::StatusCode::NOT_FOUND {
                        eprintln!("{}", "No effect is currently running".yellow());
                        return Ok(());
                    }
                }
                eprintln!("{}", format_error(&e));
                return Err(e.into());
            }
        },
    }
    Ok(())
}

fn handle_led_commands(client: &LightWaveClient, action: LedCommands) -> Result<(), Box<dyn Error>> {
    match action {
        LedCommands::Color { color } => match client.set_color(&color) {
            Ok(_) => println!("LED color set to {} successfully.", color.cyan()),
            Err(e) => {
                eprintln!("{}", format_error(&e));
                return Err(e.into());
            }
        },
        LedCommands::Brightness { brightness } => match client.set_brightness(brightness) {
            Ok(_) => println!(
                "LED brightness set to {} successfully.",
                format!("{:.1}%", brightness * 100.0).cyan()
            ),
            Err(e) => {
                eprintln!("{}", format_error(&e));
                return Err(e.into());
            }
        },
        LedCommands::Clear => match client.clear_leds() {
            Ok(_) => println!("{}", "LEDs cleared successfully.".green()),
            Err(e) => {
                eprintln!("{}", format_error(&e));
                return Err(e.into());
            }
        },
    }
    Ok(())
}

fn handle_status(client: &LightWaveClient) -> Result<(), Box<dyn Error>> {
    match client.get_effect_status() {
        Ok(resp) => {
            println!("\n{}\n", "LightWave Status:".bold().underline());

            // Effect status
            if resp.running {
                println!("• {}: {}", "Status".bold(), "Running".green());
                println!(
                    "• {}: {}",
                    "Effect".bold(),
                    resp.name.unwrap_or_default().green().bold()
                );

                if let Some(runtime) = resp.runtime {
                    println!("• {}: {}", "Runtime".bold(), format_time(runtime).cyan());
                }

                if let Some(params) = resp.parameters {
                    if !params.is_empty() {
                        println!("\n{}\n", "Parameters:".bold());
                        for (key, value) in params {
                            println!("  - {}: {}", key.cyan(), format_value(&value));
                        }
                    }
                }
            } else {
                println!("• {}: {}", "Status".bold(), "Idle".yellow());
                println!("• {}: {}", "Effect".bold(), "None".dimmed());
            }

            println!();
            Ok(())
        }
        Err(e) => {
            eprintln!("{}", format_error(&e));
            Err(e.into())
        }
    }
}