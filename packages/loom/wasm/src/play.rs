//! Local, in-browser Loom play sessions.
//!
//! Exposes [`loom_runtime::session::PlaySession`] through wasm-bindgen
//! so the editor can run a full Loom show entirely client-side — no
//! relay, no account, no server round-trip. Every method returns the
//! session's `PlayStateSnapshot` serialized to a **JSON string**
//! (`serde_json`). That string is byte-identical to the `play-state`
//! payload `loom-server` pushes over the relay WebSocket, so the React
//! runner panels (Transcript / World / Timeline / Choices / Cast /
//! Inspector / Ledger / Graph / Booth) consume the local and remote
//! paths through exactly the same `PlayStatePayload` shape.
//!
//! The Luau VM is unavailable on `wasm32-unknown-unknown` (mlua needs
//! the emscripten target). The runtime's directive registry runs in
//! *lenient* mode there, so Lua-defined directives and `.luau`
//! extensions degrade to logged envelopes rather than aborting the
//! session — the narrative flow (beats, choices, diverts, conditionals,
//! world state, characters, stats) is fully native Rust and plays
//! correctly. See `loom_runtime::directives::Registry::lenient`.

use loom_runtime::session::{PlaySession, SessionError};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

/// One `(path, source)` file as handed in from JS (`{ path, source }`).
#[derive(Deserialize)]
struct FileInput {
    path: String,
    source: String,
}

fn parse_files(files: JsValue) -> Result<Vec<(String, String)>, JsError> {
    let parsed: Vec<FileInput> = serde_wasm_bindgen::from_value(files)
        .map_err(|e| JsError::new(&format!("invalid files payload: {e}")))?;
    Ok(parsed.into_iter().map(|f| (f.path, f.source)).collect())
}

fn session_err(e: SessionError) -> JsError {
    JsError::new(&e.to_string())
}

/// A live single-user play session running in the browser.
///
/// Construct with `new(files, starter?)`; advance with `choose`; branch
/// with `fork` / `snapshot` / `restore`; manage heads with `dropHead` /
/// `setPrimary`; live-patch with `boothSkip` / `boothForce` /
/// `boothReload`. Every call returns the updated state as a JSON string
/// — `JSON.parse` it into a `PlayStatePayload`.
#[wasm_bindgen]
pub struct LoomSession {
    inner: PlaySession,
}

#[wasm_bindgen]
impl LoomSession {
    /// Start a local play session from `[{ path, source }]`. `starter`
    /// labels the session host (defaults to `"local"`). Throws if the
    /// bundle fails to build or the entry beat can't be resolved.
    #[wasm_bindgen(constructor)]
    pub fn new(files: JsValue, starter: Option<String>) -> Result<LoomSession, JsError> {
        let sources = parse_files(files)?;
        let inner = PlaySession::new(
            "local".to_string(),
            sources,
            starter.unwrap_or_else(|| "local".to_string()),
        )
        .map_err(session_err)?;
        Ok(LoomSession { inner })
    }

    /// Current session state as a JSON string (`PlayStatePayload`).
    pub fn state(&self) -> Result<String, JsError> {
        self.snapshot_json()
    }

    /// Pick choice `index` on `head` (defaults to the primary head).
    pub fn choose(&mut self, index: usize, head: Option<String>) -> Result<String, JsError> {
        let head = self.resolve_head(head);
        self.inner.choose(&head, index).map_err(session_err)?;
        self.snapshot_json()
    }

    /// Fork a new head off `parent` (defaults to primary), optionally
    /// rewinding to a captured `from_snapshot`.
    pub fn fork(
        &mut self,
        parent: Option<String>,
        from_snapshot: Option<String>,
    ) -> Result<String, JsError> {
        let parent = self.resolve_head(parent);
        self.inner
            .fork(&parent, from_snapshot.as_deref())
            .map_err(session_err)?;
        self.snapshot_json()
    }

    /// Capture a time-travel snapshot of `head` (defaults to primary).
    pub fn snapshot(
        &mut self,
        head: Option<String>,
        label: Option<String>,
    ) -> Result<String, JsError> {
        let head = self.resolve_head(head);
        self.inner.snapshot(&head, label).map_err(session_err)?;
        self.snapshot_json()
    }

    /// Restore `head` to a previously captured `snapshot`.
    pub fn restore(&mut self, head: String, snapshot: String) -> Result<String, JsError> {
        self.inner.restore(&head, &snapshot).map_err(session_err)?;
        self.snapshot_json()
    }

    /// Drop a non-primary head.
    #[wasm_bindgen(js_name = dropHead)]
    pub fn drop_head(&mut self, head: String) -> Result<String, JsError> {
        self.inner.drop_head(&head).map_err(session_err)?;
        self.snapshot_json()
    }

    /// Make `head` the primary (focused) head.
    #[wasm_bindgen(js_name = setPrimary)]
    pub fn set_primary(&mut self, head: String) -> Result<String, JsError> {
        self.inner.set_primary(&head).map_err(session_err)?;
        self.snapshot_json()
    }

    /// Booth live-patch — skip the current beat on `head`.
    #[wasm_bindgen(js_name = boothSkip)]
    pub fn booth_skip(&mut self, head: Option<String>) -> Result<String, JsError> {
        let head = self.resolve_head(head);
        self.inner.booth_skip(&head).map_err(session_err)?;
        self.snapshot_json()
    }

    /// Booth live-patch — inject a raw `<directive>` at the head of the
    /// queue and resume.
    #[wasm_bindgen(js_name = boothForce)]
    pub fn booth_force(&mut self, raw: String, head: Option<String>) -> Result<String, JsError> {
        let head = self.resolve_head(head);
        self.inner.booth_force(&head, raw).map_err(session_err)?;
        self.snapshot_json()
    }

    /// Booth live-patch — hot-reload the bundle from fresh sources under
    /// every head (ledger + world preserved).
    #[wasm_bindgen(js_name = boothReload)]
    pub fn booth_reload(&mut self, files: JsValue) -> Result<String, JsError> {
        let sources = parse_files(files)?;
        self.inner.booth_hot_reload(sources).map_err(session_err)?;
        self.snapshot_json()
    }
}

impl LoomSession {
    fn resolve_head(&self, head: Option<String>) -> String {
        head.unwrap_or_else(|| self.inner.primary().to_string())
    }

    fn snapshot_json(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.inner.snapshot_view())
            .map_err(|e| JsError::new(&e.to_string()))
    }
}
