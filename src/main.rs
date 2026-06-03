mod cli;

use clap::Parser;
use cli::{Args, CliSubcommand};

#[tokio::main]
async fn main() {
    let args = Args::parse();

    match args.command {
        Some(CliSubcommand::Init) => {
            println!("Initializing cursed-coder workspace...");
        }
        None => {
            let cycles = args.cycles.unwrap_or(0);
            if !args.yes {
                println!("Starting cursed-coder (cycles: {})", cycles);
            }
            println!("Entering execution loop...");
        }
    }
}
