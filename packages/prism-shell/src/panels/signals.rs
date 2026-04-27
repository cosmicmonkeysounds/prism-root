//! Signals panel — connection authoring and signal event inspection.
//!
//! Provides a dedicated editing surface for wiring signal connections
//! between nodes: source signal → target action. Works alongside the
//! Luau code editor and the visual builder as "another lens" on the
//! same document-level `Connection` data.

use prism_builder::{ActionKind, BuilderDocument, ComponentRegistry, Connection, Node};
use prism_core::help::HelpEntry;
use serde_json::json;

use super::Panel;

pub struct SignalsPanel;

/// One row in the connection list shown by the Slint signals panel.
#[derive(Debug, Clone)]
pub struct ConnectionRow {
    pub id: String,
    pub source_node: String,
    pub source_label: String,
    pub signal: String,
    pub target_node: String,
    pub target_label: String,
    pub action_kind: String,
    pub action_summary: String,
}

/// Available signal for a node — shown in the "add connection" picker.
#[derive(Debug, Clone)]
pub struct AvailableSignal {
    pub signal_name: String,
    pub description: String,
    pub payload_summary: String,
}

/// Available target nodes for a connection.
#[derive(Debug, Clone)]
pub struct TargetNode {
    pub node_id: String,
    pub label: String,
    pub component: String,
}

impl Default for SignalsPanel {
    fn default() -> Self {
        Self
    }
}

impl SignalsPanel {
    pub const ID: i32 = 5;

    pub fn new() -> Self {
        Self
    }

    /// Build connection rows for all connections in the document.
    pub fn connection_rows(doc: &BuilderDocument) -> Vec<ConnectionRow> {
        doc.connections
            .iter()
            .map(|conn| {
                let source_label = node_label(doc, &conn.source_node);
                let target_label = node_label(doc, &conn.target_node);
                let (action_kind, action_summary) = describe_action(&conn.action);
                ConnectionRow {
                    id: conn.id.clone(),
                    source_node: conn.source_node.clone(),
                    source_label,
                    signal: conn.signal.clone(),
                    target_node: conn.target_node.clone(),
                    target_label,
                    action_kind: action_kind.into(),
                    action_summary,
                }
            })
            .collect()
    }

    /// Build connection rows filtered to the selected node (as source or target).
    pub fn connections_for_node(doc: &BuilderDocument, node_id: &str) -> Vec<ConnectionRow> {
        Self::connection_rows(doc)
            .into_iter()
            .filter(|r| r.source_node == node_id || r.target_node == node_id)
            .collect()
    }

    /// List available signals for a given node based on its component type.
    pub fn available_signals(
        node_id: &str,
        doc: &BuilderDocument,
        registry: &ComponentRegistry,
    ) -> Vec<AvailableSignal> {
        let Some(node) = find_node(doc, node_id) else {
            return vec![];
        };
        let signals = match registry.get(&node.component) {
            Some(comp) => comp.signals(),
            None => prism_builder::common_signals(),
        };
        signals
            .iter()
            .map(|sig| {
                let payload_summary = if sig.payload.is_empty() {
                    "(no payload)".into()
                } else {
                    sig.payload
                        .iter()
                        .map(|f| f.key.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                AvailableSignal {
                    signal_name: sig.name.clone(),
                    description: sig.description.clone(),
                    payload_summary,
                }
            })
            .collect()
    }

    /// List all nodes in the document as potential connection targets.
    pub fn available_targets(doc: &BuilderDocument) -> Vec<TargetNode> {
        let mut targets = Vec::new();
        if let Some(root) = &doc.root {
            collect_targets(root, &mut targets);
        }
        targets
    }

    /// Create a new connection and return it. The caller is responsible
    /// for appending it to `doc.connections`.
    pub fn create_connection(
        id: &str,
        source_node: &str,
        signal: &str,
        target_node: &str,
        action: ActionKind,
    ) -> Connection {
        Connection {
            id: id.into(),
            source_node: source_node.into(),
            signal: signal.into(),
            target_node: target_node.into(),
            action,
            params: json!({}),
        }
    }

    /// Remove a connection by id. Returns true if found and removed.
    pub fn remove_connection(doc: &mut BuilderDocument, connection_id: &str) -> bool {
        let before = doc.connections.len();
        doc.connections.retain(|c| c.id != connection_id);
        doc.connections.len() < before
    }

    /// Build a signal context list for the Luau provider from a node's
    /// component type. Bridges the builder's `SignalDef` to the syntax
    /// engine's `SignalContext`.
    pub fn signal_contexts_for_node(
        node_id: &str,
        doc: &BuilderDocument,
        registry: &ComponentRegistry,
    ) -> Vec<prism_core::language::syntax::SignalContext> {
        let Some(node) = find_node(doc, node_id) else {
            return vec![];
        };
        let signals = match registry.get(&node.component) {
            Some(comp) => comp.signals(),
            None => prism_builder::common_signals(),
        };
        prism_builder::signal_contexts(&signals)
    }
}

impl Panel for SignalsPanel {
    fn id(&self) -> i32 {
        Self::ID
    }
    fn label(&self) -> &'static str {
        "Signals"
    }
    fn title(&self) -> &'static str {
        "Signal Connections"
    }
    fn hint(&self) -> &'static str {
        "Wire events between components"
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry {
            id: "panel.signals".into(),
            title: "Signals Panel".into(),
            summary: "Author and inspect signal connections between components. \
                      Each connection wires a source signal to a target action."
                .into(),
            body: None,
            doc_path: None,
            doc_anchor: None,
        })
    }
}

