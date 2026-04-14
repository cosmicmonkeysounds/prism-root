//! Runtime evaluation of [`PrivilegeSet`]s against data. Wraps access
//! control logic: filters objects by row-level security, redacts
//! hidden fields, and enforces read/write permissions.
//!
//! Port of `identity/manifest/privilege-enforcer.ts`. The TS module
//! used a closure-over-state factory; Rust models it as a plain
//! struct owning a `PrivilegeSet` clone.

use serde_json::Value;

use crate::foundation::object_model::GraphObject;

use super::privilege_set::{
    can_read as can_read_collection, can_write as can_write_collection, get_collection_permission,
    get_field_permission, get_layout_permission, CollectionPermission, FieldPermission,
    LayoutPermission, PrivilegeSet,
};

/// Evaluation context passed to [`PrivilegeEnforcer::filter_objects`].
#[derive(Debug, Clone)]
pub struct PrivilegeContext {
    pub current_did: String,
    pub current_role: Option<String>,
}

/// Stateful enforcer wrapping a [`PrivilegeSet`].
#[derive(Debug, Clone)]
pub struct PrivilegeEnforcer {
    privilege_set: PrivilegeSet,
}

impl PrivilegeEnforcer {
    pub fn new(privilege_set: PrivilegeSet) -> Self {
        Self { privilege_set }
    }

    pub fn privilege_set(&self) -> &PrivilegeSet {
        &self.privilege_set
    }

    pub fn can_read(&self, collection_id: &str) -> bool {
        can_read_collection(&self.privilege_set, collection_id)
    }

    pub fn can_write(&self, collection_id: &str) -> bool {
        can_write_collection(&self.privilege_set, collection_id)
    }

    pub fn field_permission(&self, collection_id: &str, field_path: &str) -> FieldPermission {
        get_field_permission(&self.privilege_set, collection_id, field_path)
    }

    pub fn can_edit_field(&self, collection_id: &str, field_path: &str) -> bool {
        matches!(
            self.field_permission(collection_id, field_path),
            FieldPermission::Readwrite
        )
    }

    pub fn can_see_field(&self, collection_id: &str, field_path: &str) -> bool {
        !matches!(
            self.field_permission(collection_id, field_path),
            FieldPermission::Hidden
        )
    }

    pub fn can_see_layout(&self, layout_id: &str) -> bool {
        matches!(
            get_layout_permission(&self.privilege_set, layout_id),
            LayoutPermission::Visible
        )
    }

    pub fn collection_permission(&self, collection_id: &str) -> CollectionPermission {
        get_collection_permission(&self.privilege_set, collection_id)
    }

    /// Filter objects by row-level security expression. Returns an
    /// empty `Vec` when the collection is not readable, or clones
    /// through unchanged when no `recordFilter` is set.
    pub fn filter_objects(
        &self,
        collection_id: &str,
        objects: &[GraphObject],
        context: &PrivilegeContext,
    ) -> Vec<GraphObject> {
        if !self.can_read(collection_id) {
            return Vec::new();
        }
        let Some(expr) = self.privilege_set.record_filter.as_deref() else {
            return objects.to_vec();
        };
        objects
            .iter()
            .filter(|obj| evaluate_record_filter(expr, obj, context))
            .cloned()
            .collect()
    }

    /// Strip hidden fields from [`GraphObject::data`]. Returns a clone
    /// of the original when the privilege set has no field overrides
    /// — mirrors the TS `if (!privilegeSet.fields) return object`.
    pub fn redact_object(&self, collection_id: &str, object: &GraphObject) -> GraphObject {
        if self.privilege_set.fields.is_none() {
            return object.clone();
        }
        let mut redacted = object.clone();
        redacted.data.retain(|key, _| {
            !matches!(
                get_field_permission(&self.privilege_set, collection_id, key),
                FieldPermission::Hidden,
            )
        });
        redacted
    }

    /// List of visible field paths for a collection — all names in
    /// `all_fields` whose permission is not [`FieldPermission::Hidden`].
    pub fn visible_fields(&self, collection_id: &str, all_fields: &[String]) -> Vec<String> {
        all_fields
            .iter()
            .filter(|field| {
                !matches!(
                    get_field_permission(&self.privilege_set, collection_id, field),
                    FieldPermission::Hidden,
                )
            })
            .cloned()
            .collect()
    }
}

/// Factory mirror of the TS `createPrivilegeEnforcer`.
pub fn create_privilege_enforcer(privilege_set: PrivilegeSet) -> PrivilegeEnforcer {
    PrivilegeEnforcer::new(privilege_set)
}

/// Evaluate a simple row-level security expression. Supports
/// `record.field == value` / `record.field != value` with special
/// variables `current_did`, `current_role`, and double-quoted literals.
/// An unparseable expression is treated as "allow" to match the TS.
fn evaluate_record_filter(
    expression: &str,
    object: &GraphObject,
    context: &PrivilegeContext,
) -> bool {
    let Some((field, op, raw_value)) = parse_simple_comparison(expression) else {
        return true;
    };

    let actual = object
        .data
        .get(field)
        .cloned()
        .or_else(|| top_level_field_value(object, field));

    let expected = resolve_expected_value(raw_value.trim(), context);

    match op {
        "==" => compare_eq(actual.as_ref(), expected.as_deref()),
        "!=" => !compare_eq(actual.as_ref(), expected.as_deref()),
        _ => true,
    }
}

