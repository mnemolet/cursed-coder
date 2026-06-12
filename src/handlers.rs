use crate::memory::Memory;
use std::path::Path;
use std::process::Output;
use tracing::{error, info};

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
pub async fn execute_llm_completion(
    provider: &str,
    model: &str,
    prompt_template: &str,
    output_variable_key: &str,
    memory: &mut Memory,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let resolved = resolve_template(prompt_template, memory)?;

    info!("Sending LLM completion to {provider}/{model}");

    let response = crate::api::send_completion(provider, model, &resolved).await?;

    memory.set_variable(output_variable_key, serde_json::Value::String(response));

    memory.add_tokens(0, 0.0);
    memory.record_step(true);
    memory.increment_cycle();

    info!("LLM completion stored under '{output_variable_key}'");
    Ok(())
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
