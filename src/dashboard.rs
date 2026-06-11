use crate::config::GlobalConfig;
use std::path::Path;

pub fn print(
    version: &str,
    cwd: &Path,
    config_path: &Path,
    config: &GlobalConfig,
    max_cycles_override: Option<usize>,
    local_path: &Path,
) {
    println!("cursed-coder v{}", version);
    println!("Current Directory: {}", cwd.to_string_lossy());
    println!();
    println!("GLOBAL CONFIG LAYER");
    println!("  - Path: {}", config_path.to_string_lossy());
    println!("  - Provider: {}", config.provider);
    println!("  - Model: {}", config.model);
    let effective_cycles = max_cycles_override.unwrap_or(config.max_cycles as usize);
    let cycle_display = if effective_cycles == 0 {
        "infinite"
    } else {
        &effective_cycles.to_string()
    };
    println!("  - Max Cycle: {}", cycle_display);
    println!();
    println!("LOCAL WORKSPACE MODULES");
    println!("  - Path: {}", local_path.to_string_lossy());
}
