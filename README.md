# cursed-coder

An autonomous AI coding agent that operates from your terminal. It reads a step-graph pipeline from `steps.toml`, executes LLM completions and shell commands, and iterates across cycles until the task is complete.

## Overview

cursed-coder is a CLI tool that bridges large language models with your local development environment. You define a pipeline of steps (LLM prompts or shell commands) in a simple TOML file, and the agent executes them in order, following success/failure/retry transitions between steps. It remembers context across cycles through a persistent memory system, so the agent builds understanding of your project over time.

Key capabilities:

- **Step-graph engine** — Define multi-step pipelines with conditional transitions
- **LLM integration** — Sends prompts to OpenRouter, Gemini, Ollama, or a mock provider
- **Shell execution** — Runs commands in your workspace with automatic code block extraction
- **Persistent memory** — Retains project state, variables, and analytics across cycles
- **Human-in-the-loop** — Requires explicit consent before executing in a workspace
- **Cross-platform** — Works on Linux, macOS, and Windows

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| clap | 4.6.1 | CLI argument parsing |
| dirs | 6.0.0 | Platform config directory resolution |
| dotenvy | 0.15.7 | `.env` file loading |
| indicatif | 0.18.4 | Terminal progress spinners |
| reqwest | 0.13.3 | HTTP client for LLM API calls |
| serde / serde_json | 1.0.228 / 1.0.149 | JSON and TOML serialization |
| tempfile | 3.27.0 | Temporary file handling (tests) |
| tokio | 1.52.3 | Async runtime |
| toml | 1.1.2 | TOML parsing for steps configuration |
| tracing / tracing-subscriber | 0.1.44 / 0.3.23 | Structured logging with file output |

## Installation

### From source

Requires Rust 1.85+ (edition 2024).

```bash
git clone https://github.com/your-org/cursed-coder.git
cd cursed-coder
cargo build --release
```

The binary is available at `target/release/cursedcoder`.

### Pre-built binaries

Download from the [Releases](https://github.com/your-org/cursed-coder/releases) page. Binaries are available for:

- Linux (x86_64)
- macOS (x86_64, arm64)
- Windows (x86_64)

## Quick Start

```bash
# Initialize a workspace
cursedcoder init

# Edit .cursedcoder/steps.toml to define your pipeline
# Then run the agent (skip confirmation with -y)
cursedcoder -y -c 5
```

## Usage

```
cursedcoder [OPTIONS] [COMMAND]

Commands:
  init      Initialize the local workspace for cursed-coder

Options:
  -c, --cycles <CYCLES>   Maximum number of execution cycles (0 = infinite)
  -y, --yes               Skip startup confirmation and begin immediately
  -h, --help              Print help
  -V, --version           Print version
```

### Workspace Structure

After `cursedcoder init`, your workspace contains:

```
.cursedcoder/
  steps.toml      # Pipeline definition (edit this)
  memory.json     # Persistent agent memory (auto-managed)
```

### Steps Configuration

Edit `.cursedcoder/steps.toml` to define your pipeline. Each step has an action type, optional prompt/command, and transition rules:

```toml
[[step]]
name = "Analyze"
description = "Analyze the codebase"
action_type = "llm"
prompt = "Analyze the project structure and identify what needs to be done"
command = ""
enabled = true
on_success = "Build"
on_failure = ""
max_retries = 1
on_retry = ""

[[step]]
name = "Build"
description = "Build the project"
action_type = "shell"
prompt = ""
command = "cargo build"
enabled = true
on_success = ""
on_failure = ""
max_retries = 1
on_retry = ""
```

### Action Types

- **`llm`** — Sends the prompt to the configured LLM provider. Supports inline text or file paths.
- **`shell`** — Executes the command in the workspace directory. Non-zero exit codes are treated as failures.

### LLM Response Code Blocks

When the LLM returns fenced code blocks with shell language tags, they are automatically extracted and executed:

````markdown
Here is the fix:

```bash
cargo fmt
```
````

### Prompt Templates

Prompts support `{variable}` substitution from memory. Variables are set by the engine after each step:

```toml
prompt = "Continue working on {_active_task}. Previous output: {_Analyze_response}"
```

### Environment Variables

Configure the LLM provider in `.env` (created by `init`):

```bash
# OpenRouter
CURSED_PROVIDER=openrouter
CURSED_MODEL=anthropic/claude-sonnet-4
OPENROUTER_API_KEY=sk-or-...

# Gemini
CURSED_PROVIDER=gemini
CURSED_MODEL=gemini-2.5-flash
GEMINI_API_KEY=AIza...

# Ollama (local)
CURSED_PROVIDER=ollama
CURSED_MODEL=codellama
```

### Logging

Logs are written to `~/.config/cursedcoder/<timestamp>.log`. Configure the log level in `config.json`:

```json
{
  "log_level": "info"
}
```

Set `RUST_LOG` to override: `RUST_LOG=debug cursedcoder -y`.

## Testing

```bash
# Run all tests
cargo test

# Run with clippy lints
cargo clippy --all-targets -- -D warnings

# Check formatting
cargo fmt --all -- --check
```

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feat/my-feature`
3. Make your changes following the conventions below
4. Ensure all checks pass:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```
5. Commit with a descriptive message following [Conventional Commits](https://www.conventionalcommits.org/)
6. Open a pull request with a meaningful title and description

### Commit Convention

Use [Conventional Commits](https://www.conventionalcommits.org/) format:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

Types: `feat`, `fix`, `ci`, `docs`, `refactor`, `test`, `chore`

Examples:
- `feat(engine): add cycle counting per graph traversal`
- `fix(telemetry): apply log_level filter to file output`
- `ci: add GitHub Actions workflow for format, lint and test`
- `docs: add README with installation and usage instructions`

### Branch Naming

Follow conventional branch naming:

- `feat/*` — New features
- `fix/*` — Bug fixes
- `ci/*` — CI/CD changes
- `docs/*` — Documentation
- `refactor/*` — Code refactoring

### Pull Requests

PRs should include:

- **Descriptive title** — e.g., "Add persistent project state to memory"
- **Summary** — What changed and why
- **Testing** — How the changes were verified
- **Checklist** — `cargo fmt`, `clippy`, `tests` all passing

## Roadmap

### In Progress
- [ ] LLM tool calling — Replace code-block parsing with native function calling for more reliable structured output
- [ ] File edit action — Dedicated `edit` action type with diff-based file modifications
- [ ] Git integration — Branch, commit, and push from within the pipeline

### Planned
- [ ] MCP client — Connect to external MCP servers (GitHub, Playwright, databases, search) as pipeline actions
- [ ] TUI dashboard — Real-time terminal UI showing pipeline progress, step outputs, and cost (ratatui)
- [ ] Parallel step execution — Run independent steps concurrently within a cycle
- [ ] Custom actions — User-defined action types via Python/Shell scripts
- [ ] Auto-discovery — Analyze workspace and suggest a steps.toml pipeline
- [ ] Dry-run mode — Preview pipeline execution without running LLM calls or commands

### Under Consideration
- [ ] MCP server — Expose cursed-coder capabilities (run pipeline, query memory, execute steps) to external tools (Claude Desktop, VS Code, etc.)
- [ ] Plugin system — Extend with community-contributed LLM providers and action types
- [ ] Webhook notifications — Send step completion/failure events to Slack, Discord, etc.
- [ ] Cost budgeting — Set per-cycle or per-session cost limits with automatic cutoff
- [ ] Multi-workspace support — Run pipelines across multiple repositories

## License

Apache 2.0 — see [LICENSE](LICENSE) for details.
