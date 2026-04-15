//! `intelligence::providers` — Ollama, OpenAI-compatible external, and
//! an in-memory test provider.
//!
//! These wrap the pluggable [`AiHttpClient`] trait; hosts drive real
//! HTTP themselves. The `TestAiProvider` is deliberate zero-dependency
//! and ships here so the rest of the core can use it in doctests and
//! unit tests without pulling in a mock framework.

use std::sync::Arc;

use super::super::actor::ExecutionTarget;
use super::types::{
    AiCompletion, AiCompletionRequest, AiError, AiHttpClient, AiMessage, AiProvider, AiRole,
    AiUsage, InlineCompletion, InlineCompletionRequest,
};

fn role_str(role: AiRole) -> &'static str {
    match role {
        AiRole::System => "system",
        AiRole::User => "user",
        AiRole::Assistant => "assistant",
    }
}

fn messages_to_json(messages: &[AiMessage]) -> serde_json::Value {
    serde_json::Value::Array(
        messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": role_str(m.role),
                    "content": m.content,
                })
            })
            .collect(),
    )
}

// ── Ollama ──────────────────────────────────────────────────────────────────

pub struct OllamaProvider {
    base_url: String,
    default_model: String,
    client: Arc<dyn AiHttpClient>,
}

impl OllamaProvider {
    pub fn new(
        base_url: impl Into<String>,
        default_model: impl Into<String>,
        client: Arc<dyn AiHttpClient>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            default_model: default_model.into(),
            client,
        }
    }
}

impl AiProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }
    fn target(&self) -> ExecutionTarget {
        ExecutionTarget::Local
    }
    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn list_models(&self) -> Result<Vec<String>, AiError> {
        let resp = self
            .client
            .get(&format!("{}/api/tags", self.base_url), &[])
            .map_err(|e| AiError::Transport("ollama".into(), e))?;
        if resp.status != 200 {
            return Ok(Vec::new());
        }
        let parsed: serde_json::Value = serde_json::from_str(&resp.body)
            .map_err(|e| AiError::InvalidResponse("ollama".into(), e.to_string()))?;
        let models = parsed
            .get("models")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        m.get("name")
                            .and_then(|n| n.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(models)
    }

    fn complete(&self, request: &AiCompletionRequest) -> Result<AiCompletion, AiError> {
        let model = request
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());
        let start = std::time::Instant::now();

        let mut options = serde_json::Map::new();
        if let Some(t) = request.temperature {
            options.insert("temperature".into(), serde_json::json!(t));
        }
        if let Some(n) = request.max_tokens {
            options.insert("num_predict".into(), serde_json::json!(n));
        }

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages_to_json(&request.messages),
            "stream": false,
            "options": options,
        });
        if let Some(stop) = &request.stop {
            body["stop"] = serde_json::json!(stop);
        }

        let resp = self
            .client
            .post(
                &format!("{}/api/chat", self.base_url),
                &body.to_string(),
                &[("Content-Type".into(), "application/json".into())],
            )
            .map_err(|e| AiError::Transport("ollama".into(), e))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        if resp.status != 200 {
            return Err(AiError::RequestFailed {
                name: "ollama".into(),
                status: resp.status,
                body: resp.body,
            });
        }

        let parsed: serde_json::Value = serde_json::from_str(&resp.body)
            .map_err(|e| AiError::InvalidResponse("ollama".into(), e.to_string()))?;
        let content = parsed
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        let prompt = parsed
            .get("prompt_eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let eval = parsed
            .get("eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        Ok(AiCompletion {
            content,
            model,
            usage: AiUsage {
                prompt_tokens: prompt,
                completion_tokens: eval,
                total_tokens: prompt + eval,
            },
            duration_ms,
        })
    }

    fn complete_inline(
        &self,
        request: &InlineCompletionRequest,
    ) -> Result<InlineCompletion, AiError> {
        let model = self.default_model.clone();
        let start = std::time::Instant::now();

        let prompt = if request.suffix.is_empty() {
            request.prefix.clone()
        } else {
            format!("{}<FILL>{}", request.prefix, request.suffix)
        };

        let mut options = serde_json::Map::new();
        if let Some(n) = request.max_tokens {
            options.insert("num_predict".into(), serde_json::json!(n));
        }
        let body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
            "options": options,
        });

        let resp = self
            .client
            .post(
                &format!("{}/api/generate", self.base_url),
                &body.to_string(),
                &[("Content-Type".into(), "application/json".into())],
            )
            .map_err(|e| AiError::Transport("ollama".into(), e))?;

        let duration_ms = start.elapsed().as_millis() as u64;
        if resp.status != 200 {
            return Err(AiError::RequestFailed {
                name: "ollama".into(),
                status: resp.status,
                body: resp.body,
            });
        }
        let parsed: serde_json::Value = serde_json::from_str(&resp.body)
            .map_err(|e| AiError::InvalidResponse("ollama".into(), e.to_string()))?;
        let text = parsed
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Ok(InlineCompletion {
            text,
            model,
            duration_ms,
        })
    }

    fn is_available(&self) -> bool {
        match self.client.get(&format!("{}/api/tags", self.base_url), &[]) {
            Ok(resp) => resp.status == 200,
            Err(_) => false,
        }
    }
}

// ── External (OpenAI-compatible) ────────────────────────────────────────────

pub struct ExternalProvider {
    name: String,
    base_url: String,
    default_model: String,
    api_key: String,
    client: Arc<dyn AiHttpClient>,
}

