use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;
use tracing::info;

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
        ApiProvider::Ollama => Some("http://localhost:11434/api/generate"),
        ApiProvider::OpenRouter => Some("https://openrouter.ai/api/v1/chat/completions"),
        ApiProvider::Gemini => None,
        ApiProvider::Mock => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaRequest {
    pub model: String,
    pub prompt: String,
    pub system: String,
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaResponse {
    pub model: String,
    pub response: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterChoice {
    pub message: OpenRouterMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterRequest {
    pub model: String,
    pub messages: Vec<OpenRouterMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterResponse {
    pub choices: Vec<OpenRouterChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiPart {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiContent {
    pub parts: Vec<GeminiPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiSystemInstruction {
    pub parts: Vec<GeminiPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiRequest {
    pub contents: Vec<GeminiContent>,
    #[serde(rename = "systemInstruction")]
    pub system_instruction: GeminiSystemInstruction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiCandidate {
    pub content: GeminiContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiResponse {
    pub candidates: Vec<GeminiCandidate>,
}

/// Unified entry point: resolves provider from string, builds the
/// request, logs the call, and returns the text response.
pub async fn send_completion(
    provider: &str,
    model: &str,
    prompt: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_provider = match provider {
        "ollama" => ApiProvider::Ollama,
        "openrouter" => ApiProvider::OpenRouter,
        "gemini" => ApiProvider::Gemini,
        "mock" => ApiProvider::Mock,
        other => return Err(format!("unsupported API provider: {other}").into()),
    };

    info!("Sending request to {api_provider} using {model}");

    completion(
        &api_provider,
        model,
        "You are a helpful coding assistant.",
        prompt,
    )
    .await
}

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
                        content: system_prompt.to_string(),
                    },
                    OpenRouterMessage {
                        role: "user".to_string(),
                        content: user_prompt.to_string(),
                    },
                ],
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
                .map(|c| c.message.content.clone())
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
                    parts: vec![GeminiPart {
                        text: user_prompt.to_string(),
                    }],
                }],
                system_instruction: GeminiSystemInstruction {
                    parts: vec![GeminiPart {
                        text: system_prompt.to_string(),
                    }],
                },
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
                .map(|p| p.text.clone())
                .ok_or_else(|| "Gemini response contained no text".into())
        }

        ApiProvider::Ollama => {
            let request = OllamaRequest {
                model: model.to_string(),
                prompt: user_prompt.to_string(),
                system: system_prompt.to_string(),
                stream: false,
            };

            let response = client
                .post("http://localhost:11434/api/generate")
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await?;
                return Err(format!("Ollama API error ({}): {}", status, body).into());
            }

            let parsed: OllamaResponse = response.json().await?;
            Ok(parsed.response)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            Some("http://localhost:11434/api/generate")
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
        let req = OllamaRequest {
            model: "test-model".into(),
            prompt: "what is rust?".into(),
            system: "you are helpful".into(),
            stream: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""stream":false"#), "got: {}", json);
        assert!(json.contains(r#""model":"test-model""#), "got: {}", json);
    }

    #[test]
    fn test_payload_serialization_shapes_openrouter() {
        let msg = OpenRouterMessage {
            role: "user".into(),
            content: "hello".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"role":"user","content":"hello"}"#);
    }

    #[test]
    fn test_payload_serialization_shapes_gemini() {
        let req = GeminiRequest {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart {
                    text: "user prompt".into(),
                }],
            }],
            system_instruction: GeminiSystemInstruction {
                parts: vec![GeminiPart {
                    text: "system prompt".into(),
                }],
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""systemInstruction""#), "got: {}", json);
        assert!(json.contains(r#""text":"user prompt""#), "got: {}", json);
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
}
