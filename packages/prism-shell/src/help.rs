//! Help registration coordinator + entries that don't belong to a
//! specific module.
//!
//! Components register their own help via [`Component::help_entry`],
//! panels via [`Panel::help_entry`]. This module provides:
//!
//! 1. [`register_help_entries`] — the top-level coordinator that pulls
//!    entries from all distributed sources into one registry.
//! 2. Entries for field types, toolbar actions, and shell-level features
//!    that don't have a natural owner struct.

use prism_builder::ComponentRegistry;
use prism_core::help::{HelpEntry, HelpRegistry};

use crate::panels::builder::BuilderPanel;
use crate::panels::identity::IdentityPanel;
use crate::panels::inspector::InspectorPanel;
use crate::panels::properties::PropertiesPanel;
use crate::panels::signals::SignalsPanel;
use crate::panels::Panel;

/// Populate `registry` from all distributed help sources.
///
/// New modules that want help entries should either:
/// - Implement `HelpProvider` / `Component::help_entry` / `Panel::help_entry`
///   and get picked up here automatically, or
/// - Add a helper function returning `Vec<HelpEntry>` and call it below.
pub fn register_help_entries(registry: &mut HelpRegistry, components: &ComponentRegistry) {
    registry.register_provider(components);

    for panel in panels() {
        if let Some(entry) = panel.help_entry() {
            registry.register(entry);
        }
    }

    registry.register_many(field_entries());
    registry.register_many(toolbar_entries());
    registry.register_many(shell_entries());
    registry.register_many(chrome_entries());
}

fn panels() -> Vec<Box<dyn Panel>> {
    vec![
        Box::new(IdentityPanel::new()),
        Box::new(BuilderPanel::new()),
        Box::new(InspectorPanel::new()),
        Box::new(PropertiesPanel::new()),
        Box::new(SignalsPanel::new()),
    ]
}

pub fn field_entries() -> Vec<HelpEntry> {
    vec![
        HelpEntry::new(
            "builder.fields.text",
            "Text Field",
            "Single-line text input. Press Enter to commit the value.",
        ),
        HelpEntry::new(
            "builder.fields.textarea",
            "Text Area",
            "Multi-line text input for longer content.",
        ),
        HelpEntry::new(
            "builder.fields.number",
            "Number",
            "Numeric input with optional min/max bounds.",
        ),
        HelpEntry::new(
            "builder.fields.integer",
            "Integer",
            "Whole number input with optional min/max bounds.",
        ),
        HelpEntry::new(
            "builder.fields.boolean",
            "Boolean",
            "Toggle switch for true/false values.",
        ),
        HelpEntry::new(
            "builder.fields.select",
            "Select",
            "Dropdown selector over a fixed set of options.",
        ),
        HelpEntry::new(
            "builder.fields.color",
            "Color",
            "Hex color input (#RRGGBB or #RRGGBBAA).",
        ),
    ]
}

