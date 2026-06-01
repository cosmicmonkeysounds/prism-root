//! wasm-bindgen surface for the Loom v3 parser.
//!
//! Compiled to `wasm32-unknown-unknown` via `wasm-pack build`, this
//! crate produces a JS module the web editor (packages/loom/editor)
//! imports to get live parsing + diagnostics for `.loom` buffers.
//!
//! The public surface is intentionally small:
//!
//! * [`parse`] — full AST + diagnostic stream as a JSON-compatible
//!   JS object (one shape, no schema drift between versions because
//!   the AST derives `Serialize`).
//! * [`diagnose`] — diagnostics only, for lint passes that don't
//!   need the AST.
//! * [`emit_tmgrammar`] — re-export of `loom_syntax::emit_tmgrammar`
//!   so editor builds can fetch the same grammar JSON the Zed /
//!   VSCode extensions ship.

use serde::Serialize;
use wasm_bindgen::prelude::*;

pub mod loom_doc;
pub mod lsp_workspace;
pub use loom_doc::{LoomDoc, SubscriptionHandle};
pub use lsp_workspace::LspWorkspace;

#[wasm_bindgen(start)]
pub fn init() {
    // Surface Rust panics as readable JS console errors during dev.
    // No-op once compiled into wasm targets that strip the symbol.
    console_error_panic_hook::set_once();
}

/// Mirrors `loom_parser::Severity` but uses lowercase strings so the
/// JS side can treat it as the literal CodeMirror severity tag.
#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JsSeverity {
    Error,
    Warning,
}

impl From<loom_parser::Severity> for JsSeverity {
    fn from(value: loom_parser::Severity) -> Self {
        match value {
            loom_parser::Severity::Error => JsSeverity::Error,
            loom_parser::Severity::Warning => JsSeverity::Warning,
        }
    }
}

#[derive(Serialize)]
pub struct JsDiagnostic {
    pub code: &'static str,
    pub severity: JsSeverity,
    pub message: String,
    pub from: u32,
    pub to: u32,
    pub line: u32,
    pub column: u32,
}

impl From<&loom_parser::Diagnostic> for JsDiagnostic {
    fn from(d: &loom_parser::Diagnostic) -> Self {
        Self {
            code: d.code.id(),
            severity: d.severity.into(),
            message: d.message.clone(),
            from: d.span.start.byte,
            to: d.span.end.byte,
            line: d.span.start.line,
            column: d.span.start.column,
        }
    }
}

#[derive(Serialize)]
pub struct ParseResult {
    pub ast: loom_parser::LoomFile,
    pub diagnostics: Vec<JsDiagnostic>,
}

/// Parse a `.loom` source string into AST + diagnostics. Returns a JS
/// object: `{ ast: LoomFile, diagnostics: JsDiagnostic[] }`.
#[wasm_bindgen]
pub fn parse(source: &str) -> Result<JsValue, JsError> {
    let (ast, diagnostics) = loom_parser::parse(source);
    let result = ParseResult {
        ast,
        diagnostics: diagnostics.iter().map(JsDiagnostic::from).collect(),
    };
    serde_wasm_bindgen::to_value(&result).map_err(|e| JsError::new(&e.to_string()))
}

/// Set a beat's contract property (`cast:`, `setting:`, …) and return
/// the rewritten source. A span-preserving splice: every untouched
/// line is byte-identical (IDE redesign v2 §10 — edits round-trip to
/// `.loom` source). Inserts the line if the property is absent;
/// returns the source unchanged when the value already matches.
#[wasm_bindgen]
pub fn apply_beat_property(
    source: &str,
    beat: &str,
    key: &str,
    value: &str,
) -> Result<String, JsError> {
    let (file, _) = loom_parser::parse(source);
    let edits = loom_parser::set_beat_property(source, &file, beat, key, value)
        .map_err(|e| JsError::new(&e.to_string()))?;
    loom_parser::apply_edits(source, &edits).map_err(|e| JsError::new(&e.to_string()))
}