/// Parse `record.<field> (==|!=) <rest>`; returns `None` on anything
/// that doesn't match the TS `/^record\.(\w+)\s*(==|!=)\s*(.+)$/` regex.
fn parse_simple_comparison(expression: &str) -> Option<(&str, &str, &str)> {
    let rest = expression.strip_prefix("record.")?;
    let field_end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    if field_end == 0 {
        return None;
    }
    let (field, after_field) = rest.split_at(field_end);
    let after_field = after_field.trim_start();
    let (op, after_op) = if let Some(r) = after_field.strip_prefix("==") {
        ("==", r)
    } else if let Some(r) = after_field.strip_prefix("!=") {
        ("!=", r)
    } else {
        return None;
    };
    let value = after_op.trim_start();
    if value.is_empty() {
        return None;
    }
    Some((field, op, value))
}

fn resolve_expected_value(raw: &str, context: &PrivilegeContext) -> Option<String> {
    match raw {
        "current_did" => Some(context.current_did.clone()),
        "current_role" => context.current_role.clone(),
        _ => {
            if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
                Some(raw[1..raw.len() - 1].to_string())
            } else {
                Some(raw.to_string())
            }
        }
    }
}

fn compare_eq(actual: Option<&Value>, expected: Option<&str>) -> bool {
    match (actual, expected) {
        (Some(Value::String(s)), Some(e)) => s == e,
        (Some(Value::Null), _) => false,
        (None, _) => false,
        // Non-string JSON values — compare their display form against
        // the raw token. The TS used `===` which only returns true when
        // the field already happens to be a string; anything else here
        // collapses to string equality on the JSON serialisation.
        (Some(other), Some(e)) => match other {
            Value::Bool(b) => b.to_string() == e,
            Value::Number(n) => n.to_string() == e,
            _ => false,
        },
        (Some(_), None) => false,
    }
}

