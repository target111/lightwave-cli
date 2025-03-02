mod api;
mod cli;
mod models;
mod utils;

use clap::Parser;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let cli = cli::Cli::parse();
    
    // Execute the appropriate command
    cli::handle_command(cli)
}
