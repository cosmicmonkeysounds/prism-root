//! Shell-resident Luau handles. Phase 5 of
//! `docs/dev/luau-integration-plan.md`: scripts running inside the
//! shell see `prism.document` / `prism.signals` / `prism.selection` /
//! `prism.app` on top of the daemon's stateless [`PrismContext`].
//!
//! The handles wrap shared `Rc<RefCell<…>>` state. Mutations are
//! queued during script execution and drained by the caller after the
//! script returns, which keeps Lua's borrow rules clean (no
//! re-entrant calls back into [`ShellInner`] mid-script) and lets the
//! shell apply queued ops through the same `live`-source-edit / undo
//! path the [`apply_luau_result`](crate::app::ShellInner::apply_luau_result)
//! `_actions` protocol uses.

pub mod document;
pub mod signals;

use std::cell::RefCell;
use std::rc::Rc;

use mlua::Lua;
use prism_builder::{BuilderDocument, Node};

pub use document::{DocumentHandle, DocumentMode, DocumentMutation};
pub use signals::{SignalEntry, SignalsHandle};

use prism_daemon::modules::prism_context::PrismContext;

/// Lightweight read-only mirror of the active app + selection used by
/// `prism.app` / `prism.selection` getters. Built fresh per script
/// run, so scripts always see a consistent snapshot.
#[derive(Clone, Default)]
pub struct ShellSnapshot {
    pub active_app_id: Option<String>,
    pub active_page_id: Option<String>,
    pub active_page_route: Option<String>,
    pub selected_node: Option<String>,
    pub selected_nodes: Vec<String>,
}

/// Bag of handles installed onto the Lua state alongside the daemon's
/// `prism` userdata. The caller drains queued mutations via the
/// `*_queue` fields after the script returns.
pub struct ShellHandles {
    pub document: DocumentHandle,
    pub signals: SignalsHandle,
    pub document_queue: Rc<RefCell<Vec<DocumentMutation>>>,
    pub signal_queue: Rc<RefCell<Vec<SignalEntry>>>,
    pub snapshot: ShellSnapshot,
}

impl ShellHandles {
    pub fn new(document: BuilderDocument, mode: DocumentMode, snapshot: ShellSnapshot) -> Self {
        let document_queue: Rc<RefCell<Vec<DocumentMutation>>> = Rc::new(RefCell::new(Vec::new()));
        let signal_queue: Rc<RefCell<Vec<SignalEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let document = DocumentHandle::new(Rc::new(document), mode, document_queue.clone());
        let signals = SignalsHandle::new(signal_queue.clone());
        Self {
            document,
            signals,
            document_queue,
            signal_queue,
            snapshot,
        }
    }

    /// Install the shell-side handles onto `lua` and rewire the
    /// `prism` global so `prism.document` / `prism.signals` /
    /// `prism.selection` / `prism.app` resolve to them while every
    /// other field falls through to the daemon's userdata via
    /// `__index`.
    pub fn install(&self, lua: &Lua) -> mlua::Result<()> {
        let globals = lua.globals();
        let prism_user: mlua::Value = globals.get("prism")?;
        let wrapper = lua.create_table()?;
        wrapper.set("document", self.document.clone())?;
        wrapper.set("signals", self.signals.clone())?;

        let selection = lua.create_table()?;
        if let Some(id) = &self.snapshot.selected_node {
            selection.set("primary", id.clone())?;
        }
        selection.set("node_ids", self.snapshot.selected_nodes.clone())?;
        selection.set("is_multi", self.snapshot.selected_nodes.len() > 1)?;
        wrapper.set("selection", selection)?;

        let app = lua.create_table()?;
        if let Some(id) = &self.snapshot.active_app_id {
            app.set("id", id.clone())?;
        }
        if let Some(id) = &self.snapshot.active_page_id {
            app.set("page_id", id.clone())?;
        }
        if let Some(route) = &self.snapshot.active_page_route {
            app.set("page_route", route.clone())?;
        }
        wrapper.set("app", app)?;

        let mt = lua.create_table()?;
        mt.set("__index", prism_user)?;
        wrapper.set_metatable(Some(mt));

        globals.set("prism", wrapper)?;
        Ok(())
    }
}

/// Build a snapshot of the active page's source so scripts can be
/// executed against a stable view. The shell calls this just before
/// running the script.
pub fn snapshot_from_state(state: &crate::app::AppState) -> ShellSnapshot {
    let active_app = state.active_app();
    let active_page = active_app.and_then(|a| a.pages.get(a.active_page));
    ShellSnapshot {
        active_app_id: active_app.map(|a| a.id.clone()),
        active_page_id: active_page.map(|p| p.id.clone()),
        active_page_route: active_page.map(|p| p.route.clone()),
        selected_node: state.selection.primary().cloned(),
        selected_nodes: state.selection.items().to_vec(),
    }
}

/// Construct a [`PrismContext`] suited to a shell-side Luau run.
/// Wraps the live collection so `prism.objects` / `prism.edges` work.
pub fn shell_prism_context(
    collection: Rc<RefCell<prism_core::foundation::persistence::CollectionStore>>,
) -> PrismContext {
    PrismContext::default().with_collection(collection)
}

/// Extract the new [`Node`] descriptor a script passed to
/// `prism.document:insert(...)`. Mirrors the loose JSON shape we
/// already accept in the legacy `_actions` protocol.
pub fn descriptor_to_node(desc: &serde_json::Value, next_id: &mut u64) -> Option<Node> {
    let obj = desc.as_object()?;
    let component = obj.get("component")?.as_str()?.to_string();
    let id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| {
            let n = *next_id;
            *next_id = next_id.saturating_add(1);
            format!("node-luau-{n}")
        });
    let props = obj
        .get("props")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let children: Vec<Node> = obj
        .get("children")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| descriptor_to_node(v, next_id))
                .collect()
        })
        .unwrap_or_default();
    Some(Node {
        id,
        component,
        props,
        children,
        ..Default::default()
    })
}
