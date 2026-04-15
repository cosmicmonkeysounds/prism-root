//! `LuauSyntaxProvider` — stub implementation of [`SyntaxProvider`]
//! for Luau.
//!
//! Port of `language/luau/luau-provider.ts`. The pre-Rust version
//! layered `createLuauSyntaxProvider` on top of the full-moon WASM
//! parser and fed diagnostics/completions back into Studio's
//! `SyntaxEngine`. The Rust port lands the trait implementation
//! first; the diagnostic/completion bodies fill in when the
//! full-moon port is wired through `LanguageContribution::parse`.
//!
//! Keeping the provider present (even as a stub) lets the unified
//! registry resolve `contribution.syntax_provider` without a feature
//! gate, and lets downstream code register it with
//! `SyntaxEngine::register_provider` today.

use crate::language::syntax::{
    CompletionItem, Diagnostic, HoverInfo, SchemaContext, SyntaxProvider,
};

/// Default provider returned by the Luau contribution. Every hook
/// returns an empty result until the full-moon parser ports, at
/// which point the provider will forward into the shared AST
/// helpers (the same path `findUiCalls` / `validateLuau` used on
/// the TS side).
#[derive(Debug, Clone, Default)]
pub struct LuauSyntaxProvider;

impl LuauSyntaxProvider {
    pub fn new() -> Self {
        Self
    }
}

impl SyntaxProvider for LuauSyntaxProvider {
    fn name(&self) -> &str {
        "prism:luau"
    }

    fn diagnose(&self, _source: &str, _context: Option<&SchemaContext>) -> Vec<Diagnostic> {
        Vec::new()
    }

    fn complete(
        &self,
        _source: &str,
        _offset: usize,
        _context: Option<&SchemaContext>,
    ) -> Vec<CompletionItem> {
        Vec::new()
    }

    fn hover(
        &self,
        _source: &str,
        _offset: usize,
        _context: Option<&SchemaContext>,
    ) -> Option<HoverInfo> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_name_matches_contribution_id() {
        let provider = LuauSyntaxProvider::new();
        assert_eq!(provider.name(), "prism:luau");
    }

    #[test]
    fn stub_returns_empty_diagnostics() {
        let provider = LuauSyntaxProvider::new();
        assert!(provider.diagnose("return 1", None).is_empty());
        assert!(provider.complete("return 1", 0, None).is_empty());
        assert!(provider.hover("return 1", 0, None).is_none());
    }
}
