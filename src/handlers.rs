use crate::api::{CompletionResponse, OpenRouterMessage};
use crate::memory::Memory;
use crate::tools;
use std::path::Path;
use std::process::Output;
use tracing::{error, info, warn};

const MAX_TOOL_ITERATIONS: usize = 10;

/// Substitutes `{variable_name}` placeholders in `template` with values
/// from `memory.cross_step_variables`. Returns an error if a referenced
/// variable is missing from the memory store.
pub fn resolve_template(
    template: &str,
    memory: &Memory,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut result = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        result.push_str(&rest[..open]);
        let after_open = &rest[open + 1..];
        let close = after_open
            .find('}')
            .ok_or_else(|| format!("unclosed placeholder in prompt template: {template}"))?;
        let var_name = &after_open[..close];

        let value = memory
            .get_variable(var_name)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .ok_or_else(|| {
                format!(
                    "placeholder '{{{var_name}}}' references missing variable '{var_name}' \
                     in memory.cross_step_variables"
                )
            })?;

        result.push_str(&value);
        rest = &after_open[close + 1..];
    }

    result.push_str(rest);
    Ok(result)
}

/// Executes an LLM completion step: resolves the prompt template,
/// dispatches to `api::send_completion`, and stores the result into
/// `memory.cross_step_variables` under `output_variable_key`.
///
/// If project state exists, it is prepended to the prompt with
/// instructions for the LLM to output state updates. Any state
/// update block (`<!-- STATE_UPDATE:{ ... } -->`) in the response
/// is parsed and applied to memory before returning.
///
/// Returns the cleaned response text on success.
pub async fn execute_llm_completion(
    provider: &str,
    model: &str,
    prompt_template: &str,
    output_variable_key: &str,
    memory: &mut Memory,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let resolved = resolve_template(prompt_template, memory)?;

    let prompt = match memory.build_project_context() {
        Some(ctx) => {
            format!(
                "{ctx}\n\n---\n\n{resolved}\n\n---\n\n\
                 If you learned something new about the project during this step, \
                 output a state update block at the very end of your response in \
                 this exact format (no other text around it):\n\
                 <!-- STATE_UPDATE:{{\"summary\":\"...\",\"current_focus\":\"...\",\
                 \"completed_milestones\":[\"...\"],\"blockers\":[\"...\"]}} -->\n\
                 Only include fields that changed. Omit fields that didn't change."
            )
        }
        None => {
            format!(
                "{resolved}\n\n---\n\n\
                 If you learned something new about the project during this step, \
                 output a state update block at the very end of your response in \
                 this exact format (no other text around it):\n\
                 <!-- STATE_UPDATE:{{\"summary\":\"...\",\"current_focus\":\"...\",\
                 \"completed_milestones\":[\"...\"],\"blockers\":[\"...\"]}} -->\n\
                 Only include fields that changed. Omit fields that didn't change."
            )
        }
    };

    info!("Sending LLM completion to {provider}/{model}");

    let response = crate::api::send_completion(provider, model, &prompt).await?;

    let cleaned = match Memory::parse_state_update(&response) {
        Some((update, cleaned)) => {
            memory.apply_state_update(&update);
            cleaned
        }
        None => response,
    };

    memory.set_variable(
        output_variable_key,
        serde_json::Value::String(cleaned.clone()),
    );

    info!("LLM completion stored under '{output_variable_key}'");
    Ok(cleaned)
}

/// Executes an LLM completion step with native tool calling.
///
/// Sends the prompt to the LLM with built-in tool definitions.
/// When the model returns tool calls, they are executed locally
/// and the results are sent back in a multi-turn loop until the
/// model responds with plain text (or the iteration limit is hit).
///
/// Returns the final text response on success.
pub async fn execute_llm_with_tools(
    provider: &str,
    model: &str,
    prompt_template: &str,
    output_variable_key: &str,
    memory: &mut Memory,
    workspace_dir: &Path,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let resolved = resolve_template(prompt_template, memory)?;

    let user_prompt = match memory.build_project_context() {
        Some(ctx) => format!("{ctx}\n\n---\n\n{resolved}"),
        None => resolved,
    };

    let tools = tools::built_in_tools();

    let mut messages = vec![
        OpenRouterMessage {
            role: "system".to_string(),
            content: Some(
                "You are a helpful coding assistant with access to shell execution tools. \
                 Use the execute_shell tool to run commands when needed. \
                 After completing your task, provide a text summary of what you did."
                    .to_string(),
            ),
            tool_calls: None,
            tool_call_id: None,
        },
        OpenRouterMessage {
            role: "user".to_string(),
            content: Some(user_prompt),
            tool_calls: None,
            tool_call_id: None,
        },
    ];

    info!("Sending LLM completion with tools to {provider}/{model}");

    for iteration in 0..MAX_TOOL_ITERATIONS {
        let response =
            crate::api::send_completion_with_tools(provider, model, &messages, &tools).await?;

        match response {
            CompletionResponse::Text(text) => {
                info!(
                    "LLM returned text after {iteration} tool iteration(s) ({} chars)",
                    text.len()
                );

                let cleaned = match Memory::parse_state_update(&text) {
                    Some((update, cleaned)) => {
                        memory.apply_state_update(&update);
                        cleaned
                    }
                    None => text,
                };

                memory.set_variable(
                    output_variable_key,
                    serde_json::Value::String(cleaned.clone()),
                );
                info!("LLM completion stored under '{output_variable_key}'");
                return Ok(cleaned);
            }
            CompletionResponse::ToolCalls(calls) => {
                info!(
                    "LLM requested {} tool call(s) (iteration {}/{MAX_TOOL_ITERATIONS})",
                    calls.len(),
                    iteration + 1
                );

                // Append assistant message with tool_calls
                let assistant_tool_calls: Vec<crate::api::OpenRouterToolCall> = calls
                    .iter()
                    .map(|tc| crate::api::OpenRouterToolCall {
                        id: tc.id.clone(),
                        call_type: "function".to_string(),
                        function: crate::api::OpenRouterFunctionCall {
                            name: tc.name.clone(),
                            arguments: serde_json::to_string(&tc.arguments).unwrap_or_default(),
                        },
                    })
                    .collect();

                messages.push(OpenRouterMessage {
                    role: "assistant".to_string(),
                    content: None,
                    tool_calls: Some(assistant_tool_calls),
                    tool_call_id: None,
                });

                // Execute each tool call and append results
                for tc in &calls {
                    let result = tools::execute_tool_call(tc, workspace_dir);

                    if result.is_error {
                        warn!("Tool '{}' failed: {}", tc.name, result.content);
                    } else {
                        info!(
                            "Tool '{}' succeeded ({} chars)",
                            tc.name,
                            result.content.len()
                        );
                    }

                    messages.push(OpenRouterMessage {
                        role: "tool".to_string(),
                        content: Some(result.content),
                        tool_calls: None,
                        tool_call_id: Some(result.tool_call_id),
                    });
                }
            }
        }
    }

    warn!("Tool calling loop hit max iterations ({MAX_TOOL_ITERATIONS})");
    Err(
        format!("LLM tool calling loop did not complete after {MAX_TOOL_ITERATIONS} iterations")
            .into(),
    )
}

