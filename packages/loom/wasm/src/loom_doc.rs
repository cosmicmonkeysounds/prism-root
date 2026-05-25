//! `LoomDoc` — a thin wasm-bindgen wrapper around `loro::LoroDoc` that
//! the React editor drives directly. The Loom multi-user backbone
//! (see `docs/dev/loom-multiuser.md`) models each workspace as a
//! single `LoroDoc` with three top-level maps:
//!
//! ```text
//! workspace (LoroDoc)
//! ├── meta:   LoroMap
//! ├── files:  LoroMap   { "<path>" -> LoroText }
//! └── assets: LoroMap
//! ```
//!
//! This module exposes the slice the editor needs for Phase 4:
//! snapshot import/export, version-vector based update export, file
//! CRUD on the `files` map, and a synchronous subscription hook the
//! TS sync client uses to forward local edits over the WebSocket.

use std::cell::RefCell;
use std::sync::Arc;

use js_sys::Function;
use loro::{Container, ExportMode, LoroDoc, LoroMap, LoroText, ValueOrContainer, VersionVector};
use wasm_bindgen::prelude::*;

const FILES_MAP: &str = "files";

/// Wrapper around `loro::LoroDoc`. `Clone` is intentionally not
/// derived — every JS caller works against the same handle via
/// `wasm_bindgen`.
#[wasm_bindgen]
pub struct LoomDoc {
    doc: LoroDoc,
}

