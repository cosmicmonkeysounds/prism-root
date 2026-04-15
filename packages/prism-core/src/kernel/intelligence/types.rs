//! `intelligence::types` — AI provider trait, context-builder shapes,
//! minimal HTTP-client interface.
//!
//! Port of `kernel/actor/ai-types.ts` at 8426588, split out of
//! `kernel::actor` per ADR-002 §Part C so the executor and the AI layer
//! are no longer tangled in one folder.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::super::actor::ExecutionTarget;

/// AI conversation role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiRole {
    System,
    User,
    Assistant,
}

/// A single message in a chat-style completion request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: AiRole,
    pub content: String,
}

impl AiMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: AiRole::System,
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: AiRole::User,
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: AiRole::Assistant,
            content: content.into(),
        }
    }
}

/// Chat-style completion request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCompletionRequest {
    pub messages: Vec<AiMessage>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub stop: Option<Vec<String>>,
}

/// Token usage returned by providers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Chat-style completion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCompletion {
    pub content: String,
    pub model: String,
    pub usage: AiUsage,
    pub duration_ms: u64,
}

/// Inline / ghost-text completion request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineCompletionRequest {
    pub prefix: String,
    pub suffix: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

/// Inline completion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineCompletion {
    pub text: String,
    pub model: String,
    pub duration_ms: u64,
}

/// Object-aware context built from graph neighbours and collection
/// state. Fed to providers for object-aware reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectContext {
    pub object: JsonValue,
    pub object_type: String,
    pub ancestors: Vec<ContextEntry>,
    pub children: Vec<ContextEntry>,
    pub edges: Vec<ContextEdge>,
    pub collection: Option<ContextCollection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEdge {
    pub id: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub target_id: String,
    pub target_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextCollection {
    pub id: String,
    pub name: String,
}

/// Errors returned by AI provider calls.
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("no active AI provider")]
    NoActiveProvider,
    #[error("AI provider \"{0}\" not registered")]
    NotRegistered(String),
    #[error("provider \"{name}\" request failed: {status} {body}")]
    RequestFailed {
        name: String,
        status: u16,
        body: String,
    },
    #[error("provider \"{0}\" transport error: {1}")]
    Transport(String, String),
    #[error("provider \"{0}\" returned invalid JSON: {1}")]
    InvalidResponse(String, String),
    #[error("provider misconfigured: {0}")]
    Misconfigured(String),
}

/// HTTP response as seen by an [`AiHttpClient`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// Minimal HTTP-client interface. Real hosts wire in `reqwest` or their
/// own transport; tests inject a scripted client.
pub trait AiHttpClient: Send + Sync {
    fn post(
        &self,
        url: &str,
        body: &str,
        headers: &[(String, String)],
    ) -> Result<HttpResponse, String>;
    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<HttpResponse, String>;
}

/// Pluggable AI provider. Implementations: `ollama` (local HTTP),
/// `external` (Claude / OpenAI-compatible), plus `TestAiProvider` for
/// tests.
pub trait AiProvider: Send + Sync {
    fn name(&self) -> &str;
    fn target(&self) -> ExecutionTarget;
    fn default_model(&self) -> &str;
    fn list_models(&self) -> Result<Vec<String>, AiError>;
    fn complete(&self, request: &AiCompletionRequest) -> Result<AiCompletion, AiError>;
    fn complete_inline(
        &self,
        request: &InlineCompletionRequest,
    ) -> Result<InlineCompletion, AiError>;
    fn is_available(&self) -> bool;
}

/// Options for the context builder.
#[derive(Debug, Clone, Copy)]
pub struct ContextBuilderOptions {
    /// Maximum ancestor depth. Default 5.
    pub max_ancestor_depth: usize,
    /// Maximum children to include. Default 20.
    pub max_children: usize,
    /// Maximum edges to include. Default 20.
    pub max_edges: usize,
}

impl Default for ContextBuilderOptions {
    fn default() -> Self {
        Self {
            max_ancestor_depth: 5,
            max_children: 20,
            max_edges: 20,
        }
    }
}
