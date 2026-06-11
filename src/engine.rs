use crate::memory::Memory;
use crate::spinner::EngineSpinner;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::info;

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

#[derive(Deserialize)]
struct StepsFile {
    step: Vec<Step>,
}

pub fn parse_and_validate_steps(workspace_dir: &Path) -> Result<StepGraph, GraphError> {
    let steps_path = workspace_dir.join(".cursedcoder").join("steps.toml");
    let content = fs::read_to_string(&steps_path)?;
    let file: StepsFile = toml::from_str(&content)?;
    let steps = file.step;

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

#[derive(Debug)]
pub enum EngineError {
    StepNotFound(String),
    MaxCyclesReached { limit: u64, actual: u64 },
    Memory(crate::memory::MemoryError),
    Io(std::io::Error),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::StepNotFound(name) => {
                write!(f, "step '{name}' not found in execution graph")
            }
            EngineError::MaxCyclesReached { limit, actual } => {
                write!(f, "max cycles ({limit}) reached after {actual} cycles")
            }
            EngineError::Memory(e) => write!(f, "memory error: {e}"),
            EngineError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<crate::memory::MemoryError> for EngineError {
    fn from(e: crate::memory::MemoryError) -> Self {
        EngineError::Memory(e)
    }
}

impl From<std::io::Error> for EngineError {
    fn from(e: std::io::Error) -> Self {
        EngineError::Io(e)
    }
}

enum StepOutcome {
    Success,
    Failed,
}

/// Runs the autonomous execution pipeline.
///
/// Starts at the graph's entry point and follows transition hops
/// (`on_success`, `on_failure`, `on_retry`) until a terminating
/// state (`""`, `"exit"`, `"done"`) or the cycle limit is reached.
pub async fn run_pipeline(
    max_cycles: u64,
    steps: HashMap<String, Step>,
    memory: &mut Memory,
    _workspace_dir: &Path,
) -> Result<(), EngineError> {
    let spinner = EngineSpinner::new();
    let mut current_step_name = memory
        .get_variable("_cursed_entry_point")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| {
            steps.keys().next().cloned().unwrap_or_default()
        });
    let mut cycle: u64 = 0;

    loop {
        cycle += 1;
        memory.increment_cycle();

        if max_cycles > 0 && cycle > max_cycles {
            info!("max_cycles ({max_cycles}) reached, stopping pipeline");
            memory.cycle_analytics.current_cycle = cycle - 1;
            memory.save()?;
            return Err(EngineError::MaxCyclesReached {
                limit: max_cycles,
                actual: cycle - 1,
            });
        }

        let step = steps.get(&current_step_name).ok_or_else(|| {
            EngineError::StepNotFound(current_step_name.clone())
        })?;

        let msg = format!("[{cycle}] {}", step.name);
        spinner.set_message(msg);

        let outcome = execute_step(step, memory).await;

        let next = match &outcome {
            StepOutcome::Success => &step.on_success,
            StepOutcome::Failed => {
                if cycle as u32 <= step.max_retries {
                    &step.on_retry
                } else {
                    &step.on_failure
                }
            }
        };

        let transition = if next.is_empty() || next == "exit" || next == "done" {
            break;
        } else {
            next.clone()
        };

        if !steps.contains_key(&transition) {
            memory.save()?;
            return Err(EngineError::StepNotFound(transition));
        }

        current_step_name = transition;
        memory.record_step(matches!(&outcome, StepOutcome::Success));
        memory.save()?;
    }

    memory.cycle_analytics.current_cycle = cycle;
    memory.record_step(true);
    memory.save()?;
    spinner.finish_success();
    info!("Pipeline finished after {cycle} cycles");
    Ok(())
}

async fn execute_step(step: &Step, memory: &mut Memory) -> StepOutcome {
    info!("Executing step: {} ({:?})", step.name, step.action_type);

    match step.action_type {
        ActionType::LlmCompletion => {
            if step.prompt.is_empty() {
                info!("Step '{}': no prompt configured, skipping", step.name);
                return StepOutcome::Success;
            }
            let prompt_path = workspace_dir_for(memory).join(&step.prompt);
            match fs::read_to_string(&prompt_path) {
                Ok(prompt_content) => {
                    info!("Step '{}': LLM prompt loaded ({} bytes)", step.name, prompt_content.len());
                    StepOutcome::Success
                }
                Err(e) => {
                    info!("Step '{}': failed to read prompt: {e}", step.name);
                    StepOutcome::Failed
                }
            }
        }
        ActionType::ShellCommand => {
            if step.command.is_empty() {
                info!("Step '{}': no command configured, skipping", step.name);
                return StepOutcome::Success;
            }
            info!("Step '{}': shell command execution (placeholder)", step.name);
            StepOutcome::Success
        }
    }
}

fn workspace_dir_for(memory: &Memory) -> std::path::PathBuf {
    memory
        .get_variable("_cursed_workspace")
        .and_then(|v| v.as_str().map(std::path::PathBuf::from))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}
