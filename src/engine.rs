use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    LlmCompletion,
    ShellCommand,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Step {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub action_type: ActionType,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub on_success: String,
    #[serde(default)]
    pub on_failure: String,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default)]
    pub on_retry: String,
}

const fn default_max_retries() -> u32 {
    1
}

#[derive(Debug)]
pub struct StepGraph {
    pub steps: HashMap<String, Step>,
    pub entry_point: String,
}

#[derive(Debug)]
pub enum GraphError {
    MissingEntry(String),
    BrokenTransition {
        step: String,
        field: &'static str,
        target: String,
    },
    Io(std::io::Error),
    Parse(toml::de::Error),
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::MissingEntry(name) => {
                write!(f, "entry point step '{name}' not found in steps.toml")
            }
            GraphError::BrokenTransition { step, field, target } => {
                write!(
                    f,
                    "step '{step}' has {field} pointing to '{target}' which does not exist"
                )
            }
            GraphError::Io(e) => write!(f, "I/O error: {e}"),
            GraphError::Parse(e) => write!(f, "TOML parse error: {e}"),
        }
    }
}

impl std::error::Error for GraphError {}

impl From<std::io::Error> for GraphError {
    fn from(e: std::io::Error) -> Self {
        GraphError::Io(e)
    }
}

impl From<toml::de::Error> for GraphError {
    fn from(e: toml::de::Error) -> Self {
        GraphError::Parse(e)
    }
}

const STEPS_TOML_TEMPLATE: &str = r#"[[step]]
name = "Initial Step"
description = "First step of the cursed-coder pipeline"
action_type = "llm_completion"
prompt = ""
command = ""
enabled = true
on_success = ""
on_failure = ""
max_retries = 1
on_retry = ""
"#;

pub fn initialize_workspace(workspace_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dot_dir = workspace_dir.join(".cursedcoder");
    fs::create_dir_all(&dot_dir)?;

    let steps_path = dot_dir.join("steps.toml");
    if !steps_path.exists() {
        fs::write(&steps_path, STEPS_TOML_TEMPLATE)?;
    }

    Ok(())
}

pub fn parse_and_validate_steps(workspace_dir: &Path) -> Result<StepGraph, GraphError> {
    let steps_path = workspace_dir.join(".cursedcoder").join("steps.toml");
    let content = fs::read_to_string(&steps_path)?;
    let steps: Vec<Step> = toml::from_str(&content)?;

    if steps.is_empty() {
        return Err(GraphError::MissingEntry("(none)".to_string()));
    }

    let entry_point = steps[0].name.clone();
    let mut map = HashMap::new();

    for step in &steps {
        map.insert(step.name.clone(), step.clone());
    }

    for step in &steps {
        for (field, target) in [
            ("on_success", &step.on_success),
            ("on_failure", &step.on_failure),
            ("on_retry", &step.on_retry),
        ] {
            if target.is_empty() || target == "exit" || target == "done" {
                continue;
            }
            if !map.contains_key(target.as_str()) {
                return Err(GraphError::BrokenTransition {
                    step: step.name.clone(),
                    field,
                    target: target.clone(),
                });
            }
        }
    }

    Ok(StepGraph {
        steps: map,
        entry_point,
    })
}

/// Returns true if the workspace has a valid steps.toml file.
pub fn has_steps_toml(workspace_dir: &Path) -> bool {
    workspace_dir
        .join(".cursedcoder")
        .join("steps.toml")
        .exists()
}
