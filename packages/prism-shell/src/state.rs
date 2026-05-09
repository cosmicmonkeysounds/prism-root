//! `AppState` — slot-typed root of every datum a binding can read.
//!
//! Each top-level field is a *slot*: a typed sub-state owned by one
//! domain (chrome, workspace, selection, overlay, builder, project,
//! …). Slots own their own typed accessors and JSON emitters, so:
//!
//! * The bindings table in [`crate::props`] never reaches across
//!   slot boundaries — every closure asks the slot it cares about
//!   for one fully-shaped JSON value.
//! * `panel_props.rs`-style helpers move *onto* the slot they
//!   describe; there is no separate "shape this state into a row"
//!   layer that the closures have to plumb through.
//! * Adding a new datum is one struct field on the right slot and
//!   one method that returns the JSON shape its block consumes.
//!   Existing bindings keep compiling.
//!
//! See `docs/dev/clay-migration-plan.md` §19.

use serde_json::{json, Value};

/// Reloadable root state. `Default` returns the zero-data shell that
/// the §17 contract boots into; ports re-introduce real data slot by
/// slot.
#[derive(Default, Clone)]
pub struct AppState {
    pub chrome: ChromeSlot,
}

/// Static chrome strings — app name in the menu row, status string
/// in the bottom bar. Two bindings (`shell.app-window`,
/// `shell.status-bar`) read from this slot; both go through methods
/// here, never inline JSON construction in the closures.
#[derive(Clone, Debug)]
pub struct ChromeSlot {
    pub app_name: String,
    pub status: String,
}

impl Default for ChromeSlot {
    fn default() -> Self {
        Self {
            app_name: "Prism".into(),
            status: "Ready".into(),
        }
    }
}

impl ChromeSlot {
    /// JSON for `shell.app-window`. The skeleton's structural attrs
    /// (`id`, `panel-id`) win over emissions, so this method emits
    /// only data attrs — chrome, not identity.
    pub fn app_window_props(&self) -> Value {
        json!({
            "app-name": self.app_name,
            "status": self.status,
            "menus": [
                { "label": "File" },
                { "label": "Edit" },
                { "label": "View" },
                { "label": "Help" },
            ],
            "tabs": [],
            "nav-buttons": [{ "icon": "icons/home.svg", "selected": true }],
        })
    }

    /// JSON for `shell.status-bar`. Same shape contract: chrome data
    /// only, no structural keys.
    pub fn status_bar_props(&self) -> Value {
        json!({ "status": self.status })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_chrome_emits_app_name_and_status() {
        let state = AppState::default();
        let props = state.chrome.app_window_props();
        assert_eq!(props["app-name"], "Prism");
        assert_eq!(props["status"], "Ready");
        assert!(props["menus"].is_array());
    }

    #[test]
    fn status_bar_props_carries_status_only() {
        let mut state = AppState::default();
        state.chrome.status = "Saving…".into();
        let props = state.chrome.status_bar_props();
        assert_eq!(props["status"], "Saving…");
        assert!(props.get("app-name").is_none());
    }
}
