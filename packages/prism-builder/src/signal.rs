//! Signals and connections — user-authorable event wiring between nodes.
//!
//! Components declare the signals they can emit via
//! [`Component::signals()`]. Document authors wire connections between
//! nodes: source signal -> target action. The render walker emits the
//! appropriate event handlers for each backend.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::document::NodeId;
use crate::registry::FieldSpec;

pub type ConnectionId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub payload: Vec<FieldSpec>,
}

impl SignalDef {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            payload: Vec::new(),
        }
    }

    pub fn with_payload(mut self, fields: Vec<FieldSpec>) -> Self {
        self.payload = fields;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: ConnectionId,
    pub source_node: NodeId,
    pub signal: String,
    pub target_node: NodeId,
    pub action: ActionKind,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ActionKind {
    SetProperty { key: String, value: Value },
    ToggleVisibility,
    NavigateTo { target: String },
    PlayAnimation { animation: String },
    EmitSignal { signal: String },
    Custom { handler: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn signal_def_builder() {
        let sig = SignalDef::new("clicked", "Fires when the button is pressed")
            .with_payload(vec![FieldSpec::text("target", "Event target")]);
        assert_eq!(sig.name, "clicked");
        assert_eq!(sig.payload.len(), 1);
    }

    #[test]
    fn connection_round_trips() {
        let conn = Connection {
            id: "c1".into(),
            source_node: "btn-1".into(),
            signal: "clicked".into(),
            target_node: "modal-1".into(),
            action: ActionKind::ToggleVisibility,
            params: json!({}),
        };
        let json = serde_json::to_string(&conn).unwrap();
        let back: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "c1");
        assert_eq!(back.signal, "clicked");
        assert!(matches!(back.action, ActionKind::ToggleVisibility));
    }

    #[test]
    fn set_property_action_serializes() {
        let action = ActionKind::SetProperty {
            key: "visible".into(),
            value: json!(true),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("set-property"));
        let back: ActionKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ActionKind::SetProperty { .. }));
    }

    #[test]
    fn navigate_action_serializes() {
        let action = ActionKind::NavigateTo {
            target: "/dashboard".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: ActionKind = serde_json::from_str(&json).unwrap();
        match back {
            ActionKind::NavigateTo { target } => assert_eq!(target, "/dashboard"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn custom_action_serializes() {
        let action = ActionKind::Custom {
            handler: "onSubmit".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: ActionKind = serde_json::from_str(&json).unwrap();
        match back {
            ActionKind::Custom { handler } => assert_eq!(handler, "onSubmit"),
            _ => panic!("wrong variant"),
        }
    }
}