pub fn toolbar_entries() -> Vec<HelpEntry> {
    vec![
        HelpEntry::new(
            "shell.toolbar.undo",
            "Undo",
            "Revert the last document change. Keyboard shortcut: Ctrl+Z.",
        )
        .with_body("Undo reverts the most recent document mutation. Prism keeps a snapshot stack of up to 100 entries. Each undo restores both the document tree and the selection state at the time of the change.\n\nKeyboard shortcut: Ctrl+Z\n\nThe undo stack is per-session and resets when the document is reloaded.")
        .with_doc("shell/toolbar"),
        HelpEntry::new(
            "shell.toolbar.redo",
            "Redo",
            "Re-apply the last undone change. Keyboard shortcut: Ctrl+Shift+Z.",
        )
        .with_body("Redo re-applies a previously undone change. The redo stack is cleared whenever a new mutation occurs after an undo.\n\nKeyboard shortcut: Ctrl+Shift+Z")
        .with_doc("shell/toolbar"),
        HelpEntry::new(
            "shell.toolbar.copy",
            "Copy",
            "Copy the selected node to the internal clipboard. Keyboard shortcut: Ctrl+C.",
        )
        .with_body("Copy places a deep clone of the selected node (including all children) into the internal clipboard buffer. The original node is left in place.\n\nKeyboard shortcut: Ctrl+C\n\nThe clipboard is internal to the Prism session — it does not interact with the system clipboard.")
        .with_doc("shell/toolbar"),
        HelpEntry::new(
            "shell.toolbar.cut",
            "Cut",
            "Cut the selected node (copy then delete). Keyboard shortcut: Ctrl+X.",
        )
        .with_doc("shell/toolbar"),
        HelpEntry::new(
            "shell.toolbar.paste",
            "Paste",
            "Insert a copy of the clipboard node as a child of the selection. Keyboard shortcut: Ctrl+V.",
        )
        .with_body("Paste inserts a deep clone of the clipboard node as a child of the currently selected node. All node IDs are regenerated to avoid duplicates.\n\nKeyboard shortcut: Ctrl+V\n\nIf no node is selected, paste is unavailable.")
        .with_doc("shell/toolbar"),
        HelpEntry::new(
            "shell.toolbar.duplicate",
            "Duplicate",
            "Clone the selected node and insert it as the next sibling. Keyboard shortcut: Ctrl+D.",
        )
        .with_doc("shell/toolbar"),
        HelpEntry::new(
            "shell.toolbar.move-up",
            "Move Up",
            "Move the selected node one position earlier among its siblings.",
        ),
        HelpEntry::new(
            "shell.toolbar.move-down",
            "Move Down",
            "Move the selected node one position later among its siblings.",
        ),
        HelpEntry::new(
            "shell.toolbar.delete",
            "Delete",
            "Remove the selected node from the document. Keyboard shortcut: Delete.",
        )
        .with_body("Delete removes the selected node and all its children from the document tree. The operation is undoable via Ctrl+Z.\n\nKeyboard shortcut: Delete\n\nIf the deleted node was the last child of its parent, the parent becomes the new selection.")
        .with_doc("shell/toolbar"),
    ]
}

pub fn shell_entries() -> Vec<HelpEntry> {
    vec![
        HelpEntry::new(
            "shell.panels.editor",
            "Editor",
            "Visual page editor. Canvas, component palette, layer tree, and properties are all visible simultaneously. Select elements on the canvas or in the layers panel to edit their properties.",
        )
        .with_body("The Editor panel is Prism Studio's primary workspace. It combines four surfaces:\n\n• Component Palette — drag components from the sidebar to add them to the document tree.\n• Canvas — WYSIWYG preview of the document. Click to select, Enter to inline-edit.\n• Layers — hierarchical tree view of all nodes. Reorder with the arrow buttons.\n• Properties — schema-driven field editor for the selected component's properties.\n\nThe toolbar at the top provides undo/redo, clipboard operations, and node reordering.")
        .with_doc("shell/editor"),
        HelpEntry::new(
            "shell.tabs",
            "Tabs",
            "Open documents. Click a tab to switch, click the X to close.",
        )
        .with_body("Tabs provide a multi-document interface. Each open document gets its own tab in the tab bar above the canvas.\n\n• Click a tab to switch to that document.\n• Click the X to close a tab.\n• The active tab is highlighted with the accent color.\n\nClosing the last tab does not close the editor — a blank canvas is shown instead.")
        .with_doc("shell/tabs"),
        HelpEntry::new(
            "shell.command-palette",
            "Command Palette",
            "Search and run any command. Open with Ctrl+Shift+P.",
        )
        .with_body("The Command Palette provides quick access to all registered commands. Type to filter by label or category, then press Enter or click to execute.\n\nKeyboard shortcut: Ctrl+Shift+P\n\nCommands include panel navigation, clipboard operations, undo/redo, and search. Each command shows its keyboard shortcut on the right.")
        .with_doc("shell/command-palette"),
        HelpEntry::new(
            "shell.search",
            "Search",
            "Full-text search across all node properties in the current document. Open with Ctrl+F.",
        )
        .with_body("Search uses TF-IDF full-text indexing to find nodes by their property values. Results are ranked by relevance and show the matching field and a text snippet.\n\nKeyboard shortcut: Ctrl+F\n\nClick a search result to select that node in the canvas and inspector.")
        .with_doc("shell/search"),
    ]
}

