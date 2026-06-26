use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;
use tracing::info;

use crate::tools::{Tool, ToolCall};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ApiProvider {
    Ollama,
    OpenRouter,
    Gemini,
    Mock,
}

impl fmt::Display for ApiProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ollama => write!(f, "ollama"),
            Self::OpenRouter => write!(f, "openrouter"),
            Self::Gemini => write!(f, "gemini"),
            Self::Mock => write!(f, "mock"),
        }
    }
}

impl ApiProvider {
    pub fn required_env_vars(&self) -> &[&str] {
        match self {
            Self::Ollama | Self::Mock => &[],
            Self::OpenRouter => &["OPENROUTER_API_KEY"],
            Self::Gemini => &["GEMINI_API_KEY"],
        }
    }
}

/// Returns the base endpoint URL for the provider, or `None` for
/// providers that build the URL dynamically at request time.
#[allow(dead_code)]
fn provider_endpoint(provider: &ApiProvider) -> Option<&'static str> {
    match provider {
        ApiProvider::Ollama => Some("http://localhost:11434/api/chat"),
        ApiProvider::OpenRouter => Some("https://openrouter.ai/api/v1/chat/completions"),
        ApiProvider::Gemini => None,
        ApiProvider::Mock => None,
    }
}

// ---------------------------------------------------------------------------
// Unified response type
// ---------------------------------------------------------------------------

/// The response from an LLM completion, either text or tool calls.
#[derive(Debug, Clone)]
pub enum CompletionResponse {
    Text(String),
    ToolCalls(Vec<ToolCall>),
}

// ---------------------------------------------------------------------------
// OpenRouter types (OpenAI-compatible)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenRouterToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: OpenRouterFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterChoice {
    pub message: OpenRouterMessage,
    #[serde(rename = "finish_reason")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterRequest {
    pub model: String,
    pub messages: Vec<OpenRouterMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterResponse {
    pub choices: Vec<OpenRouterChoice>,
}

