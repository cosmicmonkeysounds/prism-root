//! TextMate grammar generator for Loom v3.
//!
//! The grammar is **derived** from [`loom_parser::keywords`]: adding
//! a keyword or declaration kind in the parser automatically extends
//! the editor highlights on the next `prism codegen loom-tmgrammar`.
//!
//! The five-bracket discipline (spec §5) gives the grammar its top-
//! level scopes: `( … )`, `{ … }`, `[ … ]`, `< … >`, and
//! ` ``` … ``` ` each get their own scope name so themes can colour
//! them independently.
//!
//! Phase-1 stub.

/// Emit the canonical `loom.tmLanguage.json` as a serialised JSON
/// string. Phase-1: returns a minimal placeholder grammar that
/// matches the `.loom` file extension and nothing else.
pub fn emit_tmgrammar() -> String {
    let value = serde_json::json!({
        "name": "Loom",
        "scopeName": "source.loom",
        "fileTypes": ["loom"],
        "patterns": []
    });
    serde_json::to_string_pretty(&value).expect("tmgrammar JSON is well-formed")
}
