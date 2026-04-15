//! `CodegenPipeline` — dispatches heterogeneous emitters by input kind.
//!
//! Port of `language/codegen/codegen-pipeline.ts`. The TS version used
//! a mutable array of `Emitter` objects and dispatched via a shared
//! input bundle; the Rust port does the same with `Box<dyn Emitter>`
//! and `CodegenInputs` (see [`super::types`]).
//!
//! Emitters whose slot on `CodegenInputs` is missing are skipped
//! silently, matching the TS semantics: callers register a full set
//! of emitters once and opt in per run by populating the slots they
//! actually want to emit.

use super::types::{CodegenInputs, CodegenMeta, CodegenResult, Emitter};

/// Accepts a heterogeneous list of emitters and fans inputs out to
/// each one based on its declared `input_kind`.
#[derive(Default)]
pub struct CodegenPipeline {
    emitters: Vec<Box<dyn Emitter>>,
}

impl CodegenPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an emitter. Chainable — mirrors the TS fluent style.
    pub fn register(mut self, emitter: impl Emitter + 'static) -> Self {
        self.emitters.push(Box::new(emitter));
        self
    }

    /// Register a boxed emitter. Useful when the concrete type is
    /// erased (e.g. when building the pipeline from a config file).
    pub fn register_boxed(mut self, emitter: Box<dyn Emitter>) -> Self {
        self.emitters.push(emitter);
        self
    }

    /// Dispatch every registered emitter to its matching slot on
    /// `inputs`. Aggregates files and errors across all emitters.
    /// Each error is prefixed with `[emitter-id]` like in the TS
    /// version.
    pub fn run(&self, inputs: &CodegenInputs, meta: &CodegenMeta) -> CodegenResult {
        let mut result = CodegenResult::new();
        for emitter in &self.emitters {
            let kind = emitter.input_kind();
            let Some(slot) = inputs.any(kind) else {
                continue;
            };
            let emitted = emitter.emit(slot, meta);
            result.files.extend(emitted.files);
            for err in emitted.errors {
                result.errors.push(format!("[{}] {err}", emitter.id()));
            }
        }
        result
    }

    /// Read-only view of the registered emitters — matches the TS
    /// `emitters()` accessor.
    pub fn emitters(&self) -> &[Box<dyn Emitter>] {
        &self.emitters
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::EmittedFile;
    use super::*;

    struct RecordingEmitter {
        id: String,
        kind: String,
    }

    impl Emitter for RecordingEmitter {
        fn id(&self) -> &str {
            &self.id
        }
        fn input_kind(&self) -> &str {
            &self.kind
        }
        fn emit(&self, input: &dyn std::any::Any, _meta: &CodegenMeta) -> CodegenResult {
            let count = input
                .downcast_ref::<Vec<String>>()
                .map(Vec::len)
                .unwrap_or(0);
            CodegenResult {
                files: vec![EmittedFile {
                    filename: format!("{}.out", self.id),
                    content: format!("count={count}"),
                    language: "text".into(),
                }],
                errors: vec![],
            }
        }
    }

    struct FailingEmitter;

    impl Emitter for FailingEmitter {
        fn id(&self) -> &str {
            "boom"
        }
        fn input_kind(&self) -> &str {
            "symbols"
        }
        fn emit(&self, _input: &dyn std::any::Any, _meta: &CodegenMeta) -> CodegenResult {
            CodegenResult {
                files: vec![],
                errors: vec!["something went wrong".into()],
            }
        }
    }

    #[test]
    fn run_dispatches_to_matching_slot() {
        let pipeline = CodegenPipeline::new().register(RecordingEmitter {
            id: "rec".into(),
            kind: "symbols".into(),
        });
        let inputs = CodegenInputs::new().with("symbols", vec!["a".to_string(), "b".into()]);
        let result = pipeline.run(&inputs, &CodegenMeta::new("test"));
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].content, "count=2");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn run_skips_emitters_whose_slot_is_missing() {
        let pipeline = CodegenPipeline::new().register(RecordingEmitter {
            id: "rec".into(),
            kind: "schema".into(),
        });
        let inputs = CodegenInputs::new().with("symbols", vec!["a".to_string()]);
        let result = pipeline.run(&inputs, &CodegenMeta::new("test"));
        assert!(result.files.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn run_prefixes_errors_with_emitter_id() {
        let pipeline = CodegenPipeline::new().register(FailingEmitter);
        let inputs = CodegenInputs::new().with("symbols", vec!["x".to_string()]);
        let result = pipeline.run(&inputs, &CodegenMeta::new("test"));
        assert_eq!(
            result.errors,
            vec!["[boom] something went wrong".to_string()]
        );
    }

    #[test]
    fn emitters_accessor_returns_registered_set() {
        let pipeline = CodegenPipeline::new()
            .register(RecordingEmitter {
                id: "a".into(),
                kind: "symbols".into(),
            })
            .register(RecordingEmitter {
                id: "b".into(),
                kind: "schema".into(),
            });
        let ids: Vec<&str> = pipeline.emitters().iter().map(|e| e.id()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }
}
