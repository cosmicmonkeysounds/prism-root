use serde::{Deserialize, Serialize};

use crate::node::DockNode;
use crate::panel::PanelKind;
use crate::state::DockState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPage {
    pub id: String,
    pub label: String,
    pub icon_hint: String,
    pub dock: DockState,
}

impl WorkflowPage {
    pub fn edit() -> Self {
        // 3-column: left tabs | Builder | Properties+CodeEditor
        // Left has Component Palette, Inspector, and Explorer as tabs.
        // Right has Properties and CodeEditor as tabs (bidirectional editing).
        let dock = DockState::new(DockNode::hsplit(
            0.18,
            DockNode::tabs(vec![
                PanelKind::ComponentPalette.id(),
                PanelKind::Inspector.id(),
                PanelKind::Explorer.id(),
            ]),
            DockNode::hsplit(
                0.72,
                DockNode::tab(PanelKind::Builder.id()),
                DockNode::tabs(vec![
                    PanelKind::Properties.id(),
                    PanelKind::Signals.id(),
                    PanelKind::CodeEditor.id(),
                ]),
            ),
        ));
        Self {
            id: "edit".into(),
            label: "Edit".into(),
            icon_hint: "edit".into(),
            dock,
        }
    }

    pub fn design() -> Self {
        // 3-column with bottom strip:
        // [ComponentPalette | Builder | Inspector]
        //         [Properties]
        let dock = DockState::new(DockNode::vsplit(
            0.7,
            DockNode::hsplit(
                0.15,
                DockNode::tab(PanelKind::ComponentPalette.id()),
                DockNode::hsplit(
                    0.75,
                    DockNode::tab(PanelKind::Builder.id()),
                    DockNode::tab(PanelKind::Inspector.id()),
                ),
            ),
            DockNode::tab(PanelKind::Properties.id()),
        ));
        Self {
            id: "design".into(),
            label: "Design".into(),
            icon_hint: "design".into(),
            dock,
        }
    }

    pub fn code() -> Self {
        // 2-column: Explorer | CodeEditor
        let dock = DockState::new(DockNode::hsplit(
            0.22,
            DockNode::tab(PanelKind::Explorer.id()),
            DockNode::vsplit(
                0.75,
                DockNode::tab(PanelKind::CodeEditor.id()),
                DockNode::tab(PanelKind::Console.id()),
            ),
        ));
        Self {
            id: "code".into(),
            label: "Code".into(),
            icon_hint: "code".into(),
            dock,
        }
    }

    pub fn fusion() -> Self {
        // Resolve Fusion style: center node graph with surrounding docks.
        // [Builder      | Inspector]
        // [NodeGraph    | Properties]
        // [     Timeline            ]
        let dock = DockState::new(DockNode::vsplit(
            0.65,
            DockNode::vsplit(
                0.5,
                DockNode::hsplit(
                    0.65,
                    DockNode::tab(PanelKind::Builder.id()),
                    DockNode::tab(PanelKind::Inspector.id()),
                ),
                DockNode::hsplit(
                    0.65,
                    DockNode::tab(PanelKind::NodeGraph.id()),
                    DockNode::tab(PanelKind::Properties.id()),
                ),
            ),
            DockNode::tab(PanelKind::Timeline.id()),
        ));
        Self {
            id: "fusion".into(),
            label: "Fusion".into(),
            icon_hint: "node-graph".into(),
            dock,
        }
    }

    pub fn builtins() -> Vec<Self> {
        vec![Self::edit(), Self::design(), Self::code(), Self::fusion()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_preset_has_expected_panels() {
        let page = WorkflowPage::edit();
        assert!(page.dock.contains_panel("component-palette"));
        assert!(page.dock.contains_panel("inspector"));
        assert!(page.dock.contains_panel("explorer"));
        assert!(page.dock.contains_panel("builder"));
        assert!(page.dock.contains_panel("properties"));
        assert!(page.dock.contains_panel("code-editor"));
        assert!(page.dock.contains_panel("signals"));
        assert_eq!(page.dock.tab_count(), 7);
    }

    #[test]
    fn design_preset_has_component_palette() {
        let page = WorkflowPage::design();
        assert!(page.dock.contains_panel("component-palette"));
        assert!(page.dock.contains_panel("builder"));
        assert!(page.dock.contains_panel("inspector"));
        assert!(page.dock.contains_panel("properties"));
    }

    #[test]
    fn code_preset_has_editor() {
        let page = WorkflowPage::code();
        assert!(page.dock.contains_panel("code-editor"));
        assert!(page.dock.contains_panel("explorer"));
        assert!(page.dock.contains_panel("console"));
    }

    #[test]
    fn fusion_preset_has_all_creative_panels() {
        let page = WorkflowPage::fusion();
        assert!(page.dock.contains_panel("builder"));
        assert!(page.dock.contains_panel("node-graph"));
        assert!(page.dock.contains_panel("timeline"));
        assert!(page.dock.contains_panel("inspector"));
        assert!(page.dock.contains_panel("properties"));
    }

    #[test]
    fn builtins_returns_four_pages() {
        let pages = WorkflowPage::builtins();
        assert_eq!(pages.len(), 4);
        let ids: Vec<&str> = pages.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, &["edit", "design", "code", "fusion"]);
    }

    #[test]
    fn all_presets_serde_roundtrip() {
        for page in WorkflowPage::builtins() {
            let json = serde_json::to_string(&page).unwrap();
            let page2: WorkflowPage = serde_json::from_str(&json).unwrap();
            assert_eq!(page.id, page2.id);
            assert_eq!(page.dock.tab_count(), page2.dock.tab_count());
        }
    }
}
