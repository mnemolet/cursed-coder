mod cli;
mod config;

use clap::Parser;
use cli::{Args, CliSubcommand};

#[tokio::main]
async fn main() {
    let args = Args::parse();

    match args.command {
        Some(CliSubcommand::Init) => match config::ensure_global_configs() {
            Ok(cfg) => {
                println!("Initialized cursed-coder workspace at {:?}", dirs::config_dir().map(|p| p.join("cursedcoder")));
                println!("  provider:  {}", cfg.provider);
                println!("  model:     {}", cfg.model);
                println!("  log_level: {}", cfg.log_level);
            }
            Err(e) => {
                eprintln!("Failed to initialize workspace: {e}");
                std::process::exit(1);
            }
        },
        None => {
            let cycles = args.cycles.unwrap_or(0);
            if !args.yes {
                println!("Starting cursed-coder (cycles: {})", cycles);
            }
            println!("Entering execution loop...");
        }
    }
}
