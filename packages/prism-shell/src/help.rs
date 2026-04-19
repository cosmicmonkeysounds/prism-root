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
}

fn panels() -> Vec<Box<dyn Panel>> {
    vec![
        Box::new(IdentityPanel::new()),
        Box::new(BuilderPanel::new()),
        Box::new(InspectorPanel::new()),
        Box::new(PropertiesPanel::new()),
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
        ),
        HelpEntry::new(
            "shell.toolbar.redo",
            "Redo",
            "Re-apply the last undone change. Keyboard shortcut: Ctrl+Shift+Z.",
        ),
        HelpEntry::new(
            "shell.toolbar.copy",
            "Copy",
            "Copy the selected node to the internal clipboard. Keyboard shortcut: Ctrl+C.",
        ),
        HelpEntry::new(
            "shell.toolbar.cut",
            "Cut",
            "Cut the selected node (copy then delete). Keyboard shortcut: Ctrl+X.",
        ),
        HelpEntry::new(
            "shell.toolbar.paste",
            "Paste",
            "Insert a copy of the clipboard node as a child of the selection. Keyboard shortcut: Ctrl+V.",
        ),
        HelpEntry::new(
            "shell.toolbar.duplicate",
            "Duplicate",
            "Clone the selected node and insert it as the next sibling. Keyboard shortcut: Ctrl+D.",
        ),
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
        ),
    ]
}

pub fn shell_entries() -> Vec<HelpEntry> {
    vec![
        HelpEntry::new(
            "shell.tabs",
            "Tabs",
            "Open documents. Click a tab to switch, click the X to close.",
        ),
        HelpEntry::new(
            "shell.command-palette",
            "Command Palette",
            "Search and run any command. Open with Ctrl+Shift+P.",
        ),
        HelpEntry::new(
            "shell.search",
            "Search",
            "Full-text search across all node properties in the current document. Open with Ctrl+F.",
        ),
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
    fn all_seventeen_components_registered() {
        let (reg, _) = setup();
        let components = [
            "heading",
            "text",
            "link",
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
            help.get("builder.components.heading").is_none(),
            "component help should come from ComponentRegistry, not hardcoded"
        );
    }

    #[test]
    fn search_finds_component_by_name() {
        let (reg, _) = setup();
        let results = reg.search("heading");
        let ids: Vec<&str> = results.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"builder.components.heading"));
    }

    #[test]
    fn all_four_panels_registered() {
        let (reg, _) = setup();
        for panel in ["identity", "builder", "inspector", "properties"] {
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
