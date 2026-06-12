use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "cursed-coder",
    version,
    about = "An AI-powered coding agent that operates from the terminal"
)]
pub struct Args {
    #[arg(
        short = 'c',
        long = "cycles",
        help = "Maximum number of execution cycles (0 = infinite)"
    )]
    pub cycles: Option<usize>,

    #[arg(
        short = 'y',
        long = "yes",
        help = "Skip startup confirmation and begin immediately"
    )]
    pub yes: bool,

    #[command(subcommand)]
    pub command: Option<CliSubcommand>,
}

#[derive(Subcommand, Debug)]
pub enum CliSubcommand {
    /// Initialize the local workspace for cursed-coder
    Init,
}
