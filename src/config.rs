use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GlobalConfig {
    pub provider: String,
    pub model: String,
    pub max_cycles: u64,
    pub log_level: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            provider: "openrouter".to_string(),
            model: "openrouter/free".to_string(),
            max_cycles: 10,
            log_level: "info".to_string(),
        }
    }
}

pub fn ensure_global_configs() -> Result<GlobalConfig, Box<dyn std::error::Error>> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| Box::<dyn std::error::Error>::from("failed to resolve config directory"))?
        .join("cursedcoder");

    std::fs::create_dir_all(&config_dir)?;

    let config_path = config_dir.join("config.json");
    if !config_path.exists() {
        let defaults = GlobalConfig::default();
        let content = serde_json::to_string_pretty(&defaults)?;
        std::fs::write(&config_path, content)?;
    }

    let env_path = config_dir.join(".env");
    if !env_path.exists() {
        let content = r#"# cursed-coder API Configuration
# Set the API key for your chosen provider.
# You only need the key for the provider configured in config.json.

# OpenRouter API key (required for cloud models)
OPENROUTER_API_KEY=your_key_here
"#;
        std::fs::write(&env_path, content)?;
    }

    dotenvy::from_path(&env_path)?;

    let config_file = std::fs::File::open(&config_path)?;
    let config: GlobalConfig = serde_json::from_reader(config_file)?;

    Ok(config)
}
