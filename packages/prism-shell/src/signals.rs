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
            DispatchResult::EmitSignal {
                target_node,
                signal,
            } => {
                // Cascading: re-fire on the target node. The caller
                // should detect this and recursively dispatch.
                let _ = (target_node, signal);
                None
            }
            DispatchResult::NavigateTo { .. }
            | DispatchResult::PlayAnimation { .. }
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
}