// ---------------------------------------------------------------------------
// Ollama types (chat endpoint, OpenAI-compatible tool format)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OllamaToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaToolCall {
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: OllamaFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaFunctionCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaChatRequest {
    pub model: String,
    pub messages: Vec<OllamaChatMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaChatResponse {
    pub model: String,
    pub message: OllamaChatMessage,
    pub done: bool,
}

// ---------------------------------------------------------------------------
// Gemini types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "functionCall")]
    pub function_call: Option<GeminiFunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "functionResponse")]
    pub function_response: Option<GeminiFunctionResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionCall {
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiContent {
    pub role: String,
    pub parts: Vec<GeminiPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiSystemInstruction {
    pub parts: Vec<GeminiPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiToolDecl {
    #[serde(rename = "functionDeclarations")]
    pub function_declarations: Vec<GeminiFunctionDecl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionDecl {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiToolConfig {
    #[serde(rename = "functionCallingConfig")]
    pub function_calling_config: GeminiFunctionCallingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionCallingConfig {
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiRequest {
    pub contents: Vec<GeminiContent>,
    #[serde(rename = "systemInstruction")]
    pub system_instruction: GeminiSystemInstruction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<GeminiToolDecl>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "toolConfig")]
    pub tool_config: Option<GeminiToolConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiCandidate {
    pub content: GeminiContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiResponse {
    pub candidates: Vec<GeminiCandidate>,
}

// ---------------------------------------------------------------------------
// Unified entry points
// ---------------------------------------------------------------------------

/// Parse provider string into ApiProvider.
fn parse_provider(provider: &str) -> Result<ApiProvider, Box<dyn std::error::Error + Send + Sync>> {
    match provider {
        "ollama" => Ok(ApiProvider::Ollama),
        "openrouter" => Ok(ApiProvider::OpenRouter),
        "gemini" => Ok(ApiProvider::Gemini),
        "mock" => Ok(ApiProvider::Mock),
        other => Err(format!("unsupported API provider: {other}").into()),
    }
}

/// Simple text completion (no tools). Backward-compatible with existing callers.
pub async fn send_completion(
    provider: &str,
    model: &str,
    prompt: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_provider = parse_provider(provider)?;
    info!("Sending request to {api_provider} using {model}");
    completion(
        &api_provider,
        model,
        "You are a helpful coding assistant.",
        prompt,
    )
    .await
}

/// Completion with tool support. Returns `CompletionResponse::ToolCalls`
/// when the model wants to invoke tools, or `CompletionResponse::Text`
/// when it responds with plain text.
pub async fn send_completion_with_tools(
    provider: &str,
    model: &str,
    messages: &[OpenRouterMessage],
    tools: &[Tool],
) -> Result<CompletionResponse, Box<dyn std::error::Error + Send + Sync>> {
    let api_provider = parse_provider(provider)?;
    info!(
        "Sending request to {api_provider} using {model} (with {} tools)",
        tools.len()
    );
    completion_with_tools(&api_provider, model, messages, tools).await
}

// ---------------------------------------------------------------------------
// Internal implementation
// ---------------------------------------------------------------------------

async fn completion(
    provider: &ApiProvider,
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()?;

    match provider {
        ApiProvider::Mock => Ok("add-login-system".to_string()),

        ApiProvider::OpenRouter => {
            let api_key = std::env::var("OPENROUTER_API_KEY")
                .map_err(|_| "OPENROUTER_API_KEY environment variable not set")?;

            let request = OpenRouterRequest {
                model: model.to_string(),
                messages: vec![
                    OpenRouterMessage {
                        role: "system".to_string(),
                        content: Some(system_prompt.to_string()),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                    OpenRouterMessage {
                        role: "user".to_string(),
                        content: Some(user_prompt.to_string()),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                ],
                tools: None,
            };

            let response = client
                .post("https://openrouter.ai/api/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await?;
                return Err(format!("OpenRouter API error ({}): {}", status, body).into());
            }

            let parsed: OpenRouterResponse = response.json().await?;
            parsed
                .choices
                .first()
                .and_then(|c| c.message.content.clone())
                .ok_or_else(|| "OpenRouter response contained no choices".into())
        }

        ApiProvider::Gemini => {
            let api_key = std::env::var("GEMINI_API_KEY")
                .map_err(|_| "GEMINI_API_KEY environment variable not set")?;

            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                model, api_key
            );

            let request = GeminiRequest {
                contents: vec![GeminiContent {
                    role: "user".to_string(),
                    parts: vec![GeminiPart {
                        text: Some(user_prompt.to_string()),
                        function_call: None,
                        function_response: None,
                    }],
                }],
                system_instruction: GeminiSystemInstruction {
                    parts: vec![GeminiPart {
                        text: Some(system_prompt.to_string()),
                        function_call: None,
                        function_response: None,
                    }],
                },
                tools: None,
                tool_config: None,
            };

            let response = client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await?;
                return Err(format!("Gemini API error ({}): {}", status, body).into());
            }

            let parsed: GeminiResponse = response.json().await?;
            parsed
                .candidates
                .first()
                .and_then(|c| c.content.parts.first())
                .and_then(|p| p.text.clone())
                .ok_or_else(|| "Gemini response contained no text".into())
        }

        ApiProvider::Ollama => {
            let request = OllamaChatRequest {
                model: model.to_string(),
                messages: vec![
                    OllamaChatMessage {
                        role: "system".to_string(),
                        content: Some(system_prompt.to_string()),
                        tool_calls: None,
                        tool_name: None,
                    },
                    OllamaChatMessage {
                        role: "user".to_string(),
                        content: Some(user_prompt.to_string()),
                        tool_calls: None,
                        tool_name: None,
                    },
                ],
                stream: false,
                tools: None,
            };

            let response = client
                .post("http://localhost:11434/api/chat")
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await?;
                return Err(format!("Ollama API error ({}): {}", status, body).into());
            }

            let parsed: OllamaChatResponse = response.json().await?;
            Ok(parsed.message.content.unwrap_or_default())
        }
    }
}

async fn completion_with_tools(
    provider: &ApiProvider,
    model: &str,
    messages: &[OpenRouterMessage],
    tools: &[Tool],
) -> Result<CompletionResponse, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()?;

    match provider {
        ApiProvider::Mock => {
            // Return a mock tool call for testing
            Ok(CompletionResponse::ToolCalls(vec![ToolCall {
                id: "mock_call_1".to_string(),
                name: "execute_shell".to_string(),
                arguments: serde_json::json!({"command": "echo mock-executed"}),
            }]))
        }

        ApiProvider::OpenRouter => {
            let api_key = std::env::var("OPENROUTER_API_KEY")
                .map_err(|_| "OPENROUTER_API_KEY environment variable not set")?;

            let request = OpenRouterRequest {
                model: model.to_string(),
                messages: messages.to_vec(),
                tools: Some(tools.to_vec()),
            };

            let response = client
                .post("https://openrouter.ai/api/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await?;
                return Err(format!("OpenRouter API error ({}): {}", status, body).into());
            }

            let parsed: OpenRouterResponse = response.json().await?;
            let choice = parsed
                .choices
                .first()
                .ok_or("OpenRouter response contained no choices")?;

            if choice.finish_reason.as_deref() == Some("tool_calls") {
                let tool_calls = choice
                    .message
                    .tool_calls
                    .as_ref()
                    .map(|calls| {
                        calls
                            .iter()
                            .map(|tc| {
                                let args: serde_json::Value =
                                    serde_json::from_str(&tc.function.arguments)
                                        .unwrap_or(serde_json::Value::Null);
                                ToolCall {
                                    id: tc.id.clone(),
                                    name: tc.function.name.clone(),
                                    arguments: args,
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(CompletionResponse::ToolCalls(tool_calls))
            } else {
                let text = choice.message.content.clone().unwrap_or_default();
                Ok(CompletionResponse::Text(text))
            }
        }

        ApiProvider::Gemini => {
            let api_key = std::env::var("GEMINI_API_KEY")
                .map_err(|_| "GEMINI_API_KEY environment variable not set")?;

            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                model, api_key
            );

            // Convert unified messages to Gemini format
            let contents = messages
                .iter()
                .filter(|m| m.role != "system")
                .map(|m| {
                    let mut parts = Vec::new();
                    if let Some(ref text) = m.content {
                        parts.push(GeminiPart {
                            text: Some(text.clone()),
                            function_call: None,
                            function_response: None,
                        });
                    }
                    if let Some(ref tool_calls) = m.tool_calls {
                        for tc in tool_calls {
                            parts.push(GeminiPart {
                                text: None,
                                function_call: Some(GeminiFunctionCall {
                                    name: tc.function.name.clone(),
                                    args: serde_json::from_str(&tc.function.arguments)
                                        .unwrap_or(serde_json::Value::Null),
                                }),
                                function_response: None,
                            });
                        }
                    }
                    if m.role == "tool" {
                        let name = m.tool_call_id.clone().unwrap_or_default();
                        let content = m.content.clone().unwrap_or_default();
                        parts.push(GeminiPart {
                            text: None,
                            function_call: None,
                            function_response: Some(GeminiFunctionResponse {
                                name,
                                response: serde_json::json!({"result": content}),
                            }),
                        });
                    }
                    GeminiContent {
                        role: if m.role == "tool" {
                            "user".to_string()
                        } else {
                            m.role.clone()
                        },
                        parts,
                    }
                })
                .collect();

            // Convert tools to Gemini format
            let gemini_tools: Vec<GeminiToolDecl> = tools
                .iter()
                .map(|t| GeminiToolDecl {
                    function_declarations: vec![GeminiFunctionDecl {
                        name: t.function.name.clone(),
                        description: t.function.description.clone(),
                        parameters: convert_schema_to_gemini_upper(&t.function.parameters),
                    }],
                })
                .collect();

            let system_parts: Vec<GeminiPart> = messages
                .iter()
                .find(|m| m.role == "system")
                .and_then(|m| m.content.clone())
                .map(|text| {
                    vec![GeminiPart {
                        text: Some(text),
                        function_call: None,
                        function_response: None,
                    }]
                })
                .unwrap_or_default();

            let request = GeminiRequest {
                contents,
                system_instruction: GeminiSystemInstruction {
                    parts: system_parts,
                },
                tools: Some(gemini_tools),
                tool_config: Some(GeminiToolConfig {
                    function_calling_config: GeminiFunctionCallingConfig {
                        mode: "AUTO".to_string(),
                    },
                }),
            };

            let response = client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await?;
                return Err(format!("Gemini API error ({}): {}", status, body).into());
            }

            let parsed: GeminiResponse = response.json().await?;
            let candidate = parsed
                .candidates
                .first()
                .ok_or("Gemini response contained no candidates")?;

            // Check for function calls in parts
            let mut tool_calls = Vec::new();
            let mut text_parts = Vec::new();
            for part in &candidate.content.parts {
                if let Some(ref fc) = part.function_call {
                    tool_calls.push(ToolCall {
                        id: fc.name.clone(),
                        name: fc.name.clone(),
                        arguments: fc.args.clone(),
                    });
                }
                if let Some(ref text) = part.text {
                    text_parts.push(text.clone());
                }
            }

            if !tool_calls.is_empty() {
                Ok(CompletionResponse::ToolCalls(tool_calls))
            } else {
                Ok(CompletionResponse::Text(text_parts.join("")))
            }
        }

        ApiProvider::Ollama => {
            // Convert unified messages to Ollama format
            let ollama_messages: Vec<OllamaChatMessage> = messages
                .iter()
                .map(|m| OllamaChatMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                    tool_calls: m.tool_calls.as_ref().map(|calls| {
                        calls
                            .iter()
                            .map(|tc| OllamaToolCall {
                                call_type: Some(tc.call_type.clone()),
                                function: OllamaFunctionCall {
                                    name: tc.function.name.clone(),
                                    arguments: serde_json::from_str(&tc.function.arguments)
                                        .unwrap_or(serde_json::Value::Null),
                                },
                            })
                            .collect()
                    }),
                    tool_name: m.tool_call_id.clone(),
                })
                .collect();

            let request = OllamaChatRequest {
                model: model.to_string(),
                messages: ollama_messages,
                stream: false,
                tools: Some(tools.to_vec()),
            };

            let response = client
                .post("http://localhost:11434/api/chat")
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await?;
                return Err(format!("Ollama API error ({}): {}", status, body).into());
            }

            let parsed: OllamaChatResponse = response.json().await?;

            if let Some(ref tool_calls) = parsed.message.tool_calls {
                let calls: Vec<ToolCall> = tool_calls
                    .iter()
                    .enumerate()
                    .map(|(i, tc)| ToolCall {
                        id: format!("ollama_call_{i}"),
                        name: tc.function.name.clone(),
                        arguments: tc.function.arguments.clone(),
                    })
                    .collect();
                Ok(CompletionResponse::ToolCalls(calls))
            } else {
                let text = parsed.message.content.clone().unwrap_or_default();
                Ok(CompletionResponse::Text(text))
            }
        }
    }
}

/// Convert JSON Schema type names from lowercase to Gemini's uppercase format.
fn convert_schema_to_gemini_upper(schema: &serde_json::Value) -> serde_json::Value {
    match schema {
        serde_json::Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (key, val) in map {
                if key == "type" {
                    if let Some(s) = val.as_str() {
                        let upper = match s {
                            "object" => "OBJECT",
                            "string" => "STRING",
                            "number" => "NUMBER",
                            "integer" => "INTEGER",
                            "array" => "ARRAY",
                            "boolean" => "BOOLEAN",
                            other => other,
                        };
                        new_map.insert(key.clone(), serde_json::Value::String(upper.to_string()));
                    } else {
                        new_map.insert(key.clone(), val.clone());
                    }
                } else {
                    new_map.insert(key.clone(), convert_schema_to_gemini_upper(val));
                }
            }
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(convert_schema_to_gemini_upper).collect())
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::built_in_tools;

    #[test]
    fn test_mock_provider_fallback_behavior() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let result = rt.block_on(completion(
            &ApiProvider::Mock,
            "not-used",
            "not-used",
            "not-used",
        ));
        assert_eq!(result.unwrap(), "add-login-system");
    }

    #[test]
    fn test_provider_endpoint_and_routing_logic() {
        assert_eq!(
            provider_endpoint(&ApiProvider::Ollama),
            Some("http://localhost:11434/api/chat")
        );
        assert_eq!(
            provider_endpoint(&ApiProvider::OpenRouter),
            Some("https://openrouter.ai/api/v1/chat/completions")
        );
        assert_eq!(provider_endpoint(&ApiProvider::Gemini), None);
        assert_eq!(provider_endpoint(&ApiProvider::Mock), None);
    }

    #[test]
    fn test_payload_serialization_shapes_ollama() {
        let req = OllamaChatRequest {
            model: "test-model".into(),
            messages: vec![OllamaChatMessage {
                role: "user".into(),
                content: Some("what is rust?".into()),
                tool_calls: None,
                tool_name: None,
            }],
            stream: false,
            tools: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""stream":false"#), "got: {}", json);
        assert!(json.contains(r#""model":"test-model""#), "got: {}", json);
        assert!(json.contains(r#""messages""#), "got: {}", json);
    }

    #[test]
    fn test_payload_serialization_shapes_openrouter() {
        let msg = OpenRouterMessage {
            role: "user".into(),
            content: Some("hello".into()),
            tool_calls: None,
            tool_call_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"role":"user","content":"hello"}"#);
    }

    #[test]
    fn test_payload_serialization_shapes_gemini() {
        let req = GeminiRequest {
            contents: vec![GeminiContent {
                role: "user".to_string(),
                parts: vec![GeminiPart {
                    text: Some("user prompt".into()),
                    function_call: None,
                    function_response: None,
                }],
            }],
            system_instruction: GeminiSystemInstruction {
                parts: vec![GeminiPart {
                    text: Some("system prompt".into()),
                    function_call: None,
                    function_response: None,
                }],
            },
            tools: None,
            tool_config: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""systemInstruction""#), "got: {}", json);
        assert!(json.contains(r#""text":"user prompt""#), "got: {}", json);
    }

    #[test]
    fn test_convert_schema_to_gemini_upper() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "A shell command"
                }
            },
            "required": ["command"]
        });
        let converted = convert_schema_to_gemini_upper(&schema);
        assert_eq!(converted["type"], "OBJECT");
        assert_eq!(converted["properties"]["command"]["type"], "STRING");
        // Non-type fields should be preserved
        assert_eq!(converted["required"], serde_json::json!(["command"]));
    }

    #[test]
    fn test_mock_completion_with_tools() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let msgs = vec![OpenRouterMessage {
            role: "user".to_string(),
            content: Some("test".to_string()),
            tool_calls: None,
            tool_call_id: None,
        }];
        let tools = built_in_tools();
        let result = rt.block_on(completion_with_tools(
            &ApiProvider::Mock,
            "not-used",
            &msgs,
            &tools,
        ));
        match result.unwrap() {
            CompletionResponse::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "execute_shell");
            }
            _ => panic!("Expected tool calls from mock"),
        }
    }

    /// Helper: remove an env var for the duration of a single test.
    struct EnvGuard(String, Option<String>);

    impl EnvGuard {
        fn remove(key: &str) -> Self {
            let prev = std::env::var(key).ok();
            unsafe { std::env::remove_var(key) };
            Self(key.to_string(), prev)
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.1 {
                Some(val) => unsafe { std::env::set_var(&self.0, val) },
                None => unsafe { std::env::remove_var(&self.0) },
            }
        }
    }

    #[test]
    fn test_missing_credential_openrouter() {
        let _guard = EnvGuard::remove("OPENROUTER_API_KEY");
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let result = rt.block_on(completion(&ApiProvider::OpenRouter, "model", "sys", "user"));
        let err = result.unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("OPENROUTER_API_KEY"),
            "expected API key error, got: {}",
            msg
        );
    }

    #[test]
    fn test_missing_credential_gemini() {
        let _guard = EnvGuard::remove("GEMINI_API_KEY");
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let result = rt.block_on(completion(&ApiProvider::Gemini, "model", "sys", "user"));
        let err = result.unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("GEMINI_API_KEY"),
            "expected API key error, got: {}",
            msg
        );
    }

    #[test]
    fn test_required_env_vars_mapping() {
        assert_eq!(ApiProvider::Ollama.required_env_vars(), &[] as &[&str]);
        assert_eq!(ApiProvider::Mock.required_env_vars(), &[] as &[&str]);
        assert_eq!(
            ApiProvider::OpenRouter.required_env_vars(),
            &["OPENROUTER_API_KEY"]
        );
        assert_eq!(ApiProvider::Gemini.required_env_vars(), &["GEMINI_API_KEY"]);
    }

    #[test]
    fn test_openrouter_tool_call_response_parsing() {
        let json = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "execute_shell",
                            "arguments": "{\"command\": \"ls -la\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }"#;
        let parsed: OpenRouterResponse = serde_json::from_str(json).unwrap();
        let choice = &parsed.choices[0];
        assert_eq!(choice.finish_reason.as_deref(), Some("tool_calls"));
        let tool_calls = choice.message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "execute_shell");
        let args: serde_json::Value =
            serde_json::from_str(&tool_calls[0].function.arguments).unwrap();
        assert_eq!(args["command"], "ls -la");
    }

    #[test]
    fn test_ollama_tool_call_response_parsing() {
        let json = r#"{
            "model": "qwen3",
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "type": "function",
                    "function": {
                        "name": "execute_shell",
                        "arguments": {"command": "echo hi"}
                    }
                }]
            },
            "done": false
        }"#;
        let parsed: OllamaChatResponse = serde_json::from_str(json).unwrap();
        let tool_calls = parsed.message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "execute_shell");
        assert_eq!(tool_calls[0].function.arguments["command"], "echo hi");
    }
}
