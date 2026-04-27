//! Signal dispatch — bridges builder signals to the shell runtime.
//!
//! When interaction events fire (click, hover, drag, etc.), the shell
//! calls [`SignalRuntime::fire`] with the source node ID, signal name,
//! and payload. The runtime evaluates the document's [`Connection`]
//! list via [`dispatch_signal`] and executes each resulting action
//! against the live `AppState`.

use prism_builder::document::NodeId;
use prism_builder::{dispatch_signal, Connection, DispatchResult, SignalEvent};
use serde_json::Value;

/// Collects dispatched actions from a signal fire for the shell to execute.
pub struct SignalRuntime;

impl SignalRuntime {
    /// Fire a signal and return the actions the shell should execute.
    /// The shell is responsible for interpreting each `DispatchResult`
    /// (mutating node props, toggling visibility, navigating, invoking
    /// Luau, etc.).
    pub fn fire(
        source_node: &str,
        signal: &str,
        payload: serde_json::Map<String, Value>,
        connections: &[Connection],
    ) -> Vec<DispatchResult> {
        let event = SignalEvent {
            source_node: source_node.into(),
            signal: signal.into(),
            payload,
        };
        dispatch_signal(&event, connections)
    }

    /// Fire a signal with an empty payload (common for focus/blur/delete/mount).
    pub fn fire_simple(
        source_node: &str,
        signal: &str,
        connections: &[Connection],
    ) -> Vec<DispatchResult> {
        Self::fire(source_node, signal, serde_json::Map::new(), connections)
    }

    /// Fire a click/hover signal with mouse coordinates.
    pub fn fire_pointer(
        source_node: &str,
        signal: &str,
        x: f64,
        y: f64,
        connections: &[Connection],
    ) -> Vec<DispatchResult> {
        let mut payload = serde_json::Map::new();
        payload.insert("x".into(), Value::from(x));
        payload.insert("y".into(), Value::from(y));
        Self::fire(source_node, signal, payload, connections)
    }

    /// Fire a property change signal.
    pub fn fire_changed(
        source_node: &str,
        key: &str,
        value: &str,
        old_value: &str,
        connections: &[Connection],
    ) -> Vec<DispatchResult> {
        let mut payload = serde_json::Map::new();
        payload.insert("key".into(), Value::from(key));
        payload.insert("value".into(), Value::from(value));
        payload.insert("old_value".into(), Value::from(old_value));
        Self::fire(source_node, "changed", payload, connections)
    }

    /// Apply a single dispatch result to a document, returning the node
    /// ID that was affected (if any) so the shell knows what to re-sync.
    pub fn apply_result(
        result: &DispatchResult,
        doc: &mut prism_builder::BuilderDocument,
    ) -> Option<NodeId> {
        match result {
            DispatchResult::SetProperty {
                target_node,
                key,
                value,
            } => {
                if let Some(node) = find_node_mut(&mut doc.root, target_node) {
                    if let Some(obj) = node.props.as_object_mut() {
                        obj.insert(key.clone(), value.clone());
                    }
                    Some(target_node.clone())
                } else {
                    None
                }
            }
            DispatchResult::ToggleVisibility { target_node } => {
                if let Some(node) = find_node_mut(&mut doc.root, target_node) {
                    let visible = node
                        .props
                        .get("visible")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    if let Some(obj) = node.props.as_object_mut() {
                        obj.insert("visible".into(), Value::from(!visible));
                    }
                    Some(target_node.clone())
                } else {
                    None
                }
            }
            DispatchResult::PlayAnimation {
                target_node,
                animation,
            } => {
                if let Some(node) = find_node_mut(&mut doc.root, target_node) {
                    if let Some(obj) = node.props.as_object_mut() {
                        obj.insert("animating".into(), Value::from(true));
                        obj.insert("animation".into(), Value::from(animation.as_str()));
                    }
                    Some(target_node.clone())
                } else {
                    None
                }
            }
            DispatchResult::EmitSignal { .. }
            | DispatchResult::NavigateTo { .. }
            | DispatchResult::Custom { .. } => None,
        }
    }
}

fn find_node_mut<'a>(
    root: &'a mut Option<prism_builder::Node>,
    id: &str,
) -> Option<&'a mut prism_builder::Node> {
    let root = root.as_mut()?;
    find_in_tree_mut(root, id)
}

fn find_in_tree_mut<'a>(
    node: &'a mut prism_builder::Node,
    id: &str,
) -> Option<&'a mut prism_builder::Node> {
    if node.id == id {
        return Some(node);
    }
    for child in &mut node.children {
        if let Some(found) = find_in_tree_mut(child, id) {
            return Some(found);
        }
    }
    None
}