/// Executes a shell command within the `workspace_path` directory.
///
/// Uses `cmd.exe /C` on Windows and `sh -c` on Unix. Returns the
/// process `Output` on success; a non-zero exit status is propagated
/// as an error with the captured stderr content.
pub fn execute_shell_command(
    command_string: &str,
    workspace_path: &Path,
) -> Result<Output, Box<dyn std::error::Error + Send + Sync>> {
    info!("Executing shell command: {command_string}");

    let output = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd.exe")
            .args(["/C", command_string])
            .current_dir(workspace_path)
            .output()?
    } else {
        std::process::Command::new("sh")
            .args(["-c", command_string])
            .current_dir(workspace_path)
            .output()?
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);
        error!("Shell command failed (exit code {exit_code}): {command_string}\nstderr: {stderr}");
        return Err(format!("shell command exited with code {exit_code}: {stderr}").into());
    }

    info!("Shell command completed successfully (exit code 0)");
    Ok(output)
}

const CODE_FENCE_START: &str = "```";
const CODE_LANGS: &[&str] = &["bash", "sh", "shell"];

/// Scans `text` for fenced code blocks with a shell language tag
/// (```` ```bash ````, ```` ```sh ````, ```` ```shell ````) and executes each
/// block body as a shell command via [`execute_shell_command`].
///
/// Blocks are executed in the order they appear. Returns `Ok(combined_stdout)`
/// if all blocks succeed, or the first error.
pub fn extract_and_execute_code_blocks(
    text: &str,
    workspace_path: &Path,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut combined = String::new();
    let mut rest = text;

    while let Some(i) = rest.find(CODE_FENCE_START) {
        let after_fence = &rest[i + CODE_FENCE_START.len()..];

        let lang_end = after_fence
            .find('\n')
            .map(|j| i + CODE_FENCE_START.len() + j)
            .unwrap_or(rest.len());
        let lang_line = &rest[i + CODE_FENCE_START.len()..lang_end];
        let lang = lang_line.trim();

        if !CODE_LANGS.contains(&lang) {
            rest = &rest[lang_end..];
            continue;
        }

        let after_lang = &rest[lang_end..];
        let content_start = if after_lang.starts_with('\n') { 1 } else { 0 };
        let body = &after_lang[content_start..];

        let close = body
            .find(CODE_FENCE_START)
            .ok_or_else(|| format!("unclosed code fence for {lang} block"))?;

        let command = &body[..close];
        let trimmed = command.trim();

        if !trimmed.is_empty() {
            info!("Executing extracted {lang} block ({})", trimmed.len());
            match execute_shell_command(trimmed, workspace_path) {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if !stdout.is_empty() {
                        combined.push_str(&stdout);
                        if !combined.ends_with('\n') {
                            combined.push('\n');
                        }
                    }
                }
                Err(e) => {
                    warn!("Extracted {lang} block failed: {e}");
                    return Err(e);
                }
            }
        }

        let block_end = lang_end + content_start + close + CODE_FENCE_START.len();
        rest = &rest[block_end..];
    }

    Ok(combined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_single_bash_block() {
        let text = r#"Some text before
```bash
echo hello
```
some after"#;
        let dir = tempfile::tempdir().unwrap();
        let result = extract_and_execute_code_blocks(text, dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("hello"));
    }

    #[test]
    fn test_extract_skips_non_shell_blocks() {
        let text = r#"```python
print("hi")
```
```sh
echo world
```"#;
        let dir = tempfile::tempdir().unwrap();
        let result = extract_and_execute_code_blocks(text, dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("world"));
    }

    #[test]
    fn test_extract_no_blocks() {
        let text = "just plain text";
        let dir = tempfile::tempdir().unwrap();
        let result = extract_and_execute_code_blocks(text, dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_extract_unclosed_fence_errors() {
        let text = r#"```bash
echo broken"#;
        let dir = tempfile::tempdir().unwrap();
        let result = extract_and_execute_code_blocks(text, dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_failed_command_errors() {
        let text = r#"```bash
exit 42
```"#;
        let dir = tempfile::tempdir().unwrap();
        let result = extract_and_execute_code_blocks(text, dir.path());
        assert!(result.is_err());
    }
}
