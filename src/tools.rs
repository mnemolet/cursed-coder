use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{error, info};

/// A tool definition that can be sent to an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

/// Function definition inside a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A tool call returned by the LLM.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Result of executing a tool call.
#[derive(Debug, Clone)]
pub struct ToolCallResult {
    pub tool_call_id: String,
    pub content: String,
    pub is_error: bool,
}

/// Returns the built-in tool definitions for the engine.
pub fn built_in_tools() -> Vec<Tool> {
    vec![Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "execute_shell".to_string(),
            description: "Execute a shell command in the workspace. Returns stdout on success, error on failure.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        },
    }]
}

/// Executes a tool call and returns the result.
pub fn execute_tool_call(tool_call: &ToolCall, workspace_dir: &Path) -> ToolCallResult {
    match tool_call.name.as_str() {
        "execute_shell" => {
            let command = tool_call
                .arguments
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if command.is_empty() {
                return ToolCallResult {
                    tool_call_id: tool_call.id.clone(),
                    content: "Error: empty command".to_string(),
                    is_error: true,
                };
            }

            info!("Executing tool call: execute_shell({command})");

            let output = if cfg!(target_os = "windows") {
                std::process::Command::new("cmd.exe")
                    .args(["/C", command])
                    .current_dir(workspace_dir)
                    .output()
            } else {
                std::process::Command::new("sh")
                    .args(["-c", command])
                    .current_dir(workspace_dir)
                    .output()
            };

            match output {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

                    if out.status.success() {
                        let mut result = stdout;
                        if result.is_empty() {
                            result = "(no output)".to_string();
                        }
                        ToolCallResult {
                            tool_call_id: tool_call.id.clone(),
                            content: result,
                            is_error: false,
                        }
                    } else {
                        let exit_code = out.status.code().unwrap_or(-1);
                        error!("Tool execute_shell failed (exit code {exit_code}): {stderr}");
                        ToolCallResult {
                            tool_call_id: tool_call.id.clone(),
                            content: format!("Exit code {exit_code}\n{stderr}"),
                            is_error: true,
                        }
                    }
                }
                Err(e) => {
                    error!("Tool execute_shell failed to spawn: {e}");
                    ToolCallResult {
                        tool_call_id: tool_call.id.clone(),
                        content: format!("Failed to execute: {e}"),
                        is_error: true,
                    }
                }
            }
        }
        other => {
            error!("Unknown tool: {other}");
            ToolCallResult {
                tool_call_id: tool_call.id.clone(),
                content: format!("Unknown tool: {other}"),
                is_error: true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_built_in_tools_has_execute_shell() {
        let tools = built_in_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "execute_shell");
        assert_eq!(tools[0].tool_type, "function");
    }

    #[test]
    fn test_execute_tool_call_shell_success() {
        let dir = tempfile::tempdir().unwrap();
        let tc = ToolCall {
            id: "call_1".to_string(),
            name: "execute_shell".to_string(),
            arguments: json!({"command": "echo hello"}),
        };
        let result = execute_tool_call(&tc, dir.path());
        assert!(!result.is_error);
        assert!(result.content.contains("hello"));
        assert_eq!(result.tool_call_id, "call_1");
    }

    #[test]
    fn test_execute_tool_call_shell_failure() {
        let dir = tempfile::tempdir().unwrap();
        let tc = ToolCall {
            id: "call_2".to_string(),
            name: "execute_shell".to_string(),
            arguments: json!({"command": "exit 42"}),
        };
        let result = execute_tool_call(&tc, dir.path());
        assert!(result.is_error);
        assert!(result.content.contains("42"));
    }

    #[test]
    fn test_execute_tool_call_empty_command() {
        let dir = tempfile::tempdir().unwrap();
        let tc = ToolCall {
            id: "call_3".to_string(),
            name: "execute_shell".to_string(),
            arguments: json!({"command": ""}),
        };
        let result = execute_tool_call(&tc, dir.path());
        assert!(result.is_error);
        assert!(result.content.contains("empty command"));
    }

    #[test]
    fn test_execute_tool_call_unknown_tool() {
        let dir = tempfile::tempdir().unwrap();
        let tc = ToolCall {
            id: "call_4".to_string(),
            name: "nonexistent_tool".to_string(),
            arguments: json!({}),
        };
        let result = execute_tool_call(&tc, dir.path());
        assert!(result.is_error);
        assert!(result.content.contains("Unknown tool"));
    }

    #[test]
    fn test_tool_serialization_shape() {
        let tools = built_in_tools();
        let json = serde_json::to_string(&tools).unwrap();
        assert!(json.contains(r#""type":"function""#));
        assert!(json.contains(r#""name":"execute_shell""#));
        assert!(json.contains(r#""required":["command"]"#));
    }
}
