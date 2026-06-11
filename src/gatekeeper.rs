use std::io::{self, BufRead, Write};
use tracing::info;

pub fn verify_execution_consent(bypass_flag: bool) -> Result<(), Box<dyn std::error::Error>> {
    if bypass_flag {
        info!("Autonomous execution loop initiated via CLI bypass flag.");
        return Ok(());
    }

    if !is_interactive() {
        eprintln!(
            "Error: Non-interactive environment detected. \
             Autonomous execution loop requires the '--yes' (-y) bypass flag \
             to run in headless environments (e.g., CI/CD, Cron)."
        );
        return Err("non-interactive environment detected".into());
    }

    print!("Confirmation: Start autonomous execution loop? [y/N]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;

    match input.trim() {
        "y" | "Y" => Ok(()),
        _ => {
            println!("Execution cancelled by user.");
            Err("execution cancelled by user".into())
        }
    }
}

fn is_interactive() -> bool {
    #[cfg(unix)]
    {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .is_ok()
    }
    #[cfg(not(unix))]
    {
        false
    }
}
