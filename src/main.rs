use clap::Parser;
use cursed_coder::cli::{Args, CliSubcommand};
use cursed_coder::config;
use cursed_coder::dashboard;
use cursed_coder::engine;
use cursed_coder::guard;
use cursed_coder::memory::Memory;
use std::path::Path;

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());

    if let Err(e) = guard::validate_workspace_root(&cwd) {
        eprintln!("{e}");
        std::process::exit(1);
    }

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

            if engine::is_unconfigured_template(&cwd) {
                eprintln!("{}", engine::EngineError::UnconfiguredTemplate);
                std::process::exit(1);
            }

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
            }

            let max_cycles = args.cycles.map(|c| c as u64).unwrap_or(
                cfg.max_cycles,
            );

            let mut runtime_memory = Memory::load_or_create(&cwd.join(".cursedcoder"))
                .unwrap_or_else(|e| {
                    eprintln!("Failed to load runtime memory: {e}");
                    std::process::exit(1);
                });

            runtime_memory.set_variable(
                "_cursed_entry_point",
                serde_json::Value::String(graph.entry_point.clone()),
            );
            runtime_memory.set_variable(
                "_cursed_workspace",
                serde_json::Value::String(cwd.to_string_lossy().to_string()),
            );

            if let Err(e) = engine::run_pipeline(
                max_cycles,
                graph.steps,
                &mut runtime_memory,
                &cwd,
            )
            .await
            {
                eprintln!("Pipeline error: {e}");
                std::process::exit(1);
            }
        }
    }
}