impl Default for LoomDoc {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl LoomDoc {
    /// Construct a fresh, empty workspace doc.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            doc: LoroDoc::new(),
        }
    }

    /// Import a full snapshot blob from the server (or another peer).
    #[wasm_bindgen(js_name = importSnapshot)]
    pub fn import_snapshot(&self, bytes: &[u8]) -> Result<(), JsError> {
        self.doc
            .import(bytes)
            .map(|_| ())
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Export the full doc as a binary snapshot.
    #[wasm_bindgen(js_name = exportSnapshot)]
    pub fn export_snapshot(&self) -> Result<Vec<u8>, JsError> {
        self.doc
            .export(ExportMode::Snapshot)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Export every update produced after the given version vector.
    /// Pass `None` (or omit) to export from the empty vector — i.e.
    /// the entire op-log as an update blob.
    #[wasm_bindgen(js_name = exportUpdatesSince)]
    pub fn export_updates_since(&self, version: Option<Vec<u8>>) -> Result<Vec<u8>, JsError> {
        let vv = match version {
            Some(bytes) => VersionVector::decode(&bytes).map_err(|e| JsError::new(&e.to_string()))?,
            None => VersionVector::default(),
        };
        self.doc
            .export(ExportMode::updates(&vv))
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Encoded current op-log version vector — opaque bytes the caller
    /// hands back to `export_updates_since` on the next round.
    #[wasm_bindgen(js_name = currentVersion)]
    pub fn current_version(&self) -> Vec<u8> {
        self.doc.oplog_vv().encode()
    }

    /// Apply an update blob coming off the wire.
    #[wasm_bindgen(js_name = applyUpdate)]
    pub fn apply_update(&self, bytes: &[u8]) -> Result<(), JsError> {
        self.doc
            .import(bytes)
            .map(|_| ())
            .map_err(|e| JsError::new(&e.to_string()))
    }

    // ── files map ────────────────────────────────────────────────

    /// List the keys (paths) of every file currently in the doc.
    #[wasm_bindgen(js_name = listFiles)]
    pub fn list_files(&self) -> Vec<JsValue> {
        let map = self.files_map();
        let mut out = Vec::with_capacity(map.len());
        map.for_each(|key, _| out.push(JsValue::from_str(key)));
        out
    }

    /// Read the current text of `path`, or `None` if it doesn't exist.
    #[wasm_bindgen(js_name = getText)]
    pub fn get_text(&self, path: &str) -> Option<String> {
        let map = self.files_map();
        let value = map.get(path)?;
        text_from(value).map(|t| t.to_string())
    }

    /// Replace the text at `path` with `value`. Creates the file if
    /// it doesn't exist. Naive diff: clear then insert. Good enough
    /// for the Phase 4 capability surface — the CodeMirror binding in
    /// `cm-loro.ts` is the place that needs to issue minimal edits.
    #[wasm_bindgen(js_name = setText)]
    pub fn set_text(&self, path: &str, value: &str) -> Result<(), JsError> {
        let map = self.files_map();
        let text = match map.get(path).and_then(text_from) {
            Some(text) => text,
            None => map
                .insert_container(path, LoroText::new())
                .map_err(|e| JsError::new(&e.to_string()))?,
        };
        let current_len = text.len_unicode();
        if current_len > 0 {
            text.delete(0, current_len)
                .map_err(|e| JsError::new(&e.to_string()))?;
        }
        if !value.is_empty() {
            text.insert(0, value)
                .map_err(|e| JsError::new(&e.to_string()))?;
        }
        self.doc.commit();
        Ok(())
    }

    /// Splice into the `LoroText` at `path` directly — the path
    /// CodeMirror takes for minimal edits. `pos` and `del_len` are in
    /// unicode code points.
    #[wasm_bindgen(js_name = spliceText)]
    pub fn splice_text(
        &self,
        path: &str,
        pos: usize,
        del_len: usize,
        insert: &str,
    ) -> Result<(), JsError> {
        let map = self.files_map();
        let text = match map.get(path).and_then(text_from) {
            Some(text) => text,
            None => map
                .insert_container(path, LoroText::new())
                .map_err(|e| JsError::new(&e.to_string()))?,
        };
        if del_len > 0 {
            text.delete(pos, del_len)
                .map_err(|e| JsError::new(&e.to_string()))?;
        }
        if !insert.is_empty() {
            text.insert(pos, insert)
                .map_err(|e| JsError::new(&e.to_string()))?;
        }
        self.doc.commit();
        Ok(())
    }

    /// Remove a file from the workspace.
    #[wasm_bindgen(js_name = deleteFile)]
    pub fn delete_file(&self, path: &str) -> Result<(), JsError> {
        let map = self.files_map();
        if map.get(path).is_some() {
            map.delete(path)
                .map_err(|e| JsError::new(&e.to_string()))?;
            self.doc.commit();
        }
        Ok(())
    }

    /// Rename a file in the workspace. Insert-then-delete; the body
    /// is copied as a single insert (no CRDT merge between authors
    /// who renamed the same file to different names — last writer
    /// wins on the map key, which matches the spec).
    #[wasm_bindgen(js_name = renameFile)]
    pub fn rename_file(&self, from: &str, to: &str) -> Result<(), JsError> {
        if from == to {
            return Ok(());
        }
        let map = self.files_map();
        let Some(src_value) = map.get(from) else {
            return Ok(());
        };
        let Some(src) = text_from(src_value) else {
            return Ok(());
        };
        let body = src.to_string();
        let dst = map
            .insert_container(to, LoroText::new())
            .map_err(|e| JsError::new(&e.to_string()))?;
        if !body.is_empty() {
            dst.insert(0, &body)
                .map_err(|e| JsError::new(&e.to_string()))?;
        }
        map.delete(from)
            .map_err(|e| JsError::new(&e.to_string()))?;
        self.doc.commit();
        Ok(())
    }

    /// Subscribe to every committed change. The callback is invoked
    /// with no arguments after each commit (local or remote import).
    /// The returned [`SubscriptionHandle`] keeps the subscription
    /// alive — drop it (or call `unsubscribe`) to detach.
    pub fn subscribe(&self, callback: Function) -> SubscriptionHandle {
        let wrapped = WasmSend(callback);
        let sub = self.doc.subscribe_root(Arc::new(move |_event| {
            // Ignore any JS error — the editor doesn't have a useful
            // recovery path for a thrown listener.
            let _ = wrapped.0.call0(&JsValue::NULL);
        }));
        SubscriptionHandle {
            inner: RefCell::new(Some(sub)),
        }
    }

    fn files_map(&self) -> LoroMap {
        self.doc.get_map(FILES_MAP)
    }
}

fn text_from(value: ValueOrContainer) -> Option<LoroText> {
    match value {
        ValueOrContainer::Container(Container::Text(t)) => Some(t),
        _ => None,
    }
}

/// Handle returned by [`LoomDoc::subscribe`]. Dropping the handle (or
/// calling [`SubscriptionHandle::unsubscribe`]) detaches the listener.
#[wasm_bindgen]
pub struct SubscriptionHandle {
    inner: RefCell<Option<loro::Subscription>>,
}

/// `Send + Sync` shim for a JS [`Function`]. wasm32 is single-threaded
/// so the unsoundness this normally introduces can't manifest — Loro's
/// `Subscriber` type requires `Send + Sync`, and `js_sys::Function`
/// declares neither.
struct WasmSend<T>(T);

// SAFETY: wasm32 has no threads; the inner JS handle can never escape
// the main JS event loop.
unsafe impl<T> Send for WasmSend<T> {}
unsafe impl<T> Sync for WasmSend<T> {}

#[wasm_bindgen]
impl SubscriptionHandle {
    pub fn unsubscribe(&self) {
        if let Some(sub) = self.inner.borrow_mut().take() {
            sub.unsubscribe();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_set_get_text() {
        let doc = LoomDoc::new();
        doc.set_text("a.loom", "# Hello").unwrap();
        assert_eq!(doc.get_text("a.loom").as_deref(), Some("# Hello"));
        doc.set_text("a.loom", "# Hello world").unwrap();
        assert_eq!(doc.get_text("a.loom").as_deref(), Some("# Hello world"));
    }

    #[test]
    fn deletes_files() {
        // `list_files` itself constructs `JsValue`s and cannot be
        // exercised under host-side `cargo test`; the wasm-pack
        // browser harness covers it. Round-trip the rest of the
        // CRUD surface here.
        let doc = LoomDoc::new();
        doc.set_text("a.loom", "one").unwrap();
        doc.set_text("b.loom", "two").unwrap();
        doc.delete_file("a.loom").unwrap();
        assert!(doc.get_text("a.loom").is_none());
        assert_eq!(doc.get_text("b.loom").as_deref(), Some("two"));
    }

    #[test]
    fn renames_preserve_text() {
        let doc = LoomDoc::new();
        doc.set_text("old.loom", "body").unwrap();
        doc.rename_file("old.loom", "new.loom").unwrap();
        assert!(doc.get_text("old.loom").is_none());
        assert_eq!(doc.get_text("new.loom").as_deref(), Some("body"));
    }

    #[test]
    fn snapshot_round_trips() {
        let a = LoomDoc::new();
        a.set_text("a.loom", "alpha").unwrap();
        let snap = a.export_snapshot().unwrap();
        let b = LoomDoc::new();
        b.import_snapshot(&snap).unwrap();
        assert_eq!(b.get_text("a.loom").as_deref(), Some("alpha"));
    }

    #[test]
    fn updates_since_version_roundtrip() {
        let a = LoomDoc::new();
        a.set_text("a.loom", "v1").unwrap();
        let baseline = a.current_version();
        a.set_text("a.loom", "v1-extended").unwrap();
        let delta = a.export_updates_since(Some(baseline)).unwrap();

        let b = LoomDoc::new();
        b.import_snapshot(&a.export_snapshot().unwrap()).unwrap();
        // re-applying the delta is a no-op for the source peer; we
        // just confirm it decodes against a doc that already saw it.
        b.apply_update(&delta).unwrap();
        assert_eq!(b.get_text("a.loom").as_deref(), Some("v1-extended"));
    }

    #[test]
    fn splice_edits_minimally() {
        let doc = LoomDoc::new();
        doc.set_text("a.loom", "abcdef").unwrap();
        doc.splice_text("a.loom", 2, 2, "XYZ").unwrap();
        assert_eq!(doc.get_text("a.loom").as_deref(), Some("abXYZef"));
    }
}