pub fn chrome_entries() -> Vec<HelpEntry> {
    vec![
        HelpEntry::new(
            "shell.menu-bar",
            "Menu Bar",
            "Application menu with File, Edit, View, Window, and Help menus.",
        )
        .with_body("The menu bar provides access to all application commands organized by category. Click a menu name to open it, hover to switch between menus. Each item shows its keyboard shortcut if available.\n\nMenus: File (save), Edit (undo, redo, clipboard, selection), View (command palette, search, sidebar toggles, grid), Window (panel navigation), Help.")
        .with_doc("shell/menu-bar"),
        HelpEntry::new(
            "shell.explorer",
            "File Explorer",
            "Browse apps, pages, and top-level nodes in a tree view.",
        )
        .with_body("The explorer panel shows all apps in the workspace as a tree. Expand an app to see its pages; expand a page to see its top-level nodes.\n\n• Click an app to expand/collapse it.\n• Click a page to navigate to it.\n• Toggle between list and grid views with the toolbar buttons.\n\nThe active app is auto-expanded when you open the explorer.")
        .with_doc("shell/explorer"),
        HelpEntry::new(
            "shell.sidebar-toggle",
            "Sidebar Toggles",
            "Show or hide the left and right sidebars.",
        )
        .with_body("Sidebar toggle buttons in the status bar let you quickly show or hide the left sidebar (component palette, layers, explorer) and right sidebar (properties).\n\nKeyboard shortcuts:\n• Ctrl+B — Toggle left sidebar\n• Ctrl+Shift+B — Toggle right sidebar\n\nHide both sidebars for a distraction-free canvas view.")
        .with_doc("shell/sidebars"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::starter::register_builtins;

    fn setup() -> (HelpRegistry, ComponentRegistry) {
        let mut components = ComponentRegistry::new();
        register_builtins(&mut components).unwrap();
        let mut help = HelpRegistry::new();
        register_help_entries(&mut help, &components);
        (help, components)
    }

    #[test]
    fn registers_all_entries() {
        let (reg, _) = setup();
        assert!(reg.len() > 30);
    }

    #[test]
    fn every_entry_has_unique_id() {
        let (reg, _) = setup();
        let all = reg.get_all();
        let count = all.len();
        let unique: std::collections::HashSet<&str> = all.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(unique.len(), count);
    }

    #[test]
    fn every_entry_has_nonempty_title_and_summary() {
        let (reg, _) = setup();
        for entry in reg.get_all() {
            assert!(!entry.title.trim().is_empty(), "empty title: {}", entry.id);
            assert!(entry.summary.len() > 20, "summary too short: {}", entry.id);
        }
    }

    #[test]
    fn all_fifteen_components_registered() {
        let (reg, _) = setup();
        let components = [
            "text",
            "image",
            "container",
            "form",
            "input",
            "button",
            "card",
            "code",
            "divider",
            "spacer",
            "columns",
            "list",
            "table",
            "tabs",
            "accordion",
        ];
        for ct in components {
            let id = format!("builder.components.{ct}");
            assert!(reg.get(&id).is_some(), "missing component entry: {id}");
        }
    }

    #[test]
    fn components_come_from_registry_not_hardcoded() {
        let components = ComponentRegistry::new();
        let mut help = HelpRegistry::new();
        register_help_entries(&mut help, &components);
        assert!(
            help.get("builder.components.text").is_none(),
            "component help should come from ComponentRegistry, not hardcoded"
        );
    }

    #[test]
    fn search_finds_component_by_name() {
        let (reg, _) = setup();
        let results = reg.search("text");
        let ids: Vec<&str> = results.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"builder.components.text"));
    }

    #[test]
    fn all_panels_registered() {
        let (reg, _) = setup();
        for panel in [
            "identity",
            "builder",
            "inspector",
            "properties",
            "editor",
            "signals",
        ] {
            let id = format!("shell.panels.{panel}");
            assert!(reg.get(&id).is_some(), "missing panel entry: {id}");
        }
    }

    #[test]
    fn toolbar_entries_registered() {
        let (reg, _) = setup();
        assert!(reg.get("shell.toolbar.undo").is_some());
        assert!(reg.get("shell.toolbar.delete").is_some());
    }

    #[test]
    fn chrome_entries_registered() {
        let (reg, _) = setup();
        assert!(reg.get("shell.menu-bar").is_some());
        assert!(reg.get("shell.explorer").is_some());
        assert!(reg.get("shell.sidebar-toggle").is_some());
    }

    #[test]
    fn external_provider_can_add_entries() {
        let components = ComponentRegistry::new();
        let mut help = HelpRegistry::new();
        register_help_entries(&mut help, &components);

        let custom = HelpEntry::new(
            "custom.widget.clock",
            "Clock Widget",
            "Displays the current time in a configurable format.",
        );
        help.register_provider(&custom);
        assert!(help.get("custom.widget.clock").is_some());
    }
}
