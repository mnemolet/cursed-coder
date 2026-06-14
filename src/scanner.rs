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
