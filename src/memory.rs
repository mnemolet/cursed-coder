use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

#[derive(Debug)]
pub enum MemoryError {
    Io(io::Error),
    Serde(serde_json::Error),
    Poisoned(String),
}

impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryError::Io(e) => write!(f, "I/O error: {e}"),
            MemoryError::Serde(e) => write!(f, "serialization error: {e}"),
            MemoryError::Poisoned(msg) => write!(f, "poisoned lock: {msg}"),
        }
    }
}

impl std::error::Error for MemoryError {}

impl From<io::Error> for MemoryError {
    fn from(e: io::Error) -> Self {
        MemoryError::Io(e)
    }
}

impl From<serde_json::Error> for MemoryError {
    fn from(e: serde_json::Error) -> Self {
        MemoryError::Serde(e)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryMetadata {
    pub session_id: String,
    pub initialized_at: String,
    pub last_updated_at: String,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct CycleAnalytics {
    pub current_cycle: u64,
    pub total_tokens_consumed: u64,
    pub estimated_cost_usd: f64,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct Metrics {
    pub successful_steps: u32,
    pub failed_steps: u32,
    pub backtrack_counts: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepOutcomeRecord {
    pub cycle: u64,
    pub step_name: String,
    pub status: String,
    pub tokens_consumed: u64,
    pub cost_usd: f64,
    pub transition: String,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct ProjectState {
    pub summary: String,
    #[serde(default)]
    pub completed_milestones: Vec<String>,
    #[serde(default)]
    pub current_focus: String,
    #[serde(default)]
    pub blockers: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Memory {
    pub metadata: MemoryMetadata,
    #[serde(default)]
    pub cross_step_variables: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub cycle_analytics: CycleAnalytics,
    #[serde(default)]
    pub metrics: Metrics,
    #[serde(default)]
    pub step_outcomes: Vec<StepOutcomeRecord>,
    #[serde(default)]
    pub project_state: ProjectState,
    #[serde(skip)]
    dir: PathBuf,
}

fn generate_session_id() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let time_part = dur.as_nanos();
    let random_part: u64 = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        time_part.hash(&mut hasher);
        let now = SystemTime::now();
        now.hash(&mut hasher);
        hasher.finish()
    };
    format!("{:016x}{:016x}", time_part, random_part)
}

fn timestamp_iso() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = dur.as_secs();
    let frac_millis = dur.subsec_millis();

    let days = total_secs / 86400;
    let time_secs = total_secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    let (year, month, day) = civil_from_days(days as i64);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{frac_millis:03}Z")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

/// A parsed state update extracted from an LLM response.
#[derive(Deserialize, Debug, Clone)]
pub struct StateUpdate {
    pub summary: Option<String>,
    pub completed_milestones: Option<Vec<String>>,
    pub current_focus: Option<String>,
    pub blockers: Option<Vec<String>>,
}

/// Marker that wraps a JSON state update block in LLM responses.
pub const STATE_UPDATE_START: &str = "<!-- STATE_UPDATE:";
pub const STATE_UPDATE_END: &str = "-->";

impl Memory {
    pub fn load_or_create(dir: &Path) -> Result<Self, MemoryError> {
        fs::create_dir_all(dir)?;
        let path = dir.join("memory.json");

        if path.exists() {
            let file = fs::File::open(&path)?;
            let mut mem: Memory = serde_json::from_reader(file)?;
            mem.dir = dir.to_path_buf();
            mem.metadata.last_updated_at = timestamp_iso();
            Ok(mem)
        } else {
            let now = timestamp_iso();
            let mut mem = Memory {
                metadata: MemoryMetadata {
                    session_id: generate_session_id(),
                    initialized_at: now.clone(),
                    last_updated_at: now,
                },
                cross_step_variables: HashMap::new(),
                cycle_analytics: CycleAnalytics {
                    current_cycle: 0,
                    total_tokens_consumed: 0,
                    estimated_cost_usd: 0.0,
                },
                metrics: Metrics {
                    successful_steps: 0,
                    failed_steps: 0,
                    backtrack_counts: 0,
                },
                step_outcomes: Vec::new(),
                project_state: ProjectState::default(),
                dir: dir.to_path_buf(),
            };
            mem.save()?;
            Ok(mem)
        }
    }

    pub fn set_variable(&mut self, key: &str, value: serde_json::Value) {
        self.cross_step_variables.insert(key.to_string(), value);
    }

    pub fn get_variable(&self, key: &str) -> Option<&serde_json::Value> {
        self.cross_step_variables.get(key)
    }

    pub fn increment_cycle(&mut self) {
        self.cycle_analytics.current_cycle += 1;
    }

    pub fn record_step(&mut self, success: bool) {
        if success {
            self.metrics.successful_steps += 1;
        } else {
            self.metrics.failed_steps += 1;
        }
    }

    pub fn push_outcome(&mut self, record: StepOutcomeRecord) {
        self.step_outcomes.push(record);
    }

    pub fn add_tokens(&mut self, tokens: u64, cost: f64) {
        self.cycle_analytics.total_tokens_consumed += tokens;
        self.cycle_analytics.estimated_cost_usd += cost;
    }

    pub fn save(&mut self) -> Result<(), MemoryError> {
        self.metadata.last_updated_at = timestamp_iso();
        let path = self.dir.join("memory.json");
        let tmp_path = self.dir.join("memory.json.tmp");
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&tmp_path, &json)?;
        fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    /// Builds a human-readable project context string for injection into
    /// LLM prompts. Returns `None` if the project state is empty.
    pub fn build_project_context(&self) -> Option<String> {
        let ps = &self.project_state;
        let has_content = !ps.summary.is_empty()
            || !ps.completed_milestones.is_empty()
            || !ps.current_focus.is_empty()
            || !ps.blockers.is_empty();

        if !has_content {
            return None;
        }

        let mut parts = Vec::new();

        if !ps.summary.is_empty() {
            parts.push(format!("Project: {}", ps.summary));
        }
        if !ps.completed_milestones.is_empty() {
            let list = ps
                .completed_milestones
                .iter()
                .map(|m| format!("- {m}"))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("Completed:\n{list}"));
        }
        if !ps.current_focus.is_empty() {
            parts.push(format!("Current focus: {}", ps.current_focus));
        }
        if !ps.blockers.is_empty() {
            let list = ps
                .blockers
                .iter()
                .map(|b| format!("- {b}"))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("Blockers:\n{list}"));
        }

        Some(parts.join("\n\n"))
    }

    /// Applies a parsed `StateUpdate` to the project state, merging
    /// completed milestones and replacing other fields.
    pub fn apply_state_update(&mut self, update: &StateUpdate) {
        if let Some(ref summary) = update.summary {
            self.project_state.summary = summary.clone();
        }
        if let Some(ref milestones) = update.completed_milestones {
            for m in milestones {
                if !self.project_state.completed_milestones.contains(m) {
                    self.project_state.completed_milestones.push(m.clone());
                }
            }
        }
        if let Some(ref focus) = update.current_focus {
            self.project_state.current_focus = focus.clone();
        }
        if let Some(ref blockers) = update.blockers {
            self.project_state.blockers = blockers.clone();
        }
        info!("Project state updated");
    }

    /// Parses a state update block from an LLM response.
    ///
    /// Looks for `<!-- STATE_UPDATE:{ ... } -->` in the response text.
    /// Returns `(parsed_update, cleaned_response)` where the cleaned
    /// response has the state update block removed.
    pub fn parse_state_update(response: &str) -> Option<(StateUpdate, String)> {
        let start = response.find(STATE_UPDATE_START)?;
        let json_start = start + STATE_UPDATE_START.len();
        let end = response[json_start..].find(STATE_UPDATE_END)?;
        let json_str = response[json_start..json_start + end].trim();

        let update: StateUpdate = match serde_json::from_str(json_str) {
            Ok(u) => u,
            Err(e) => {
                warn!("Failed to parse state update JSON: {e}");
                return None;
            }
        };

        let mut cleaned = String::with_capacity(response.len());
        cleaned.push_str(&response[..start]);
        cleaned.push_str(&response[json_start + end + STATE_UPDATE_END.len()..]);
        let cleaned = cleaned.trim().to_string();

        Some((update, cleaned))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_memory() -> Memory {
        let dir = tempfile::tempdir().unwrap();
        Memory::load_or_create(dir.path()).unwrap()
    }

    #[test]
    fn test_build_project_context_empty() {
        let mem = temp_memory();
        assert!(mem.build_project_context().is_none());
    }

    #[test]
    fn test_build_project_context_with_summary() {
        let mut mem = temp_memory();
        mem.project_state.summary = "A Rust CLI tool".to_string();
        let ctx = mem.build_project_context().unwrap();
        assert!(ctx.contains("A Rust CLI tool"));
    }

    #[test]
    fn test_build_project_context_full() {
        let mut mem = temp_memory();
        mem.project_state.summary = "Test project".to_string();
        mem.project_state.current_focus = "parsing".to_string();
        mem.project_state.completed_milestones = vec!["setup".to_string()];
        mem.project_state.blockers = vec!["missing dep".to_string()];
        let ctx = mem.build_project_context().unwrap();
        assert!(ctx.contains("Test project"));
        assert!(ctx.contains("parsing"));
        assert!(ctx.contains("- setup"));
        assert!(ctx.contains("- missing dep"));
    }

    #[test]
    fn test_apply_state_update_summary() {
        let mut mem = temp_memory();
        let update = StateUpdate {
            summary: Some("new summary".to_string()),
            completed_milestones: None,
            current_focus: None,
            blockers: None,
        };
        mem.apply_state_update(&update);
        assert_eq!(mem.project_state.summary, "new summary");
    }

    #[test]
    fn test_apply_state_update_merges_milestones() {
        let mut mem = temp_memory();
        mem.project_state
            .completed_milestones
            .push("existing".to_string());
        let update = StateUpdate {
            summary: None,
            completed_milestones: Some(vec!["new task".to_string(), "existing".to_string()]),
            current_focus: None,
            blockers: None,
        };
        mem.apply_state_update(&update);
        assert_eq!(mem.project_state.completed_milestones.len(), 2);
        assert!(
            mem.project_state
                .completed_milestones
                .contains(&"existing".to_string())
        );
        assert!(
            mem.project_state
                .completed_milestones
                .contains(&"new task".to_string())
        );
    }

    #[test]
    fn test_apply_state_update_replaces_blockers() {
        let mut mem = temp_memory();
        mem.project_state.blockers.push("old blocker".to_string());
        let update = StateUpdate {
            summary: None,
            completed_milestones: None,
            current_focus: None,
            blockers: Some(vec!["new blocker".to_string()]),
        };
        mem.apply_state_update(&update);
        assert_eq!(mem.project_state.blockers, vec!["new blocker"]);
    }

    #[test]
    fn test_parse_state_update_valid() {
        let response = "Here is what I did.\n\n<!-- STATE_UPDATE:{\"summary\":\"Test project\",\"current_focus\":\"parsing\"} -->";
        let (update, cleaned) = Memory::parse_state_update(response).unwrap();
        assert_eq!(update.summary.as_deref(), Some("Test project"));
        assert_eq!(update.current_focus.as_deref(), Some("parsing"));
        assert_eq!(cleaned, "Here is what I did.");
    }

    #[test]
    fn test_parse_state_update_no_block() {
        let response = "Just a plain response with no state update.";
        assert!(Memory::parse_state_update(response).is_none());
    }

    #[test]
    fn test_parse_state_update_invalid_json() {
        let response = "text <!-- STATE_UPDATE:not valid json --> more";
        assert!(Memory::parse_state_update(response).is_none());
    }

    #[test]
    fn test_parse_state_update_with_milestones() {
        let response = "Done!\n<!-- STATE_UPDATE:{\"completed_milestones\":[\"step1\",\"step2\"],\"summary\":\"proj\"} -->";
        let (update, cleaned) = Memory::parse_state_update(response).unwrap();
        assert_eq!(update.completed_milestones.unwrap(), vec!["step1", "step2"]);
        assert_eq!(cleaned, "Done!");
    }

    #[test]
    fn test_parse_state_update_preserves_surrounding_text() {
        let response = "Before\n<!-- STATE_UPDATE:{\"summary\":\"x\"} -->\nAfter";
        let (_, cleaned) = Memory::parse_state_update(response).unwrap();
        assert_eq!(cleaned, "Before\n\nAfter");
    }
}
