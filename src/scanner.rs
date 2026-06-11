use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tracing::warn;

pub enum WorkspaceMode {
    Invention,
    Iterative,
}

pub struct FileSource {
    pub relative_path: PathBuf,
    pub content: String,
}

pub struct WorkspaceContext {
    pub mode: WorkspaceMode,
    pub files: Vec<FileSource>,
}

const SKIP_DIRS: &[&str] = &[
    ".cursedcoder",
    ".git",
    ".venv",
    "build",
    "dist",
    "node_modules",
    "target",
];

const SKIP_EXTENSIONS: &[&str] = &[
    "bin", "dll", "dylib", "exe", "gif", "gz", "ico", "jpeg", "jpg", "lock", "mp4", "png", "so",
    "svg", "tar", "wasm", "webp", "zip",
];

const SKIP_FILENAMES: &[&str] = &[".env", ".env.local", ".gitignore"];

fn is_skip_dir(name: &str) -> bool {
    SKIP_DIRS.contains(&name)
}

fn is_skip_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SKIP_EXTENSIONS.contains(&ext))
        .unwrap_or(false)
}

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

fn scan_recursive(dir: &Path, base: &Path, files: &mut Vec<FileSource>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if path.is_dir() {
            if is_skip_dir(&name_str) {
                continue;
            }
            scan_recursive(&path, base, files)?;
        } else if path.is_file() {
            if SKIP_FILENAMES.contains(&name_str.as_ref()) {
                continue;
            }

            if is_skip_extension(&path) {
                continue;
            }

            if let Ok(metadata) = entry.metadata()
                && metadata.len() > MAX_FILE_SIZE
            {
                warn!(
                    "Skipping large file ({} bytes): {}",
                    metadata.len(),
                    path.display()
                );
                continue;
            }

            let relative_path = match path.strip_prefix(base) {
                Ok(rel) => rel.to_path_buf(),
                Err(_) => {
                    warn!("Skipping file outside base directory: {}", path.display());
                    continue;
                }
            };

            match fs::read_to_string(&path) {
                Ok(content) => {
                    files.push(FileSource {
                        relative_path,
                        content,
                    });
                }
                Err(_) => continue,
            }
        }
    }
    Ok(())
}

