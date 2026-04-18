//! `VisualLanguage` — the trait concrete languages implement for
//! bidirectional visual↔textual editing.
//!
//! Each language provides `compile` (graph → source) and `decompile`
//! (source → graph). The bridge coordinates sync: edit one
//! representation, the other updates. The graph model is
//! language-agnostic; the bridge implementations are language-specific.

use super::graph::{NodeKindDef, ScriptGraph};
use crate::language::syntax::Diagnostic;

#[derive(Debug, Clone, PartialEq)]
pub struct VisualLanguageError {
    pub message: String,
    pub diagnostics: Vec<Diagnostic>,
}

impl std::fmt::Display for VisualLanguageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for VisualLanguageError {}

/// Trait for languages that support bidirectional visual↔textual editing.
///
/// Implement this for each scripting language (Luau, Loom Lang, etc.)
/// to enable the visual node-graph editor to work alongside the text
/// editor with live sync.
pub trait VisualLanguage: Send + Sync {
    /// Language identifier matching the `LanguageContribution::id`.
    fn language_id(&self) -> &str;

    /// Parse source text into a `ScriptGraph`.
    ///
    /// The graph preserves enough structure to round-trip back to
    /// source via `compile`. Node positions are auto-laid-out on
    /// first decompile; subsequent edits preserve user-placed positions.
    fn decompile(&self, source: &str) -> Result<ScriptGraph, VisualLanguageError>;

    /// Emit source text from a `ScriptGraph`.
    ///
    /// The output must be valid, parseable source in the target
    /// language. Formatting follows language conventions.
    fn compile(&self, graph: &ScriptGraph) -> Result<String, VisualLanguageError>;

    /// Available node kinds for the palette (what the user can drag
    /// into the graph).
    fn node_palette(&self) -> Vec<NodeKindDef>;

    /// Semantic validation beyond syntax — type mismatches, missing
    /// connections, unreachable nodes.
    fn validate(&self, graph: &ScriptGraph) -> Vec<Diagnostic>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubVisualLang;

    impl VisualLanguage for StubVisualLang {
        fn language_id(&self) -> &str {
            "test:stub"
        }

        fn decompile(&self, _source: &str) -> Result<ScriptGraph, VisualLanguageError> {
            Ok(ScriptGraph::new("stub", "Stub"))
        }

        fn compile(&self, _graph: &ScriptGraph) -> Result<String, VisualLanguageError> {
            Ok("-- stub".into())
        }

        fn node_palette(&self) -> Vec<NodeKindDef> {
            Vec::new()
        }

        fn validate(&self, _graph: &ScriptGraph) -> Vec<Diagnostic> {
            Vec::new()
        }
    }

    #[test]
    fn trait_object_compiles() {
        let lang: Box<dyn VisualLanguage> = Box::new(StubVisualLang);
        assert_eq!(lang.language_id(), "test:stub");
        let graph = lang.decompile("x = 1").unwrap();
        let source = lang.compile(&graph).unwrap();
        assert_eq!(source, "-- stub");
    }
}
