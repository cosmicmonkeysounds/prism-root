//! `panel-id` → content-tag mapping. Single source of truth for
//! "what visual lives in this dock leaf?" — the dock skeleton
//! authors a `<shell.dock-panel panel-id="builder"/>` and the
//! routing table tells the panel block which `shell.<...>` content
//! tag to embed inside.
//!
//! Adding a new dockable panel is one row here plus one
//! `Block` impl + `register_shell_builtins` row. No router-arm
//! match in any block body, no per-panel dispatch in
//! `dispatch_event`. The §16 promise — "every panel is one row in
//! a declarative table" — extended to the runtime composition side.

/// One row per built-in dockable panel. The first entry is the
/// canonical `panel-id` (matches `prism_dock::PanelKind::id()`),
/// the second is the registered shell tag rendered inside the
/// dock leaf.
const PANEL_ROUTES: &[(&str, &str)] = &[
    ("builder", "shell.builder-canvas"),
    ("inspector", "shell.inspector-tree"),
    ("properties", "shell.properties-panel"),
    ("explorer", "shell.explorer"),
    ("code-editor", "shell.code-editor"),
    ("component-palette", "shell.component-palette"),
    ("signals", "shell.signals-panel"),
    ("navigation", "shell.nav-graph"),
    ("schema", "shell.schema-designer"),
    ("docs", "shell.docs-view"),
];

/// Resolve a `panel-id` to its content tag. `None` means the dock
/// leaf renders empty (for unknown panels — a live document might
/// surface an unknown id during a partial migration; the leaf
/// stays a coherent dock-panel chrome with no content rather than
/// crashing the renderer).
pub fn tag_for_panel(panel_id: &str) -> Option<&'static str> {
    PANEL_ROUTES
        .iter()
        .find(|(id, _)| *id == panel_id)
        .map(|(_, tag)| *tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_panels_resolve() {
        assert_eq!(tag_for_panel("builder"), Some("shell.builder-canvas"));
        assert_eq!(tag_for_panel("inspector"), Some("shell.inspector-tree"));
        assert_eq!(tag_for_panel("properties"), Some("shell.properties-panel"));
    }

    #[test]
    fn unknown_panels_return_none() {
        assert_eq!(tag_for_panel(""), None);
        assert_eq!(tag_for_panel("not-a-panel"), None);
    }

    #[test]
    fn every_route_targets_a_registered_tag() {
        // §16 discipline: every panel route MUST point at a tag in
        // `register_shell_builtins`. Forgetting to register the
        // content tag is a test failure here, not a blank panel
        // surfaced at runtime.
        let mut reg = crate::components::ShellComponentRegistry::new();
        crate::components::register_shell_builtins(&mut reg).expect("register");
        for (panel_id, tag) in PANEL_ROUTES {
            assert!(
                reg.get(tag).is_some(),
                "panel `{panel_id}` routes to unregistered tag `{tag}`"
            );
        }
    }
}