/// Convert a document's `Connection` list into `ScriptGraph` `EventListener`
/// nodes. Each Custom connection becomes an EventListener whose signal
/// property matches the connection's signal name, and whose body is the
/// handler name. Non-Custom connections are represented as EventListener
/// nodes whose body calls the built-in action function (e.g. `toggle_visibility`).
///
/// This is the Connection → Graph direction of the bidirectional sync.
pub fn connections_to_event_listeners(
    connections: &[prism_builder::Connection],
) -> Vec<prism_core::language::visual::ScriptNode> {
    use prism_core::language::visual::{DataType, PortDef, PortKind, ScriptNode, ScriptNodeKind};

    connections
        .iter()
        .map(|conn| {
            let signal = &conn.signal;
            let handler_name = format!("on_{}", signal.replace('-', "_"));
            let body = crate::panels::signals::SignalsPanel::connection_as_luau(conn);
            let label = format!("on {signal}");

            ScriptNode::new(
                format!("conn-{}", conn.id),
                ScriptNodeKind::EventListener,
                label,
            )
            .with_port(PortDef {
                id: "exec_in".into(),
                label: "".into(),
                kind: PortKind::Execution,
                direction: prism_core::language::visual::PortDirection::Input,
                data_type: DataType::Any,
            })
            .with_port(PortDef {
                id: "exec_out".into(),
                label: "".into(),
                kind: PortKind::Execution,
                direction: prism_core::language::visual::PortDirection::Output,
                data_type: DataType::Any,
            })
            .with_property("signal", Value::String(signal.clone()))
            .with_property("handler", Value::String(handler_name))
            .with_property("body", Value::String(body))
            .with_property("connection_id", Value::String(conn.id.clone()))
            .with_property("source_node", Value::String(conn.source_node.clone()))
            .with_property("target_node", Value::String(conn.target_node.clone()))
        })
        .collect()
}

