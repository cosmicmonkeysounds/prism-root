//! **Wave E.3** (`docs/dev/prui-luau-fusion.md` §7.8) — the bundled
//! sub-dialects, shipped as `.luau` source and embedded at compile
//! time. A host (prism-shell / prism-relay) hands these to the
//! runtime via `LowerScope::with_builtin_scripts(...)`; the runtime
//! prepends them as flat modules ahead of every dialect-using
//! document's own `<script>` blocks, so `<markdown>` / `~sql{…}`
//! resolve without the author wiring `prism.dialect{…}` by hand.
//!
//! Each file is a self-contained `prism.dialect{ name, parse }`
//! registration that returns PRUI source (built with the
//! `prui_ast.*` constructor table) the runtime re-parses + lowers.
//! "Every dialect is a Luau file, not a parser fork" — the runtime
//! mechanism (Wave E.1/E.2) is the seam; this is the bundled corpus.

/// `markdown` — block-level headings / bullets / paragraphs.
pub const MARKDOWN: &str = include_str!("../dialects/markdown.luau");
/// `mermaid` — author intent captured as an inspectable node tree.
pub const MERMAID: &str = include_str!("../dialects/mermaid.luau");
/// `sql-view` — the query surfaced as a first-class block.
pub const SQL_VIEW: &str = include_str!("../dialects/sql-view.luau");

/// Every bundled dialect's Luau source, in registration order.
/// Pass straight to `prism_ui_runtime::LowerScope::with_builtin_scripts`.
pub fn builtin_dialect_sources() -> Vec<&'static str> {
    vec![MARKDOWN, MERMAID, SQL_VIEW]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ships_three_dialects_each_registering() {
        let srcs = builtin_dialect_sources();
        assert_eq!(srcs.len(), 3);
        for s in srcs {
            assert!(s.contains("prism.dialect"), "must register a dialect");
            assert!(s.contains("parse"), "must carry a parse fn");
        }
    }

    #[test]
    fn each_dialect_source_is_valid_luau() {
        // The bundled scripts must parse cleanly — a broken bundled
        // dialect would silently no-op every `<markdown>` in the
        // workspace.
        for (name, src) in [
            ("markdown", MARKDOWN),
            ("mermaid", MERMAID),
            ("sql-view", SQL_VIEW),
        ] {
            let errs = prism_core::language::luau::parse_errors(src);
            assert!(errs.is_empty(), "{name} has Luau errors: {errs:?}");
        }
    }
}
