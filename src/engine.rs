use crate::handlers;
use crate::memory::{Memory, StepOutcomeRecord};
use crate::scanner;
use crate::spinner::EngineSpinner;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{info, warn};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    #[serde(alias = "llm_completion")]
    Llm,
    #[serde(alias = "shell_command")]
    Shell,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct StepPayload {
    #[serde(default)]
    pub command: String,
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
    pub payload: Option<StepPayload>,
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
    #[serde(default)]
    pub task_management_enabled: bool,
}

impl Step {
    pub fn effective_command(&self) -> &str {
        if !self.command.is_empty() {
            &self.command
        } else if let Some(payload) = &self.payload {
            payload.command.as_str()
        } else {
            ""
        }
    }
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
            GraphError::BrokenTransition {
                step,
                field,
                target,
            } => {
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
action_type = "llm"
prompt = ""
command = ""
enabled = true
on_success = ""
on_failure = ""
max_retries = 1
on_retry = ""
task_management_enabled = false
"#;

pub fn initialize_workspace(workspace_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let dot_dir = workspace_dir.join(".cursedcoder");
    fs::create_dir_all(&dot_dir)?;

    let steps_path = dot_dir.join("steps.toml");
    if !steps_path.exists() {
        fs::write(&steps_path, STEPS_TOML_TEMPLATE)?;
    }

    let tasks_path = dot_dir.join("tasks.toml");
    if !tasks_path.exists() {
        fs::write(&tasks_path, "")?;
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

/// Returns `true` if the steps.toml content matches the default template.
pub fn is_unconfigured_template(workspace_dir: &Path) -> bool {
    let path = workspace_dir.join(".cursedcoder").join("steps.toml");
    fs::read_to_string(&path)
        .map(|content| content.trim() == STEPS_TOML_TEMPLATE.trim())
        .unwrap_or(false)
}

#[derive(Debug)]
pub enum EngineError {
    StepNotFound(String),
    MaxCyclesReached { limit: u64, actual: u64 },
    MemoryCorruption(String),
    UnconfiguredTemplate,
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
            EngineError::MemoryCorruption(msg) => {
                write!(f, "memory corruption detected: {msg}")
            }
            EngineError::UnconfiguredTemplate => {
                write!(
                    f,
                    "Error: Workspace contains an unconfigured pipeline template.\n\
                     [Cause]: The '.cursedcoder/steps.toml' file contains only the default placeholder steps.\n\
                     [Fix]: Please edit '.cursedcoder/steps.toml' to define your actual project build, test, \
                     or deployment steps before launching the autonomous agent."
                )
            }
            EngineError::Memory(e) => write!(f, "memory error: {e}"),
            EngineError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<crate::memory::MemoryError> for EngineError {
    fn from(e: crate::memory::MemoryError) -> Self {
        match e {
            crate::memory::MemoryError::Serde(err) => {
                EngineError::MemoryCorruption(err.to_string())
            }
            other => EngineError::Memory(other),
        }
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
    TaskDrivenSuccess,
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
    workspace_dir: &Path,
) -> Result<(), EngineError> {
    let spinner = EngineSpinner::new();
    let mut current_step_name = memory
        .get_variable("_cursed_entry_point")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| steps.keys().next().cloned().unwrap_or_default());
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

        let step = steps
            .get(&current_step_name)
            .ok_or_else(|| EngineError::StepNotFound(current_step_name.clone()))?;

        let msg = format!("[{cycle}] {}", step.name);
        spinner.set_message(msg);

        let outcome = execute_step(step, memory, workspace_dir).await;

        let (next, status_str) = match &outcome {
            StepOutcome::TaskDrivenSuccess | StepOutcome::Success => (&step.on_success, "success"),
            StepOutcome::Failed => {
                if cycle as u32 <= step.max_retries {
                    (&step.on_retry, "retry")
                } else {
                    (&step.on_failure, "failed")
                }
            }
        };

        let is_terminal = next.is_empty() || next == "exit" || next == "done";
        let transition = if is_terminal {
            String::new()
        } else {
            next.clone()
        };

        if !transition.is_empty() && !steps.contains_key(&transition) {
            memory.save()?;
            return Err(EngineError::StepNotFound(transition));
        }

        memory.record_step(matches!(
            &outcome,
            StepOutcome::Success | StepOutcome::TaskDrivenSuccess
        ));

        if matches!(&outcome, StepOutcome::Failed) {
            memory.metrics.backtrack_counts += 1;
        }

        memory.push_outcome(StepOutcomeRecord {
            cycle,
            step_name: step.name.clone(),
            status: status_str.to_string(),
            tokens_consumed: memory.cycle_analytics.total_tokens_consumed,
            cost_usd: memory.cycle_analytics.estimated_cost_usd,
            transition: transition.clone(),
        });

        memory.save()?;

        if is_terminal {
            break;
        }

        current_step_name = transition;
    }

    memory.cycle_analytics.current_cycle = cycle;
    memory.record_step(true);
    memory.save()?;
    spinner.finish_success();
    info!("Pipeline finished after {cycle} cycles");
    Ok(())
}

async fn execute_step(step: &Step, memory: &mut Memory, workspace_dir: &Path) -> StepOutcome {
    info!("Executing step: {} ({:?})", step.name, step.action_type);

    if step.task_management_enabled {
        return handle_task_driven_step(step, memory, workspace_dir);
    }

    let outcome = match step.action_type {
        ActionType::Llm => {
            if step.prompt.is_empty() {
                info!("Step '{}': no prompt configured, skipping", step.name);
                StepOutcome::Success
            } else {
                let prompt_path = workspace_dir.join(&step.prompt);
                match fs::read_to_string(&prompt_path) {
                    Ok(content) => {
                        info!(
                            "Step '{}': LLM prompt loaded ({} bytes)",
                            step.name,
                            content.len()
                        );
                        let simulated_tokens = content.len() as u64 / 4;
                        let simulated_cost = simulated_tokens as f64 * 0.000_002;
                        memory.add_tokens(simulated_tokens, simulated_cost);
                        StepOutcome::Success
                    }
                    Err(e) => {
                        warn!("Step '{}': failed to read prompt: {e}", step.name);
                        StepOutcome::Failed
                    }
                }
            }
        }
        ActionType::Shell => {
            if step.effective_command().is_empty() {
                info!("Step '{}': no command configured, skipping", step.name);
                StepOutcome::Success
            } else {
                match handlers::execute_shell_command(step.effective_command(), workspace_dir) {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        if !stdout.is_empty() {
                            info!("Step '{}': stdout — {stdout}", step.name);
                        }
                        memory.set_variable(
                            &format!("_{}_stdout", step.name.replace(' ', "_")),
                            serde_json::Value::String(stdout.to_string()),
                        );
                        StepOutcome::Success
                    }
                    Err(e) => {
                        warn!("Step '{}': shell command failed: {e}", step.name);
                        StepOutcome::Failed
                    }
                }
            }
        }
    };

    outcome
}

fn handle_task_driven_step(step: &Step, memory: &mut Memory, workspace_dir: &Path) -> StepOutcome {
    info!("Step '{}': task-driven mode enabled", step.name);

    match scanner::parse_active_task(workspace_dir) {
        scanner::TaskStatus::ActiveTask(task) => {
            info!(
                "Step '{}': reconciling with active task {} — {}",
                step.name, task.id, task.task
            );
            memory.set_variable(
                "_active_task_id",
                serde_json::Value::Number(serde_json::Number::from(task.id)),
            );
            memory.set_variable(
                "_active_task_description",
                serde_json::Value::String(task.task),
            );

            let outcome = match step.action_type {
                ActionType::Llm => StepOutcome::TaskDrivenSuccess,
                ActionType::Shell => {
                    if step.effective_command().is_empty() {
                        StepOutcome::TaskDrivenSuccess
                    } else {
                        match handlers::execute_shell_command(
                            step.effective_command(),
                            workspace_dir,
                        ) {
                            Ok(_) => StepOutcome::TaskDrivenSuccess,
                            Err(e) => {
                                warn!("Step '{}': task shell command failed: {e}", step.name);
                                StepOutcome::Failed
                            }
                        }
                    }
                }
            };

            if let Err(e) = scanner::mark_task_completed(workspace_dir, task.id) {
                warn!(
                    "Step '{}': failed to mark task {} complete: {e}",
                    step.name, task.id
                );
            } else {
                info!("Step '{}': marked task {} complete", step.name, task.id);
            }

            outcome
        }
        scanner::TaskStatus::AllTasksCompleted => {
            info!("Step '{}': all tasks already completed", step.name);
            StepOutcome::TaskDrivenSuccess
        }
        scanner::TaskStatus::NoTaskFile => {
            info!(
                "Step '{}': no tasks.toml found, falling back to standard mode",
                step.name
            );
            StepOutcome::Success
        }
        scanner::TaskStatus::InvalidFormat(e) => {
            warn!("Step '{}': tasks.toml has invalid format: {e}", step.name);
            StepOutcome::Failed
        }
    }
}
