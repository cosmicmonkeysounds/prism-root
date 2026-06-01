//! wasm-bindgen wrapper around [`loom_lsp::Workspace`].
//!
//! The desktop binary `loom-lsp` pumps the same `Workspace` over
//! stdio JSON-RPC; the browser editor instead constructs one
//! `LspWorkspace` on boot, pushes document open / update / close
//! events into it as the user edits, and queries
//! `completion` / `hover` / `definition` / `documentSymbols` /
//! `diagnostics` synchronously.
//!
//! Every query returns the canonical `lsp-types` shape as a JS
//! object via `serde_wasm_bindgen` — this is exactly the on-wire
//! LSP protocol shape, so the editor side can fold the result into
//! its CodeMirror integration with no adapter layer.

use loom_lsp::Workspace;
use lsp_types::{Position, Url};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// A long-lived workspace handle. Construct once on editor boot; push
/// document state through `open` / `update` / `close`; query as the
/// user types.
#[wasm_bindgen]
pub struct LspWorkspace {
    inner: Workspace,
}

#[wasm_bindgen]
impl LspWorkspace {
    /// Build a fresh workspace with no documents.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: Workspace::new(),
        }
    }

    /// Open a document, or replace its text if it already exists.
    /// `uri` must be a valid URI string (e.g. `inmemory://a.loom`).
    #[wasm_bindgen]
    pub fn open(&mut self, uri: &str, text: &str) -> Result<(), JsError> {
        let url = parse_url(uri)?;
        self.inner.open(url, text.to_string());
        Ok(())
    }

    /// Replace the text of an already-open document and rebuild the
    /// project index.
    #[wasm_bindgen]
    pub fn update(&mut self, uri: &str, text: &str) -> Result<(), JsError> {
        let url = parse_url(uri)?;
        self.inner.update(url, text.to_string());
        Ok(())
    }

    /// Drop a document.
    #[wasm_bindgen]
    pub fn close(&mut self, uri: &str) -> Result<(), JsError> {
        let url = parse_url(uri)?;
        self.inner.close(&url);
        Ok(())
    }

    /// Latest diagnostics for `uri`, shaped as the LSP
    /// `PublishDiagnosticsParams` payload (the same shape the stdio
    /// server pushes). Returns `null` if the document is unknown.
    #[wasm_bindgen]
    pub fn diagnostics(&self, uri: &str) -> Result<JsValue, JsError> {
        let url = parse_url(uri)?;
        to_js(&self.inner.diagnostics_for(&url))
    }

    /// Completion items at the given zero-based `line`/`character`
    /// (LSP `Position`). Returns an array of `CompletionItem`.
    #[wasm_bindgen]
    pub fn completion(&self, uri: &str, line: u32, character: u32) -> Result<JsValue, JsError> {
        let url = parse_url(uri)?;
        let items = self.inner.completion_at(&url, Position { line, character });
        to_js(&items)
    }

    /// Hover at the given position, or `null` if there's nothing to
    /// show.
    #[wasm_bindgen]
    pub fn hover(&self, uri: &str, line: u32, character: u32) -> Result<JsValue, JsError> {
        let url = parse_url(uri)?;
        let hover = self.inner.hover_at(&url, Position { line, character });
        to_js(&hover)
    }

    /// Goto-definition target(s) at the given position, shaped as an
    /// LSP `GotoDefinitionResponse`, or `null` if nothing is
    /// resolvable.
    #[wasm_bindgen]
    pub fn definition(&self, uri: &str, line: u32, character: u32) -> Result<JsValue, JsError> {
        let url = parse_url(uri)?;
        let def = self.inner.definition_at(&url, Position { line, character });
        to_js(&def)
    }

    /// Document outline (`DocumentSymbolResponse`), or `null` if the
    /// document is unknown.
    #[wasm_bindgen(js_name = documentSymbols)]
    pub fn document_symbols(&self, uri: &str) -> Result<JsValue, JsError> {
        let url = parse_url(uri)?;
        to_js(&self.inner.document_symbols(&url))
    }

    /// References to the identifier under the cursor — every
    /// occurrence of the same alphanumeric/underscore token in every
    /// open document. Returns `Location[]`.
    #[wasm_bindgen]
    pub fn references(
        &self,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Result<JsValue, JsError> {
        let url = parse_url(uri)?;
        let refs = self
            .inner
            .references_at(&url, lsp_types::Position { line, character });
        to_js(&refs)
    }

    /// Convenience: references to a *named* identifier without a
    /// position. Synthesises a query by scanning every document for
    /// `name`. Used by the editor's References panel when the user
    /// pins a character / beat / world key in the focus bus (no
    /// cursor — only the name).
    #[wasm_bindgen(js_name = referencesByName)]
    pub fn references_by_name(&self, name: &str) -> Result<JsValue, JsError> {
        use lsp_types::{Location, Position, Range};
        let mut out: Vec<Location> = Vec::new();
        for (uri, doc) in self.inner.docs.iter() {
            for (line_no, line_text) in doc.text.lines().enumerate() {
                for (start, end) in loom_lsp::references::find_token_spans(line_text, name) {
                    out.push(Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position {
                                line: line_no as u32,
                                character: start as u32,
                            },
                            end: Position {
                                line: line_no as u32,
                                character: end as u32,
                            },
                        },
                    });
                }
            }
        }
        to_js(&out)
    }
}

impl Default for LspWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_url(uri: &str) -> Result<Url, JsError> {
    Url::parse(uri).map_err(|e| JsError::new(&format!("invalid uri {uri:?}: {e}")))
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    serde_wasm_bindgen::to_value(value).map_err(|e| JsError::new(&e.to_string()))
}