fn node_label(doc: &BuilderDocument, id: &str) -> String {
    find_node(doc, id)
        .map(|n| {
            n.props
                .get("label")
                .or_else(|| n.props.get("text"))
                .and_then(|v| v.as_str())
                .map(|s| format!("{} ({})", s, n.component))
                .unwrap_or_else(|| format!("{} [{}]", n.id, n.component))
        })
        .unwrap_or_else(|| id.to_string())
}

fn describe_action(action: &ActionKind) -> (&'static str, String) {
    match action {
        ActionKind::SetProperty { key, value } => ("set-property", format!("{key} = {value}")),
        ActionKind::ToggleVisibility => ("toggle-visibility", "Toggle visible".into()),
        ActionKind::NavigateTo { target } => ("navigate", format!("→ {target}")),
        ActionKind::PlayAnimation { animation } => ("animation", format!("▶ {animation}")),
        ActionKind::EmitSignal { signal } => ("emit-signal", format!("⇒ {signal}")),
        ActionKind::Custom { handler } => ("custom", format!("fn {handler}()")),
    }
}

fn find_node<'a>(doc: &'a BuilderDocument, id: &str) -> Option<&'a Node> {
    doc.root.as_ref().and_then(|n| n.find(id))
}

fn collect_targets(node: &Node, targets: &mut Vec<TargetNode>) {
    targets.push(TargetNode {
        node_id: node.id.clone(),
        label: node
            .props
            .get("label")
            .or_else(|| node.props.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or(&node.id)
            .to_string(),
        component: node.component.clone(),
    });
    for child in &node.children {
        collect_targets(child, targets);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::{BuilderDocument, Node};
    use serde_json::json;

    fn test_doc() -> BuilderDocument {
        BuilderDocument {
            root: Some(Node {
                id: "btn".into(),
                component: "button".into(),
                props: json!({ "text": "Click Me" }),
                children: vec![
                    Node {
                        id: "modal".into(),
                        component: "container".into(),
                        props: json!({ "visible": false, "label": "My Modal" }),
                        children: vec![],
                        ..Default::default()
                    },
                    Node {
                        id: "label".into(),
                        component: "text".into(),
                        props: json!({ "text": "Hello" }),
                        children: vec![],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            connections: vec![
                Connection {
                    id: "c1".into(),
                    source_node: "btn".into(),
                    signal: "clicked".into(),
                    target_node: "modal".into(),
                    action: ActionKind::ToggleVisibility,
                    params: json!({}),
                },
                Connection {
                    id: "c2".into(),
                    source_node: "btn".into(),
                    signal: "clicked".into(),
                    target_node: "label".into(),
                    action: ActionKind::SetProperty {
                        key: "text".into(),
                        value: json!("Clicked!"),
                    },
                    params: json!({}),
                },
                Connection {
                    id: "c3".into(),
                    source_node: "modal".into(),
                    signal: "deleted".into(),
                    target_node: "btn".into(),
                    action: ActionKind::Custom {
                        handler: "onModalClose".into(),
                    },
                    params: json!({}),
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn connection_rows_covers_all_connections() {
        let doc = test_doc();
        let rows = SignalsPanel::connection_rows(&doc);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].signal, "clicked");
        assert_eq!(rows[0].action_kind, "toggle-visibility");
        assert_eq!(rows[1].action_kind, "set-property");
        assert!(rows[1].action_summary.contains("text"));
        assert_eq!(rows[2].action_kind, "custom");
    }

    #[test]
    fn connections_for_node_filters_by_source_or_target() {
        let doc = test_doc();
        let btn_rows = SignalsPanel::connections_for_node(&doc, "btn");
        assert_eq!(btn_rows.len(), 3);

        let modal_rows = SignalsPanel::connections_for_node(&doc, "modal");
        assert_eq!(modal_rows.len(), 2);

        let label_rows = SignalsPanel::connections_for_node(&doc, "label");
        assert_eq!(label_rows.len(), 1);
    }

    #[test]
    fn available_targets_lists_all_nodes() {
        let doc = test_doc();
        let targets = SignalsPanel::available_targets(&doc);
        assert_eq!(targets.len(), 3);
        let ids: Vec<&str> = targets.iter().map(|t| t.node_id.as_str()).collect();
        assert!(ids.contains(&"btn"));
        assert!(ids.contains(&"modal"));
        assert!(ids.contains(&"label"));
    }

    #[test]
    fn create_connection_builds_valid_struct() {
        let conn = SignalsPanel::create_connection(
            "c-new",
            "btn",
            "hovered",
            "modal",
            ActionKind::ToggleVisibility,
        );
        assert_eq!(conn.id, "c-new");
        assert_eq!(conn.source_node, "btn");
        assert_eq!(conn.signal, "hovered");
        assert_eq!(conn.target_node, "modal");
        assert!(matches!(conn.action, ActionKind::ToggleVisibility));
    }

    #[test]
    fn remove_connection_removes_by_id() {
        let mut doc = test_doc();
        assert_eq!(doc.connections.len(), 3);
        let removed = SignalsPanel::remove_connection(&mut doc, "c2");
        assert!(removed);
        assert_eq!(doc.connections.len(), 2);
        assert!(!doc.connections.iter().any(|c| c.id == "c2"));
    }

    #[test]
    fn remove_connection_returns_false_for_missing() {
        let mut doc = test_doc();
        let removed = SignalsPanel::remove_connection(&mut doc, "nonexistent");
        assert!(!removed);
        assert_eq!(doc.connections.len(), 3);
    }

    #[test]
    fn node_label_extracts_text_or_label_prop() {
        let doc = test_doc();
        let lbl = node_label(&doc, "btn");
        assert!(lbl.contains("Click Me"));
        let lbl = node_label(&doc, "modal");
        assert!(lbl.contains("My Modal"));
    }

    #[test]
    fn available_signals_returns_common_for_unknown_component() {
        let doc = test_doc();
        let registry = ComponentRegistry::new();
        let sigs = SignalsPanel::available_signals("btn", &doc, &registry);
        assert!(sigs.len() >= 12);
        assert!(sigs.iter().any(|s| s.signal_name == "clicked"));
    }

    #[test]
    fn available_signals_with_registered_component() {
        let doc = test_doc();
        let mut registry = ComponentRegistry::new();
        prism_builder::register_builtins(&mut registry).unwrap();
        let sigs = SignalsPanel::available_signals("btn", &doc, &registry);
        assert!(sigs.iter().any(|s| s.signal_name == "clicked"));
        assert!(sigs.iter().any(|s| s.signal_name == "hovered"));
    }

    #[test]
    fn panel_trait_impl() {
        let panel = SignalsPanel::new();
        assert_eq!(panel.id(), 5);
        assert_eq!(panel.label(), "Signals");
        assert!(panel.help_entry().is_some());
    }

    #[test]
    fn describe_action_formats_all_variants() {
        assert_eq!(
            describe_action(&ActionKind::ToggleVisibility).0,
            "toggle-visibility"
        );
        let (kind, summary) = describe_action(&ActionKind::NavigateTo {
            target: "/home".into(),
        });
        assert_eq!(kind, "navigate");
        assert!(summary.contains("/home"));
        let (kind, summary) = describe_action(&ActionKind::EmitSignal {
            signal: "done".into(),
        });
        assert_eq!(kind, "emit-signal");
        assert!(summary.contains("done"));
    }
}
