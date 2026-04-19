//! Properties panel — the schema-driven field-row editor for the
//! currently selected node. Walks the component's [`FieldSpec`]
//! list and emits one field row per entry, reading the current
//! value straight out of the node's `props` via [`FieldValue`].
//!
//! The Slint side paints each row as a label + value + hint in a
//! vertical list; editing is wired up when the store grows a
//! mutate-prop action in Phase 4. Read-only for now is fine — the
//! important thing for Phase 3 is that the schema walks end-to-end.

use prism_builder::{
    BuilderDocument, ComponentRegistry, FieldKind, FieldSpec, FieldValue, Node, NodeId,
};
use prism_core::help::HelpEntry;
use serde_json::Value;

use super::Panel;

pub struct PropertiesPanel;

/// One row rendered by the Slint `FieldRowView` component. Mirrors
/// the `FieldRow` struct declared in `ui/app.slint`.
#[derive(Debug, Clone)]
pub struct FieldRowData {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub value: String,
    pub required: bool,
}

impl PropertiesPanel {
    pub const ID: i32 = 3;
    pub fn new() -> Self {
        Self
    }

    /// Find the component id of the currently selected node. Empty
    /// string when no node is selected or the id doesn't resolve.
    pub fn selected_component(doc: &BuilderDocument, selected: &Option<NodeId>) -> String {
        selected
            .as_ref()
            .and_then(|id| find_node(doc.root.as_ref(), id))
            .map(|n| n.component.clone())
            .unwrap_or_default()
    }

    /// Produce one [`FieldRowData`] per entry in the selected
    /// component's schema. Returns an empty list if nothing is
    /// selected or the component is missing from the registry.
    pub fn rows(
        doc: &BuilderDocument,
        registry: &ComponentRegistry,
        selected: &Option<NodeId>,
    ) -> Vec<FieldRowData> {
        let Some(selected_id) = selected else {
            return vec![];
        };
        let Some(node) = find_node(doc.root.as_ref(), selected_id) else {
            return vec![];
        };
        let Some(component) = registry.get(&node.component) else {
            return vec![];
        };
        component
            .schema()
            .into_iter()
            .map(|spec| row_from_spec(&spec, &node.props))
            .collect()
    }
}

fn find_node<'a>(root: Option<&'a Node>, target: &str) -> Option<&'a Node> {
    let node = root?;
    if node.id == target {
        return Some(node);
    }
    for child in &node.children {
        if let Some(hit) = find_node(Some(child), target) {
            return Some(hit);
        }
    }
    None
}

fn row_from_spec(spec: &FieldSpec, props: &Value) -> FieldRowData {
    let (kind_label, value) = match &spec.kind {
        FieldKind::Text => (
            "text".to_string(),
            FieldValue::read_string(props, spec).to_string(),
        ),
        FieldKind::TextArea => (
            "textarea".to_string(),
            FieldValue::read_string(props, spec).to_string(),
        ),
        FieldKind::Number(_) => (
            "number".to_string(),
            format_number(FieldValue::read_number(props, spec)),
        ),
        FieldKind::Integer(_) => (
            "integer".to_string(),
            FieldValue::read_integer(props, spec).to_string(),
        ),
        FieldKind::Boolean => (
            "boolean".to_string(),
            FieldValue::read_boolean(props, spec).to_string(),
        ),
        FieldKind::Select(_) => (
            "select".to_string(),
            FieldValue::read_string(props, spec).to_string(),
        ),
        FieldKind::Color => (
            "color".to_string(),
            FieldValue::read_string(props, spec).to_string(),
        ),
    };
    FieldRowData {
        key: spec.key.clone(),
        label: spec.label.clone(),
        kind: kind_label,
        value,
        required: spec.required,
    }
}

fn format_number(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

impl Default for PropertiesPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel for PropertiesPanel {
    fn id(&self) -> i32 {
        Self::ID
    }
    fn label(&self) -> &'static str {
        "Properties"
    }
    fn title(&self) -> &'static str {
        "Properties"
    }
    fn hint(&self) -> &'static str {
        "Schema-driven editor for the selected node."
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "shell.panels.properties",
            "Properties",
            "Property editor for the selected component. Fields are type-aware: text, numbers, booleans, selects, and colors.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::{starter::register_builtins, BuilderDocument, ComponentRegistry, Node};
    use serde_json::json;

    fn setup() -> (BuilderDocument, ComponentRegistry) {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                props: json!({ "spacing": 16 }),
                children: vec![Node {
                    id: "h".into(),
                    component: "heading".into(),
                    props: json!({ "text": "Hi", "level": 2 }),
                    children: vec![],
                }],
            }),
            zones: Default::default(),
        };
        (doc, reg)
    }

    #[test]
    fn empty_selection_yields_no_rows() {
        let (doc, reg) = setup();
        assert!(PropertiesPanel::rows(&doc, &reg, &None).is_empty());
    }

    #[test]
    fn heading_schema_produces_two_rows() {
        let (doc, reg) = setup();
        let rows = PropertiesPanel::rows(&doc, &reg, &Some("h".into()));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].key, "text");
        assert_eq!(rows[0].value, "Hi");
        assert!(rows[0].required);
        assert_eq!(rows[1].key, "level");
        assert_eq!(rows[1].value, "2");
    }

    #[test]
    fn container_spacing_row_has_number_kind() {
        let (doc, reg) = setup();
        let rows = PropertiesPanel::rows(&doc, &reg, &Some("root".into()));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, "spacing");
        assert_eq!(rows[0].kind, "integer");
        assert_eq!(rows[0].value, "16");
    }

    #[test]
    fn selected_component_resolves_through_registry() {
        let (doc, reg) = setup();
        let _ = reg;
        assert_eq!(
            PropertiesPanel::selected_component(&doc, &Some("h".into())),
            "heading"
        );
        assert_eq!(
            PropertiesPanel::selected_component(&doc, &Some("root".into())),
            "container"
        );
        assert_eq!(PropertiesPanel::selected_component(&doc, &None), "");
    }
}