/// Move a beat relative to another (or to file start / end) and return
/// the rewritten source. `anchor_kind` is `before` / `after` / `start`
/// / `end`; `anchor_name` names the reference beat for `before` /
/// `after` (ignored otherwise). The moved beat's bytes and every other
/// beat's bytes are preserved; only the seam separators are normalised.
#[wasm_bindgen]
pub fn apply_move_beat(
    source: &str,
    beat: &str,
    anchor_kind: &str,
    anchor_name: &str,
) -> Result<String, JsError> {
    let anchor = match anchor_kind {
        "before" => loom_parser::Anchor::Before(anchor_name.to_string()),
        "after" => loom_parser::Anchor::After(anchor_name.to_string()),
        "start" => loom_parser::Anchor::Start,
        "end" => loom_parser::Anchor::End,
        other => return Err(JsError::new(&format!("unknown anchor kind: {other}"))),
    };
    let (file, _) = loom_parser::parse(source);
    let edits = loom_parser::move_beat(source, &file, beat, anchor)
        .map_err(|e| JsError::new(&e.to_string()))?;
    loom_parser::apply_edits(source, &edits).map_err(|e| JsError::new(&e.to_string()))
}

/// Insert a new empty `== name` beat at `anchor_kind` (`before` /
/// `after` / `start` / `end`; `anchor_name` is the reference beat for
/// before/after). Returns the rewritten source.
#[wasm_bindgen]
pub fn apply_insert_beat(
    source: &str,
    name: &str,
    anchor_kind: &str,
    anchor_name: &str,
) -> Result<String, JsError> {
    let anchor = match anchor_kind {
        "before" => loom_parser::Anchor::Before(anchor_name.to_string()),
        "after" => loom_parser::Anchor::After(anchor_name.to_string()),
        "start" => loom_parser::Anchor::Start,
        "end" => loom_parser::Anchor::End,
        other => return Err(JsError::new(&format!("unknown anchor kind: {other}"))),
    };
    let (file, _) = loom_parser::parse(source);
    let edits = loom_parser::insert_beat(source, &file, name, anchor)
        .map_err(|e| JsError::new(&e.to_string()))?;
    loom_parser::apply_edits(source, &edits).map_err(|e| JsError::new(&e.to_string()))
}

/// Delete the beat named `beat`, returning the rewritten source.
#[wasm_bindgen]
pub fn apply_remove_beat(source: &str, beat: &str) -> Result<String, JsError> {
    let (file, _) = loom_parser::parse(source);
    let edits =
        loom_parser::remove_beat(source, &file, beat).map_err(|e| JsError::new(&e.to_string()))?;
    loom_parser::apply_edits(source, &edits).map_err(|e| JsError::new(&e.to_string()))
}

/// Diagnostics-only variant for lint passes that don't need the AST.
/// Roughly 30 % faster on long files because the AST serialisation
/// pass is skipped.
#[wasm_bindgen]
pub fn diagnose(source: &str) -> Result<JsValue, JsError> {
    let (_ast, diagnostics) = loom_parser::parse(source);
    let js: Vec<JsDiagnostic> = diagnostics.iter().map(JsDiagnostic::from).collect();
    serde_wasm_bindgen::to_value(&js).map_err(|e| JsError::new(&e.to_string()))
}

/// Returns the canonical `loom.tmLanguage.json` so editor builds can
/// pull the same grammar shape the Zed / VSCode extensions ship,
/// without bundling the syntax crate's tests.
#[wasm_bindgen]
pub fn emit_tmgrammar() -> String {
    loom_syntax::emit_tmgrammar()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trip_through_rust_api() {
        let (ast, diags) = loom_parser::parse("# Saltmere\nentry: opening\n");
        assert!(diags.is_empty());
        assert_eq!(ast.header.title.as_deref(), Some("Saltmere"));
    }

    #[test]
    fn js_severity_serializes_lowercase() {
        let json = serde_json::to_string(&JsSeverity::Error).unwrap();
        assert_eq!(json, "\"error\"");
    }

    #[test]
    fn apply_beat_property_rewrites_source() {
        let src = "== a\n  cast: X\n\n== b\n  cast: Y\n";
        let out = apply_beat_property(src, "a", "cast", "Z").ok().unwrap();
        assert!(out.contains("  cast: Z"));
        // The other beat is byte-identical.
        assert!(out.contains("== b\n  cast: Y"));
    }

    #[test]
    fn apply_move_beat_reorders_source() {
        let src = "== a\n  cast: X\n\n== b\n  cast: Y\n";
        let out = apply_move_beat(src, "a", "end", "").ok().unwrap();
        // `b` now precedes `a`.
        assert!(out.find("== b").unwrap() < out.find("== a").unwrap());
    }
}
