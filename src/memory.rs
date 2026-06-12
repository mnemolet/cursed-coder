use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
}