pub fn scan_workspace(dir: &Path) -> io::Result<WorkspaceContext> {
    let mut files = Vec::new();
    scan_recursive(dir, dir, &mut files)?;

    let mode = if files.is_empty() {
        WorkspaceMode::Invention
    } else {
        WorkspaceMode::Iterative
    };

    Ok(WorkspaceContext { mode, files })
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct LocalTask {
    pub id: usize,
    pub task: String,
    #[serde(default)]
    pub description: String,
    pub completed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskList {
    #[serde(default)]
    pub tasks: Vec<LocalTask>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    NoTaskFile,
    ActiveTask(LocalTask),
    AllTasksCompleted,
    InvalidFormat(String),
}

pub fn parse_active_task(workspace_dir: &Path) -> TaskStatus {
    let tasks_path = workspace_dir.join("tasks.toml");
    let content = match fs::read_to_string(&tasks_path) {
        Ok(c) => c,
        Err(_) => return TaskStatus::NoTaskFile,
    };
    let task_list: TaskList = match toml::from_str(&content) {
        Ok(list) => list,
        Err(e) => return TaskStatus::InvalidFormat(e.to_string()),
    };
    for task in task_list.tasks {
        if !task.completed {
            return TaskStatus::ActiveTask(task);
        }
    }
    TaskStatus::AllTasksCompleted
}

pub fn mark_task_completed(workspace_dir: &Path, task_id: usize) -> Result<(), String> {
    let tasks_path = workspace_dir.join("tasks.toml");
    let content =
        fs::read_to_string(&tasks_path).map_err(|e| format!("Failed to read tasks.toml: {}", e))?;
    let mut task_list: TaskList =
        toml::from_str(&content).map_err(|e| format!("Failed to parse tasks.toml: {}", e))?;
    let found = task_list.tasks.iter_mut().find(|t| t.id == task_id);
    match found {
        Some(task) => {
            task.completed = true;
        }
        None => return Err(format!("Task with id {} not found", task_id)),
    }
    let output =
        toml::to_string_pretty(&task_list).map_err(|e| format!("Failed to serialize: {}", e))?;
    fs::write(&tasks_path, &output).map_err(|e| format!("Failed to write tasks.toml: {}", e))?;
    Ok(())
}

pub fn format_workspace(ctx: &WorkspaceContext) -> String {
    match ctx.mode {
        WorkspaceMode::Invention => "[Empty Workspace]".to_string(),
        WorkspaceMode::Iterative => {
            let estimated_capacity = ctx
                .files
                .iter()
                .map(|f| f.relative_path.to_string_lossy().len() + f.content.len() + 32)
                .sum();
            let mut buf = String::with_capacity(estimated_capacity);

            for file in &ctx.files {
                use std::fmt::Write;
                let _ = writeln!(
                    buf,
                    "---\nFile: {}\n```\n{}",
                    file.relative_path.display(),
                    file.content
                );
                if !file.content.ends_with('\n') {
                    buf.push('\n');
                }
                buf.push_str("```\n\n");
            }
            buf
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Extension & directory blocklist
    // ------------------------------------------------------------------

    #[test]
    fn test_file_extension_blocklist() {
        // Allowed extensions (not in SKIP_EXTENSIONS)
        assert!(!is_skip_extension(Path::new("src/main.rs")));
        assert!(!is_skip_extension(Path::new("src/scanner.rs")));
        assert!(!is_skip_extension(Path::new("README.md")));
        assert!(!is_skip_extension(Path::new("Cargo.toml")));
        assert!(!is_skip_extension(Path::new("Makefile")));
        assert!(!is_skip_extension(Path::new(".env")));

        // Blocked extensions (in SKIP_EXTENSIONS)
        assert!(is_skip_extension(Path::new("binary_output.exe")));
        assert!(is_skip_extension(Path::new("logo.png")));
        assert!(is_skip_extension(Path::new("archive.zip")));
        assert!(is_skip_extension(Path::new("libfoo.so")));
        assert!(is_skip_extension(Path::new("archive.tar")));
        assert!(is_skip_extension(Path::new("image.jpg")));
        assert!(is_skip_extension(Path::new("image.jpeg")));
        assert!(is_skip_extension(Path::new("image.gif")));
        assert!(is_skip_extension(Path::new("image.svg")));
        assert!(is_skip_extension(Path::new("image.webp")));
        assert!(is_skip_extension(Path::new("image.ico")));
        assert!(is_skip_extension(Path::new("video.mp4")));
        assert!(is_skip_extension(Path::new("file.bin")));
        assert!(is_skip_extension(Path::new("file.dll")));
        assert!(is_skip_extension(Path::new("file.dylib")));
        assert!(is_skip_extension(Path::new("file.wasm")));
        assert!(is_skip_extension(Path::new("file.gz")));
        assert!(is_skip_extension(Path::new("Cargo.lock")));

        // Paths inside skip dirs are handled by is_skip_dir, not by extension
        assert!(!is_skip_extension(Path::new(".git/config")));
        assert!(!is_skip_extension(Path::new("target/debug/cursedcoder")));

        // Skipped directories
        assert!(is_skip_dir(".cursedcoder"));
        assert!(is_skip_dir(".git"));
        assert!(is_skip_dir(".venv"));
        assert!(is_skip_dir("build"));
        assert!(is_skip_dir("dist"));
        assert!(is_skip_dir("node_modules"));
        assert!(is_skip_dir("target"));

        // Non-skipped directories
        assert!(!is_skip_dir("src"));
        assert!(!is_skip_dir("assets"));
        assert!(!is_skip_dir("docs"));
        assert!(!is_skip_dir("tests"));
    }

    // ------------------------------------------------------------------
    // TOML task parsing
    // ------------------------------------------------------------------

    fn write_toml(workspace_dir: &Path, content: &str) {
        fs::write(workspace_dir.join("tasks.toml"), content).unwrap();
    }

    #[test]
    fn test_parse_active_task_returns_active() {
        let dir = tempfile::tempdir().unwrap();
        write_toml(
            dir.path(),
            r#"[[tasks]]
id = 1
task = "Implement JWT auth"
completed = false
description = """
Multi-line
description
"""
"#,
        );
        let status = parse_active_task(dir.path());
        match status {
            TaskStatus::ActiveTask(task) => {
                assert_eq!(task.id, 1);
                assert_eq!(task.task, "Implement JWT auth");
                assert!(!task.completed);
                assert_eq!(task.description, "Multi-line\ndescription\n");
            }
            other => panic!("Expected ActiveTask, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_active_task_skips_completed() {
        let dir = tempfile::tempdir().unwrap();
        write_toml(
            dir.path(),
            r#"[[tasks]]
id = 1
task = "Done task"
completed = true
description = ""

[[tasks]]
id = 2
task = "Pending task"
completed = false
description = ""
"#,
        );
        let status = parse_active_task(dir.path());
        match status {
            TaskStatus::ActiveTask(task) => {
                assert_eq!(task.id, 2);
                assert_eq!(task.task, "Pending task");
            }
            other => panic!("Expected ActiveTask for id=2, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_active_task_all_completed() {
        let dir = tempfile::tempdir().unwrap();
        write_toml(
            dir.path(),
            r#"[[tasks]]
id = 1
task = "Done"
completed = true
description = ""
"#,
        );
        assert_eq!(parse_active_task(dir.path()), TaskStatus::AllTasksCompleted);
    }

    #[test]
    fn test_parse_active_task_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        write_toml(dir.path(), "# no tasks\n");
        assert_eq!(parse_active_task(dir.path()), TaskStatus::AllTasksCompleted);
    }

    #[test]
    fn test_parse_active_task_no_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(parse_active_task(dir.path()), TaskStatus::NoTaskFile);
    }

    #[test]
    fn test_parse_active_task_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        write_toml(dir.path(), "this is not valid toml {{{");
        let status = parse_active_task(dir.path());
        match status {
            TaskStatus::InvalidFormat(msg) => {
                assert!(!msg.is_empty(), "Expected error message");
            }
            other => panic!("Expected InvalidFormat, got {:?}", other),
        }
    }

    #[test]
    fn test_mark_task_completed_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let toml_content = r#"[[tasks]]
id = 1
task = "Task one"
completed = false
description = ""

[[tasks]]
id = 2
task = "Task two"
completed = false
description = ""
"#;
        write_toml(dir.path(), toml_content);

        mark_task_completed(dir.path(), 1).unwrap();

        let updated = fs::read_to_string(dir.path().join("tasks.toml")).unwrap();
        assert!(updated.contains(r#"task = "Task one""#));
        assert!(updated.contains("completed = true"));
        // Second task still incomplete
        assert!(updated.contains(r#"completed = false"#));
    }

    #[test]
    fn test_mark_task_completed_id_not_found() {
        let dir = tempfile::tempdir().unwrap();
        write_toml(
            dir.path(),
            r#"[[tasks]]
id = 1
task = "Only task"
completed = false
description = ""
"#,
        );
        let result = mark_task_completed(dir.path(), 99);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_parse_active_task_missing_description_defaults_empty() {
        let dir = tempfile::tempdir().unwrap();
        write_toml(
            dir.path(),
            r#"[[tasks]]
id = 1
task = "No description"
completed = false
"#,
        );
        let status = parse_active_task(dir.path());
        match status {
            TaskStatus::ActiveTask(task) => {
                assert_eq!(task.description, "");
            }
            other => panic!("Expected ActiveTask, got {:?}", other),
        }
    }

    // ------------------------------------------------------------------
    // Workspace mode sorting
    // ------------------------------------------------------------------

    #[test]
    fn test_workspace_mode_sorting_invention_when_empty() {
        let ctx = WorkspaceContext {
            mode: WorkspaceMode::Invention,
            files: vec![],
        };
        assert_eq!(format_workspace(&ctx), "[Empty Workspace]");
    }

    #[test]
    fn test_workspace_mode_sorting_iterative_when_files_exist() {
        let ctx = WorkspaceContext {
            mode: WorkspaceMode::Iterative,
            files: vec![FileSource {
                relative_path: PathBuf::from("src/main.rs"),
                content: "fn main() {}".to_string(),
            }],
        };
        let output = format_workspace(&ctx);
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("fn main() {}"));
        assert!(output.starts_with("---"));
    }
}
