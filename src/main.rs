use clap::Parser;
use cursed_coder::cli::{Args, CliSubcommand};
use cursed_coder::config;
use cursed_coder::dashboard;
use cursed_coder::engine;
use std::path::Path;

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());

    match args.command {
        Some(CliSubcommand::Init) => {
            config::ensure_global_configs().unwrap_or_else(|e| {
                eprintln!("Failed to load config: {e}");
                std::process::exit(1);
            });

            if let Err(e) = engine::initialize_workspace(&cwd) {
                eprintln!("Failed to initialize workspace: {e}");
                std::process::exit(1);
            }

            println!(
                "Initialized empty cursed-coder workspace in {}",
                cwd.join(".cursedcoder").display()
            );
        }
        None => {
            if !engine::has_steps_toml(&cwd) {
                eprintln!(
                    "fatal: not a cursedcoder workspace (or any of the parent directories): \
                     .cursedcoder/steps.toml not found. \
                     Run 'cursedcoder init' to initialize a workspace."
                );
                std::process::exit(1);
            }

            let cfg = config::ensure_global_configs().unwrap_or_else(|e| {
                eprintln!("Failed to load config: {e}");
                std::process::exit(1);
            });

            let config_dir = dirs::config_dir()
                .map(|p| p.join("cursedcoder"))
                .unwrap_or_else(|| Path::new(".").to_path_buf());
            let config_path = config_dir.join("config.json");

            let graph = engine::parse_and_validate_steps(&cwd).unwrap_or_else(|e| {
                eprintln!("fatal: invalid step graph: {e}");
                std::process::exit(1);
            });

            dashboard::print(
                env!("CARGO_PKG_VERSION"),
                &cwd,
                &config_path,
                &cfg,
                args.cycles,
                &cwd,
            );

            cursed_coder::gatekeeper::verify_execution_consent(args.yes).unwrap_or_else(|_| {
                std::process::exit(1);
            });

            if !args.yes {
                println!();
                println!("Entering execution loop with step '{}'...", graph.entry_point);
            }
        }
    }
}
