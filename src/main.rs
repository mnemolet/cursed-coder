use clap::Parser;
use cursed_coder::cli::{Args, CliSubcommand};
use cursed_coder::config;
use cursed_coder::dashboard;
use std::path::Path;

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
            let cfg = config::ensure_global_configs().unwrap_or_else(|e| {
                eprintln!("Failed to load config: {e}");
                std::process::exit(1);
            });

            let config_dir = dirs::config_dir()
                .map(|p| p.join("cursedcoder"))
                .unwrap_or_else(|| Path::new(".").to_path_buf());
            let config_path = config_dir.join("config.json");
            let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());

            dashboard::print(
                env!("CARGO_PKG_VERSION"),
                &cwd,
                &config_path,
                &cfg,
                args.cycles,
                &cwd,
            );

            if !args.yes {
                println!();
                println!("Entering execution loop...");
            }
        }
    }
}