/// Convert `EventListener` nodes from a `ScriptGraph` back into
/// `Connection` data. Each EventListener with a `connection_id` property
/// round-trips back to its original connection. EventListeners without
/// `connection_id` (newly created in the visual editor) become Custom
/// connections with handler names derived from the signal.
///
/// This is the Graph → Connection direction of the bidirectional sync.
pub fn event_listeners_to_connections(
    nodes: &[prism_core::language::visual::ScriptNode],
    source_node_default: &str,
) -> Vec<prism_builder::Connection> {
    use prism_core::language::visual::ScriptNodeKind;

    nodes
        .iter()
        .filter(|n| n.kind == ScriptNodeKind::EventListener)
        .map(|node| {
            let signal = node
                .properties
                .get("signal")
                .and_then(|v| v.as_str())
                .unwrap_or("clicked")
                .to_string();

            let conn_id = node
                .properties
                .get("connection_id")
                .and_then(|v| v.as_str())
                .unwrap_or(&node.id)
                .to_string();

            let source = node
                .properties
                .get("source_node")
                .and_then(|v| v.as_str())
                .unwrap_or(source_node_default)
                .to_string();

            let target = node
                .properties
                .get("target_node")
                .and_then(|v| v.as_str())
                .unwrap_or(source_node_default)
                .to_string();

            let handler_name = node
                .properties
                .get("handler")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("on_{}", signal.replace('-', "_")));

            prism_builder::Connection {
                id: conn_id,
                source_node: source,
                signal,
                target_node: target,
                action: prism_builder::ActionKind::Custom {
                    handler: handler_name,
                },
                params: serde_json::json!({}),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::{ActionKind, BuilderDocument, Connection, Node};
    use serde_json::json;

    fn test_doc() -> BuilderDocument {
        BuilderDocument {
            root: Some(Node {
                id: "btn".into(),
                component: "button".into(),
                props: json!({ "text": "Click", "visible": true }),
                children: vec![Node {
                    id: "modal".into(),
                    component: "container".into(),
                    props: json!({ "visible": false }),
                    children: vec![],
                    ..Default::default()
                }],
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
                    target_node: "modal".into(),
                    action: ActionKind::SetProperty {
                        key: "title".into(),
                        value: json!("Opened!"),
                    },
                    params: json!({}),
                },
                Connection {
                    id: "c3".into(),
                    source_node: "btn".into(),
                    signal: "hovered".into(),
                    target_node: "btn".into(),
                    action: ActionKind::SetProperty {
                        key: "variant".into(),
                        value: json!("primary"),
                    },
                    params: json!({}),
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn fire_simple_dispatches_matching_connections() {
        let doc = test_doc();
        let results = SignalRuntime::fire_simple("btn", "clicked", &doc.connections);
        assert_eq!(results.len(), 2);
        assert!(
            matches!(&results[0], DispatchResult::ToggleVisibility { target_node } if target_node == "modal")
        );
        assert!(
            matches!(&results[1], DispatchResult::SetProperty { target_node, key, .. } if target_node == "modal" && key == "title")
        );
    }

    #[test]
    fn fire_pointer_dispatches_hover() {
        let doc = test_doc();
        let results = SignalRuntime::fire_pointer("btn", "hovered", 10.0, 20.0, &doc.connections);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], DispatchResult::SetProperty { key, .. } if key == "variant"));
    }

    #[test]
    fn fire_unmatched_signal_returns_empty() {
        let doc = test_doc();
        let results = SignalRuntime::fire_simple("btn", "deleted", &doc.connections);
        assert!(results.is_empty());
    }

    #[test]
    fn fire_unmatched_node_returns_empty() {
        let doc = test_doc();
        let results = SignalRuntime::fire_simple("nonexistent", "clicked", &doc.connections);
        assert!(results.is_empty());
    }

    #[test]
    fn apply_toggle_visibility() {
        let mut doc = test_doc();
        let results = SignalRuntime::fire_simple("btn", "clicked", &doc.connections);
        let affected = SignalRuntime::apply_result(&results[0], &mut doc);
        assert_eq!(affected, Some("modal".into()));

        let modal = find_node_mut(&mut doc.root, "modal").unwrap();
        assert_eq!(modal.props.get("visible"), Some(&json!(true)));
    }

    #[test]
    fn apply_set_property() {
        let mut doc = test_doc();
        let results = SignalRuntime::fire_simple("btn", "clicked", &doc.connections);
        let affected = SignalRuntime::apply_result(&results[1], &mut doc);
        assert_eq!(affected, Some("modal".into()));

        let modal = find_node_mut(&mut doc.root, "modal").unwrap();
        assert_eq!(modal.props.get("title"), Some(&json!("Opened!")));
    }

    #[test]
    fn fire_changed_carries_payload() {
        let connections = vec![Connection {
            id: "c".into(),
            source_node: "input".into(),
            signal: "changed".into(),
            target_node: "label".into(),
            action: ActionKind::Custom {
                handler: "onInputChanged".into(),
            },
            params: json!({}),
        }];
        let results = SignalRuntime::fire_changed("input", "name", "Alice", "Bob", &connections);
        assert_eq!(results.len(), 1);
        match &results[0] {
            DispatchResult::Custom { handler, payload } => {
                assert_eq!(handler, "onInputChanged");
                assert_eq!(payload.get("key").unwrap(), "name");
                assert_eq!(payload.get("value").unwrap(), "Alice");
                assert_eq!(payload.get("old_value").unwrap(), "Bob");
            }
            _ => panic!("expected Custom"),
        }
    }

    #[test]
    fn emit_signal_result_returns_none_from_apply() {
        let mut doc = test_doc();
        let result = DispatchResult::EmitSignal {
            target_node: "btn".into(),
            signal: "hovered".into(),
        };
        let affected = SignalRuntime::apply_result(&result, &mut doc);
        assert!(affected.is_none());
    }

    #[test]
    fn navigate_to_result_returns_none_from_apply() {
        let mut doc = test_doc();
        let result = DispatchResult::NavigateTo {
            target: "/home".into(),
        };
        let affected = SignalRuntime::apply_result(&result, &mut doc);
        assert!(affected.is_none());
    }

    #[test]
    fn cascading_emit_signal_produces_dispatch() {
        let connections = vec![
            Connection {
                id: "c1".into(),
                source_node: "btn".into(),
                signal: "clicked".into(),
                target_node: "modal".into(),
                action: ActionKind::EmitSignal {
                    signal: "show".into(),
                },
                params: json!({}),
            },
            Connection {
                id: "c2".into(),
                source_node: "modal".into(),
                signal: "show".into(),
                target_node: "modal".into(),
                action: ActionKind::ToggleVisibility,
                params: json!({}),
            },
        ];
        let first_results = SignalRuntime::fire_simple("btn", "clicked", &connections);
        assert_eq!(first_results.len(), 1);
        assert!(matches!(
            &first_results[0],
            DispatchResult::EmitSignal { target_node, signal }
            if target_node == "modal" && signal == "show"
        ));
        let second_results = SignalRuntime::fire_simple("modal", "show", &connections);
        assert_eq!(second_results.len(), 1);
        assert!(matches!(
            &second_results[0],
            DispatchResult::ToggleVisibility { target_node }
            if target_node == "modal"
        ));
    }

    #[test]
    fn apply_play_animation() {
        let mut doc = test_doc();
        let result = DispatchResult::PlayAnimation {
            target_node: "modal".into(),
            animation: "fade-in".into(),
        };
        let affected = SignalRuntime::apply_result(&result, &mut doc);
        assert_eq!(affected, Some("modal".into()));
        let modal = find_node_mut(&mut doc.root, "modal").unwrap();
        assert_eq!(modal.props.get("animating"), Some(&json!(true)));
        assert_eq!(modal.props.get("animation"), Some(&json!("fade-in")));
    }

    #[test]
    fn apply_play_animation_missing_node() {
        let mut doc = test_doc();
        let result = DispatchResult::PlayAnimation {
            target_node: "nonexistent".into(),
            animation: "slide".into(),
        };
        let affected = SignalRuntime::apply_result(&result, &mut doc);
        assert!(affected.is_none());
    }

    #[test]
    fn connections_to_event_listeners_roundtrip() {
        let connections = vec![
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
                signal: "hovered".into(),
                target_node: "btn".into(),
                action: ActionKind::Custom {
                    handler: "onHover".into(),
                },
                params: json!({}),
            },
        ];
        let nodes = connections_to_event_listeners(&connections);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].id, "conn-c1");
        assert_eq!(
            nodes[0].properties.get("signal").unwrap(),
            &json!("clicked")
        );
        assert_eq!(nodes[1].id, "conn-c2");
        assert_eq!(
            nodes[1].properties.get("signal").unwrap(),
            &json!("hovered")
        );
    }

    #[test]
    fn event_listeners_to_connections_creates_custom() {
        use prism_core::language::visual::{ScriptNode, ScriptNodeKind};

        let nodes = vec![
            ScriptNode::new("sig-0", ScriptNodeKind::EventListener, "on clicked")
                .with_property("signal", json!("clicked"))
                .with_property("source_node", json!("btn"))
                .with_property("target_node", json!("modal"))
                .with_property("handler", json!("on_clicked")),
        ];

        let connections = event_listeners_to_connections(&nodes, "default");
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].signal, "clicked");
        assert_eq!(connections[0].source_node, "btn");
        assert_eq!(connections[0].target_node, "modal");
        assert!(matches!(
            &connections[0].action,
            ActionKind::Custom { handler } if handler == "on_clicked"
        ));
    }

    #[test]
    fn event_listeners_to_connections_uses_defaults() {
        use prism_core::language::visual::{ScriptNode, ScriptNodeKind};

        let nodes = vec![
            ScriptNode::new("sig-0", ScriptNodeKind::EventListener, "on clicked")
                .with_property("signal", json!("clicked")),
        ];

        let connections = event_listeners_to_connections(&nodes, "fallback-node");
        assert_eq!(connections[0].source_node, "fallback-node");
        assert_eq!(connections[0].target_node, "fallback-node");
        assert_eq!(connections[0].id, "sig-0");
    }

    #[test]
    fn create_connection_then_fire_applies_action() {
        use crate::panels::signals::SignalsPanel;

        let mut doc = test_doc();
        let conn = SignalsPanel::create_connection(
            "new-conn",
            "btn",
            "clicked",
            "modal",
            ActionKind::SetProperty {
                key: "color".into(),
                value: json!("red"),
            },
        );
        doc.connections.push(conn);

        let results = SignalRuntime::fire_simple("btn", "clicked", &doc.connections);
        assert!(results.len() >= 3);

        for r in &results {
            SignalRuntime::apply_result(r, &mut doc);
        }
        let modal = find_node_mut(&mut doc.root, "modal").unwrap();
        assert_eq!(modal.props.get("color"), Some(&json!("red")));
        assert_eq!(modal.props.get("title"), Some(&json!("Opened!")));
    }

    #[test]
    fn event_listeners_ignores_non_listeners() {
        use prism_core::language::visual::{ScriptNode, ScriptNodeKind};

        let nodes = vec![
            ScriptNode::new("fn-0", ScriptNodeKind::FunctionDef, "function foo"),
            ScriptNode::new("sig-0", ScriptNodeKind::EventListener, "on clicked")
                .with_property("signal", json!("clicked")),
        ];

        let connections = event_listeners_to_connections(&nodes, "n0");
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].signal, "clicked");
    }
}
