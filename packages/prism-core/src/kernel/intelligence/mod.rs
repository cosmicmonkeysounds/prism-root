//! `kernel::intelligence` — AI provider registry, context builder, and
//! the Ollama / external / test provider implementations.
//!
//! Port of `kernel/actor/{ai,ai-types}.ts` at 8426588, split out per
//! ADR-002 §Part C so the executor (`kernel::actor`) and the AI layer
//! live in separate folders. Nothing here touches HTTP directly — hosts
//! inject an [`AiHttpClient`]; the real transport lives downstream.

pub mod context_builder;
pub mod providers;
pub mod registry;
pub mod types;

pub use context_builder::{ContextBuildParams, ContextBuilder};
pub use providers::{ExternalProvider, OllamaProvider, TestAiProvider};
pub use registry::AiProviderRegistry;
pub use types::{
    AiCompletion, AiCompletionRequest, AiError, AiHttpClient, AiMessage, AiProvider, AiRole,
    AiUsage, ContextBuilderOptions, ContextCollection, ContextEdge, ContextEntry, HttpResponse,
    InlineCompletion, InlineCompletionRequest, ObjectContext,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // ── Test HTTP client ────────────────────────────────────────────

    struct ScriptedClient {
        responses: Mutex<Vec<(String, HttpResponse)>>,
    }

    impl ScriptedClient {
        fn new(responses: Vec<(String, HttpResponse)>) -> Self {
            Self {
                responses: Mutex::new(responses),
            }
        }
    }

    impl AiHttpClient for ScriptedClient {
        fn post(
            &self,
            url: &str,
            _body: &str,
            _headers: &[(String, String)],
        ) -> Result<HttpResponse, String> {
            let mut responses = self.responses.lock().unwrap();
            let idx = responses
                .iter()
                .position(|(u, _)| u == url)
                .ok_or_else(|| format!("no scripted response for POST {url}"))?;
            Ok(responses.remove(idx).1)
        }
        fn get(&self, url: &str, _headers: &[(String, String)]) -> Result<HttpResponse, String> {
            let mut responses = self.responses.lock().unwrap();
            let idx = responses
                .iter()
                .position(|(u, _)| u == url)
                .ok_or_else(|| format!("no scripted response for GET {url}"))?;
            Ok(responses.remove(idx).1)
        }
    }

    // ── Registry ────────────────────────────────────────────────────

    #[test]
    fn registry_starts_with_no_active_provider() {
        let r = AiProviderRegistry::new();
        assert!(r.active().is_none());
        assert!(matches!(
            r.complete(&AiCompletionRequest::default()),
            Err(AiError::NoActiveProvider)
        ));
    }

    #[test]
    fn first_registered_becomes_active() {
        let mut r = AiProviderRegistry::new();
        r.register(Arc::new(TestAiProvider::new("a")));
        r.register(Arc::new(TestAiProvider::new("b")));
        assert_eq!(r.active().unwrap().name(), "a");
        assert_eq!(r.list(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn set_active_errors_on_missing() {
        let mut r = AiProviderRegistry::new();
        r.register(Arc::new(TestAiProvider::new("a")));
        assert!(r.set_active("ghost").is_err());
        assert!(r.set_active("a").is_ok());
    }

    #[test]
    fn registry_complete_routes_to_active() {
        let mut r = AiProviderRegistry::new();
        r.register(Arc::new(
            TestAiProvider::new("cool").with_complete("hello from test"),
        ));
        let resp = r.complete(&AiCompletionRequest::default()).unwrap();
        assert_eq!(resp.content, "hello from test");
    }

    // ── Context builder ─────────────────────────────────────────────

    #[test]
    fn context_builder_truncates_to_caps() {
        let cb = ContextBuilder::new(ContextBuilderOptions {
            max_ancestor_depth: 2,
            max_children: 1,
            max_edges: 1,
        });
        let ctx = cb.build(ContextBuildParams {
            object: serde_json::json!({"k": "v"}),
            object_type: "task".into(),
            ancestors: (0..5)
                .map(|i| ContextEntry {
                    id: format!("a{i}"),
                    type_name: "t".into(),
                    name: format!("ancestor {i}"),
                })
                .collect(),
            children: (0..5)
                .map(|i| ContextEntry {
                    id: format!("c{i}"),
                    type_name: "t".into(),
                    name: format!("child {i}"),
                })
                .collect(),
            edges: (0..5)
                .map(|i| ContextEdge {
                    id: format!("e{i}"),
                    type_name: "rel".into(),
                    target_id: format!("t{i}"),
                    target_type: "node".into(),
                })
                .collect(),
            collection: None,
        });
        assert_eq!(ctx.ancestors.len(), 2);
        assert_eq!(ctx.children.len(), 1);
        assert_eq!(ctx.edges.len(), 1);
    }

    #[test]
    fn context_builder_system_message_includes_all_sections() {
        let ctx = ContextBuilder::default().build(ContextBuildParams {
            object: serde_json::json!({"status": "active"}),
            object_type: "task".into(),
            ancestors: vec![
                ContextEntry {
                    id: "p1".into(),
                    type_name: "project".into(),
                    name: "Apollo".into(),
                },
                ContextEntry {
                    id: "p2".into(),
                    type_name: "section".into(),
                    name: "Phase 2b".into(),
                },
            ],
            children: vec![ContextEntry {
                id: "c1".into(),
                type_name: "subtask".into(),
                name: "Actor port".into(),
            }],
            edges: vec![ContextEdge {
                id: "e1".into(),
                type_name: "depends_on".into(),
                target_id: "t9".into(),
                target_type: "task".into(),
            }],
            collection: Some(ContextCollection {
                id: "col1".into(),
                name: "Engineering".into(),
            }),
        });
        let msg = ContextBuilder::to_system_message(&ctx);
        assert!(matches!(msg.role, AiRole::System));
        assert!(msg.content.contains("\"task\""));
        assert!(msg.content.contains("Engineering"));
        assert!(msg.content.contains("Apollo → Phase 2b"));
        assert!(msg.content.contains("Actor port [subtask]"));
        assert!(msg.content.contains("→ task:t9"));
        assert!(msg.content.contains("\"status\": \"active\""));
    }

    // ── Ollama ──────────────────────────────────────────────────────

    #[test]
    fn ollama_complete_happy_path() {
        let client = Arc::new(ScriptedClient::new(vec![(
            "http://localhost:11434/api/chat".into(),
            HttpResponse {
                status: 200,
                body: serde_json::json!({
                    "message": { "content": "hello" },
                    "prompt_eval_count": 3,
                    "eval_count": 2,
                })
                .to_string(),
            },
        )]));
        let p = OllamaProvider::new("http://localhost:11434", "qwen3.5", client);
        let out = p
            .complete(&AiCompletionRequest {
                messages: vec![AiMessage::user("hi")],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(out.content, "hello");
        assert_eq!(out.usage.total_tokens, 5);
    }

    #[test]
    fn ollama_complete_error_status() {
        let client = Arc::new(ScriptedClient::new(vec![(
            "http://localhost:11434/api/chat".into(),
            HttpResponse {
                status: 500,
                body: "boom".into(),
            },
        )]));
        let p = OllamaProvider::new("http://localhost:11434", "qwen3.5", client);
        let err = p
            .complete(&AiCompletionRequest {
                messages: vec![AiMessage::user("hi")],
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(err, AiError::RequestFailed { .. }));
    }

    #[test]
    fn ollama_list_models_parses_tags() {
        let client = Arc::new(ScriptedClient::new(vec![(
            "http://localhost:11434/api/tags".into(),
            HttpResponse {
                status: 200,
                body: serde_json::json!({
                    "models": [ { "name": "qwen3.5" }, { "name": "llama3" } ]
                })
                .to_string(),
            },
        )]));
        let p = OllamaProvider::new("http://localhost:11434", "qwen3.5", client);
        let models = p.list_models().unwrap();
        assert_eq!(models, vec!["qwen3.5".to_string(), "llama3".to_string()]);
    }

    // ── External provider ──────────────────────────────────────────

    #[test]
    fn external_complete_openai_shape() {
        let client = Arc::new(ScriptedClient::new(vec![(
            "https://api.example.com/chat/completions".into(),
            HttpResponse {
                status: 200,
                body: serde_json::json!({
                    "choices": [ { "message": { "content": "ok" } } ],
                    "usage": { "prompt_tokens": 4, "completion_tokens": 6, "total_tokens": 10 },
                })
                .to_string(),
            },
        )]));
        let p = ExternalProvider::new(
            "example",
            "https://api.example.com",
            "gpt-4",
            "sk-test",
            client,
        );
        let out = p
            .complete(&AiCompletionRequest {
                messages: vec![AiMessage::user("hi")],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(out.content, "ok");
        assert_eq!(out.model, "gpt-4");
        assert_eq!(out.usage.total_tokens, 10);
    }

    #[test]
    fn external_inline_complete_wraps_prompt() {
        let client = Arc::new(ScriptedClient::new(vec![(
            "https://api.example.com/chat/completions".into(),
            HttpResponse {
                status: 200,
                body: serde_json::json!({
                    "choices": [ { "message": { "content": "INLINE-FILL" } } ],
                    "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 },
                })
                .to_string(),
            },
        )]));
        let p = ExternalProvider::new(
            "example",
            "https://api.example.com",
            "gpt-4",
            "sk-test",
            client,
        );
        let out = p
            .complete_inline(&InlineCompletionRequest {
                prefix: "let x = ".into(),
                suffix: ";".into(),
                language: None,
                max_tokens: None,
            })
            .unwrap();
        assert_eq!(out.text, "INLINE-FILL");
    }
}