impl ExternalProvider {
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        default_model: impl Into<String>,
        api_key: impl Into<String>,
        client: Arc<dyn AiHttpClient>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            default_model: default_model.into(),
            api_key: api_key.into(),
            client,
        }
    }

    fn auth_headers(&self) -> Vec<(String, String)> {
        vec![
            ("Content-Type".into(), "application/json".into()),
            ("Authorization".into(), format!("Bearer {}", self.api_key)),
        ]
    }
}

impl AiProvider for ExternalProvider {
    fn name(&self) -> &str {
        &self.name
    }
    fn target(&self) -> ExecutionTarget {
        ExecutionTarget::External
    }
    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn list_models(&self) -> Result<Vec<String>, AiError> {
        let resp = self
            .client
            .get(&format!("{}/models", self.base_url), &self.auth_headers())
            .map_err(|e| AiError::Transport(self.name.clone(), e))?;
        if resp.status != 200 {
            return Ok(vec![self.default_model.clone()]);
        }
        let parsed: serde_json::Value = serde_json::from_str(&resp.body)
            .map_err(|e| AiError::InvalidResponse(self.name.clone(), e.to_string()))?;
        let models = parsed
            .get("data")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|n| n.as_str()).map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec![self.default_model.clone()]);
        Ok(models)
    }

    fn complete(&self, request: &AiCompletionRequest) -> Result<AiCompletion, AiError> {
        let model = request
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());
        let start = std::time::Instant::now();

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages_to_json(&request.messages),
        });
        if let Some(n) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(n);
        }
        if let Some(t) = request.temperature {
            body["temperature"] = serde_json::json!(t);
        }
        if let Some(stop) = &request.stop {
            body["stop"] = serde_json::json!(stop);
        }

        let resp = self
            .client
            .post(
                &format!("{}/chat/completions", self.base_url),
                &body.to_string(),
                &self.auth_headers(),
            )
            .map_err(|e| AiError::Transport(self.name.clone(), e))?;

        let duration_ms = start.elapsed().as_millis() as u64;
        if resp.status != 200 {
            return Err(AiError::RequestFailed {
                name: self.name.clone(),
                status: resp.status,
                body: resp.body,
            });
        }
        let parsed: serde_json::Value = serde_json::from_str(&resp.body)
            .map_err(|e| AiError::InvalidResponse(self.name.clone(), e.to_string()))?;
        let content = parsed
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let usage = parsed.get("usage");
        let prompt = usage
            .and_then(|u| u.get("prompt_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let completion = usage
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let total = usage
            .and_then(|u| u.get("total_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or((prompt + completion) as u64) as u32;
        Ok(AiCompletion {
            content,
            model,
            usage: AiUsage {
                prompt_tokens: prompt,
                completion_tokens: completion,
                total_tokens: total,
            },
            duration_ms,
        })
    }

    fn complete_inline(
        &self,
        request: &InlineCompletionRequest,
    ) -> Result<InlineCompletion, AiError> {
        let completion = self.complete(&AiCompletionRequest {
            messages: vec![
                AiMessage::system(
                    "Complete the code. Return ONLY the completion text, no explanation.",
                ),
                AiMessage::user(format!("{}[CURSOR]{}", request.prefix, request.suffix)),
            ],
            model: Some(self.default_model.clone()),
            max_tokens: Some(request.max_tokens.unwrap_or(100)),
            temperature: Some(0.0),
            stop: None,
        })?;
        Ok(InlineCompletion {
            text: completion.content,
            model: completion.model,
            duration_ms: completion.duration_ms,
        })
    }

    fn is_available(&self) -> bool {
        match self
            .client
            .get(&format!("{}/models", self.base_url), &self.auth_headers())
        {
            Ok(resp) => resp.status == 200,
            Err(_) => false,
        }
    }
}

// ── Test provider ───────────────────────────────────────────────────────────

/// In-memory provider with canned responses. Used by the registry tests
/// and by hosts bringing up the kernel without any real provider wired.
pub struct TestAiProvider {
    name: String,
    complete_text: String,
    inline_text: String,
}

impl TestAiProvider {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            complete_text: "test response".into(),
            inline_text: "test completion".into(),
        }
    }

    pub fn with_complete(mut self, text: impl Into<String>) -> Self {
        self.complete_text = text.into();
        self
    }

    pub fn with_inline(mut self, text: impl Into<String>) -> Self {
        self.inline_text = text.into();
        self
    }
}

impl AiProvider for TestAiProvider {
    fn name(&self) -> &str {
        &self.name
    }
    fn target(&self) -> ExecutionTarget {
        ExecutionTarget::Local
    }
    fn default_model(&self) -> &str {
        "test-model"
    }
    fn list_models(&self) -> Result<Vec<String>, AiError> {
        Ok(vec!["test-model".into()])
    }
    fn complete(&self, request: &AiCompletionRequest) -> Result<AiCompletion, AiError> {
        Ok(AiCompletion {
            content: self.complete_text.clone(),
            model: request.model.clone().unwrap_or_else(|| "test-model".into()),
            usage: AiUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
            duration_ms: 1,
        })
    }
    fn complete_inline(
        &self,
        _request: &InlineCompletionRequest,
    ) -> Result<InlineCompletion, AiError> {
        Ok(InlineCompletion {
            text: self.inline_text.clone(),
            model: "test-model".into(),
            duration_ms: 1,
        })
    }
    fn is_available(&self) -> bool {
        true
    }
}