fn top_level_field_value(object: &GraphObject, field: &str) -> Option<Value> {
    match field {
        "id" => Some(Value::String(object.id.as_str().to_string())),
        "type" => Some(Value::String(object.type_name.clone())),
        "name" => Some(Value::String(object.name.clone())),
        "status" => object.status.clone().map(Value::String),
        "description" => Some(Value::String(object.description.clone())),
        "color" => object.color.clone().map(Value::String),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::ObjectId;
    use crate::identity::manifest::privilege_set::{
        create_privilege_set, CollectionPermission, FieldPermission, LayoutPermission,
        PrivilegeSetOptions,
    };
    use indexmap::IndexMap;
    use serde_json::json;

    fn make_object(id: &str, data_pairs: &[(&str, Value)]) -> GraphObject {
        let mut obj = GraphObject::new(ObjectId::from(id.to_string()), "test", "Test");
        for (k, v) in data_pairs {
            obj.data.insert((*k).to_string(), v.clone());
        }
        obj
    }

    fn ctx() -> PrivilegeContext {
        PrivilegeContext {
            current_did: "did:key:alice".into(),
            current_role: Some("admin".into()),
        }
    }

    fn collections(
        pairs: &[(&str, CollectionPermission)],
    ) -> IndexMap<String, CollectionPermission> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    fn fields(pairs: &[(&str, FieldPermission)]) -> IndexMap<String, FieldPermission> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    fn layouts(pairs: &[(&str, LayoutPermission)]) -> IndexMap<String, LayoutPermission> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    fn admin_set() -> PrivilegeSet {
        create_privilege_set(
            "admin",
            "Admin",
            PrivilegeSetOptions {
                collections: collections(&[("*", CollectionPermission::Full)]),
                ..Default::default()
            },
        )
    }

    fn client_set() -> PrivilegeSet {
        create_privilege_set(
            "client",
            "Client",
            PrivilegeSetOptions {
                collections: collections(&[
                    ("invoices", CollectionPermission::Read),
                    ("contacts", CollectionPermission::None),
                    ("*", CollectionPermission::None),
                ]),
                fields: Some(fields(&[
                    ("invoices.cost_breakdown", FieldPermission::Hidden),
                    ("invoices.notes", FieldPermission::Readonly),
                ])),
                layouts: Some(layouts(&[
                    ("admin-dashboard", LayoutPermission::Hidden),
                    ("*", LayoutPermission::Visible),
                ])),
                ..Default::default()
            },
        )
    }

    // ── canRead / canWrite ──────────────────────────────────────────────────

    #[test]
    fn admin_can_read_and_write_everything() {
        let e = create_privilege_enforcer(admin_set());
        assert!(e.can_read("invoices"));
        assert!(e.can_write("invoices"));
    }

    #[test]
    fn client_can_read_invoices_but_not_write() {
        let e = create_privilege_enforcer(client_set());
        assert!(e.can_read("invoices"));
        assert!(!e.can_write("invoices"));
    }

    #[test]
    fn client_cannot_read_contacts() {
        let e = create_privilege_enforcer(client_set());
        assert!(!e.can_read("contacts"));
    }

    // ── field permissions ───────────────────────────────────────────────────

    #[test]
    fn admin_readwrite_on_all_fields() {
        let e = create_privilege_enforcer(admin_set());
        assert!(e.can_edit_field("invoices", "amount"));
        assert!(e.can_see_field("invoices", "amount"));
    }

    #[test]
    fn client_cannot_see_hidden_field() {
        let e = create_privilege_enforcer(client_set());
        assert!(!e.can_see_field("invoices", "cost_breakdown"));
        assert!(!e.can_edit_field("invoices", "cost_breakdown"));
    }

    #[test]
    fn client_sees_but_cannot_edit_readonly_field() {
        let e = create_privilege_enforcer(client_set());
        assert!(e.can_see_field("invoices", "notes"));
        assert!(!e.can_edit_field("invoices", "notes"));
    }

    #[test]
    fn client_unspecified_invoice_fields_are_readonly() {
        let e = create_privilege_enforcer(client_set());
        assert_eq!(
            e.field_permission("invoices", "amount"),
            FieldPermission::Readonly
        );
    }

    // ── layout permissions ──────────────────────────────────────────────────

    #[test]
    fn client_cannot_see_admin_dashboard() {
        let e = create_privilege_enforcer(client_set());
        assert!(!e.can_see_layout("admin-dashboard"));
    }

    #[test]
    fn client_can_see_other_layouts_via_wildcard() {
        let e = create_privilege_enforcer(client_set());
        assert!(e.can_see_layout("invoice-detail"));
    }

    // ── filterObjects ───────────────────────────────────────────────────────

    #[test]
    fn filter_returns_empty_when_collection_not_readable() {
        let e = create_privilege_enforcer(client_set());
        let objects = vec![make_object("obj-1", &[])];
        assert!(e.filter_objects("contacts", &objects, &ctx()).is_empty());
    }

    #[test]
    fn filter_returns_all_when_no_record_filter() {
        let e = create_privilege_enforcer(client_set());
        let objects = vec![make_object("obj-1", &[]), make_object("obj-2", &[])];
        assert_eq!(e.filter_objects("invoices", &objects, &ctx()).len(), 2);
    }

    #[test]
    fn filter_by_record_filter_expression() {
        let filtered_set = create_privilege_set(
            "filtered",
            "Filtered",
            PrivilegeSetOptions {
                collections: collections(&[("*", CollectionPermission::Read)]),
                record_filter: Some("record.owner_did == current_did".into()),
                ..Default::default()
            },
        );
        let e = create_privilege_enforcer(filtered_set);
        let objects = vec![
            make_object("a", &[("owner_did", json!("did:key:alice"))]),
            make_object("b", &[("owner_did", json!("did:key:bob"))]),
            make_object("c", &[("owner_did", json!("did:key:alice"))]),
        ];
        let out = e.filter_objects("invoices", &objects, &ctx());
        assert_eq!(out.len(), 2);
        assert_eq!(
            out.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            vec!["a", "c"]
        );
    }

    #[test]
    fn filter_supports_ne_with_quoted_literal() {
        let filtered_set = create_privilege_set(
            "f",
            "F",
            PrivilegeSetOptions {
                collections: collections(&[("*", CollectionPermission::Read)]),
                record_filter: Some(r#"record.status != "archived""#.into()),
                ..Default::default()
            },
        );
        let e = create_privilege_enforcer(filtered_set);
        let objects = vec![
            make_object("a", &[("status", json!("active"))]),
            make_object("b", &[("status", json!("archived"))]),
        ];
        assert_eq!(e.filter_objects("col", &objects, &ctx()).len(), 1);
    }

    // ── redactObject ────────────────────────────────────────────────────────

    #[test]
    fn redact_strips_hidden_fields_from_data() {
        let e = create_privilege_enforcer(client_set());
        let obj = make_object(
            "obj-1",
            &[
                ("amount", json!(500)),
                ("cost_breakdown", json!("internal")),
                ("notes", json!("note")),
            ],
        );
        let redacted = e.redact_object("invoices", &obj);
        assert!(redacted.data.contains_key("amount"));
        assert!(redacted.data.contains_key("notes"));
        assert!(!redacted.data.contains_key("cost_breakdown"));
    }

    #[test]
    fn redact_returns_unmodified_when_no_field_overrides() {
        let e = create_privilege_enforcer(admin_set());
        let obj = make_object("obj-1", &[("a", json!(1)), ("b", json!(2))]);
        let redacted = e.redact_object("anything", &obj);
        assert_eq!(redacted.data, obj.data);
    }

    // ── visibleFields ───────────────────────────────────────────────────────

    #[test]
    fn visible_fields_filters_out_hidden() {
        let e = create_privilege_enforcer(client_set());
        let all: Vec<String> = ["amount", "cost_breakdown", "notes", "date"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let visible = e.visible_fields("invoices", &all);
        assert_eq!(visible, vec!["amount", "notes", "date"]);
    }
}
