//! FileMaker-style privilege sets — granular access control for Prism
//! workspaces. Declarative rules map DID roles to collection, field,
//! layout, and script permissions, with optional row-level filtering.
//!
//! Port of `identity/manifest/privilege-set.ts`. The shapes serialise
//! through serde the same way the TS JSON landed on disk.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

// ── Permission Levels ───────────────────────────────────────────────────────

/// Collection-level access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CollectionPermission {
    Full,
    Read,
    Create,
    None,
}

/// Field-level access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldPermission {
    Readwrite,
    Readonly,
    Hidden,
}

/// Layout-level visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutPermission {
    Visible,
    Hidden,
}

/// Script execution permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScriptPermission {
    Execute,
    None,
}

// ── Privilege Set ───────────────────────────────────────────────────────────

/// Declarative rule set mapping a role to permissions. Keys in
/// [`collections`], [`layouts`], and [`scripts`] may be a collection /
/// layout / script id or `"*"` for the default bucket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrivilegeSet {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub collections: IndexMap<String, CollectionPermission>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<IndexMap<String, FieldPermission>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layouts: Option<IndexMap<String, LayoutPermission>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scripts: Option<IndexMap<String, ScriptPermission>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can_manage_access: Option<bool>,
}

/// Options accepted by [`create_privilege_set`]. Mirrors the TS
/// `PrivilegeSetOptions` interface.
#[derive(Debug, Clone, Default)]
pub struct PrivilegeSetOptions {
    pub collections: IndexMap<String, CollectionPermission>,
    pub fields: Option<IndexMap<String, FieldPermission>>,
    pub layouts: Option<IndexMap<String, LayoutPermission>>,
    pub scripts: Option<IndexMap<String, ScriptPermission>>,
    pub record_filter: Option<String>,
    pub is_default: Option<bool>,
    pub can_manage_access: Option<bool>,
}

/// Factory. Clones the collection map defensively to match the TS
/// `{ ...options.collections }` behaviour — mutations to the source
/// map after construction must not bleed into the returned set.
pub fn create_privilege_set(
    id: impl Into<String>,
    name: impl Into<String>,
    options: PrivilegeSetOptions,
) -> PrivilegeSet {
    PrivilegeSet {
        id: id.into(),
        name: name.into(),
        description: None,
        collections: options.collections.clone(),
        fields: options.fields.clone(),
        layouts: options.layouts.clone(),
        scripts: options.scripts.clone(),
        record_filter: options.record_filter,
        is_default: options.is_default,
        can_manage_access: options.can_manage_access,
    }
}

// ── Role Assignment ─────────────────────────────────────────────────────────

/// Maps a DID to a privilege set. Stored in the Manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleAssignment {
    pub did: String,
    pub privilege_set_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

// ── Evaluation helpers ──────────────────────────────────────────────────────

/// Check collection-level permission. Falls back to `"*"` wildcard,
/// then [`CollectionPermission::None`].
pub fn get_collection_permission(
    privilege_set: &PrivilegeSet,
    collection_id: &str,
) -> CollectionPermission {
    privilege_set
        .collections
        .get(collection_id)
        .copied()
        .or_else(|| privilege_set.collections.get("*").copied())
        .unwrap_or(CollectionPermission::None)
}

/// Check field-level permission. Falls back to the collection
/// permission mapped to a field equivalent:
/// `full`/`create` → `readwrite`, `read` → `readonly`, `none` → `hidden`.
pub fn get_field_permission(
    privilege_set: &PrivilegeSet,
    collection_id: &str,
    field_path: &str,
) -> FieldPermission {
    let field_key = format!("{collection_id}.{field_path}");
    if let Some(fields) = privilege_set.fields.as_ref() {
        if let Some(perm) = fields.get(&field_key).copied() {
            return perm;
        }
    }
    match get_collection_permission(privilege_set, collection_id) {
        CollectionPermission::Full | CollectionPermission::Create => FieldPermission::Readwrite,
        CollectionPermission::Read => FieldPermission::Readonly,
        CollectionPermission::None => FieldPermission::Hidden,
    }
}

/// Check layout visibility. Falls back to `"*"` wildcard, then
/// [`LayoutPermission::Visible`].
pub fn get_layout_permission(privilege_set: &PrivilegeSet, layout_id: &str) -> LayoutPermission {
    privilege_set
        .layouts
        .as_ref()
        .and_then(|m| m.get(layout_id).copied().or_else(|| m.get("*").copied()))
        .unwrap_or(LayoutPermission::Visible)
}

/// Check script execution permission. Falls back to `"*"` wildcard,
/// then [`ScriptPermission::None`].
pub fn get_script_permission(privilege_set: &PrivilegeSet, script_id: &str) -> ScriptPermission {
    privilege_set
        .scripts
        .as_ref()
        .and_then(|m| m.get(script_id).copied().or_else(|| m.get("*").copied()))
        .unwrap_or(ScriptPermission::None)
}

/// Check if a collection allows write operations.
pub fn can_write(privilege_set: &PrivilegeSet, collection_id: &str) -> bool {
    matches!(
        get_collection_permission(privilege_set, collection_id),
        CollectionPermission::Full | CollectionPermission::Create
    )
}

/// Check if a collection allows read operations.
pub fn can_read(privilege_set: &PrivilegeSet, collection_id: &str) -> bool {
    !matches!(
        get_collection_permission(privilege_set, collection_id),
        CollectionPermission::None
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn scripts(pairs: &[(&str, ScriptPermission)]) -> IndexMap<String, ScriptPermission> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    // ── createPrivilegeSet factory ──────────────────────────────────────────

    #[test]
    fn creates_with_id_and_name() {
        let ps = create_privilege_set(
            "admin",
            "Administrator",
            PrivilegeSetOptions {
                collections: collections(&[("*", CollectionPermission::Full)]),
                ..Default::default()
            },
        );
        assert_eq!(ps.id, "admin");
        assert_eq!(ps.name, "Administrator");
    }

    #[test]
    fn copies_collection_permissions_defensively() {
        let mut source = collections(&[
            ("invoices", CollectionPermission::Read),
            ("contacts", CollectionPermission::Full),
        ]);
        let ps = create_privilege_set(
            "viewer",
            "Viewer",
            PrivilegeSetOptions {
                collections: source.clone(),
                ..Default::default()
            },
        );
        source.insert("invoices".into(), CollectionPermission::Full);
        assert_eq!(
            ps.collections.get("invoices").copied(),
            Some(CollectionPermission::Read)
        );
    }

    #[test]
    fn includes_optional_fields_when_provided() {
        let ps = create_privilege_set(
            "client",
            "Client",
            PrivilegeSetOptions {
                collections: collections(&[("*", CollectionPermission::Read)]),
                fields: Some(fields(&[("invoices.cost", FieldPermission::Hidden)])),
                layouts: Some(layouts(&[("admin-panel", LayoutPermission::Hidden)])),
                scripts: Some(scripts(&[("delete-all", ScriptPermission::None)])),
                record_filter: Some("record.owner == current_did".into()),
                is_default: Some(true),
                can_manage_access: Some(false),
            },
        );
        assert_eq!(
            ps.fields.as_ref().unwrap().get("invoices.cost").copied(),
            Some(FieldPermission::Hidden)
        );
        assert_eq!(
            ps.layouts.as_ref().unwrap().get("admin-panel").copied(),
            Some(LayoutPermission::Hidden)
        );
        assert_eq!(
            ps.scripts.as_ref().unwrap().get("delete-all").copied(),
            Some(ScriptPermission::None)
        );
        assert_eq!(
            ps.record_filter.as_deref(),
            Some("record.owner == current_did")
        );
        assert_eq!(ps.is_default, Some(true));
        assert_eq!(ps.can_manage_access, Some(false));
    }

    #[test]
    fn omits_optional_fields_when_not_provided() {
        let ps = create_privilege_set(
            "basic",
            "Basic",
            PrivilegeSetOptions {
                collections: collections(&[("*", CollectionPermission::Read)]),
                ..Default::default()
            },
        );
        assert!(ps.fields.is_none());
        assert!(ps.layouts.is_none());
        assert!(ps.scripts.is_none());
        assert!(ps.record_filter.is_none());
    }

    // ── getCollectionPermission ─────────────────────────────────────────────

    fn admin() -> PrivilegeSet {
        create_privilege_set(
            "admin",
            "Admin",
            PrivilegeSetOptions {
                collections: collections(&[("*", CollectionPermission::Full)]),
                ..Default::default()
            },
        )
    }

    fn mixed() -> PrivilegeSet {
        create_privilege_set(
            "mixed",
            "Mixed",
            PrivilegeSetOptions {
                collections: collections(&[
                    ("invoices", CollectionPermission::Read),
                    ("contacts", CollectionPermission::Full),
                    ("*", CollectionPermission::None),
                ]),
                ..Default::default()
            },
        )
    }

    #[test]
    fn returns_specific_collection_permission() {
        let m = mixed();
        assert_eq!(
            get_collection_permission(&m, "invoices"),
            CollectionPermission::Read
        );
        assert_eq!(
            get_collection_permission(&m, "contacts"),
            CollectionPermission::Full
        );
    }

    #[test]
    fn falls_back_to_wildcard_collection() {
        assert_eq!(
            get_collection_permission(&admin(), "anything"),
            CollectionPermission::Full
        );
        assert_eq!(
            get_collection_permission(&mixed(), "unknown"),
            CollectionPermission::None
        );
    }

    #[test]
    fn returns_none_when_no_wildcard_or_match() {
        let strict = create_privilege_set(
            "strict",
            "Strict",
            PrivilegeSetOptions {
                collections: collections(&[("invoices", CollectionPermission::Read)]),
                ..Default::default()
            },
        );
        assert_eq!(
            get_collection_permission(&strict, "contacts"),
            CollectionPermission::None
        );
    }

    // ── getFieldPermission ──────────────────────────────────────────────────

    fn client_field_ps() -> PrivilegeSet {
        create_privilege_set(
            "client",
            "Client",
            PrivilegeSetOptions {
                collections: collections(&[
                    ("invoices", CollectionPermission::Read),
                    ("contacts", CollectionPermission::Full),
                ]),
                fields: Some(fields(&[
                    ("invoices.cost_breakdown", FieldPermission::Hidden),
                    ("contacts.email", FieldPermission::Readonly),
                ])),
                ..Default::default()
            },
        )
    }

    #[test]
    fn returns_explicit_field_permission() {
        let ps = client_field_ps();
        assert_eq!(
            get_field_permission(&ps, "invoices", "cost_breakdown"),
            FieldPermission::Hidden
        );
        assert_eq!(
            get_field_permission(&ps, "contacts", "email"),
            FieldPermission::Readonly
        );
    }

    #[test]
    fn derives_field_from_collection_permission() {
        let ps = client_field_ps();
        assert_eq!(
            get_field_permission(&ps, "invoices", "date"),
            FieldPermission::Readonly
        );
        assert_eq!(
            get_field_permission(&ps, "contacts", "name"),
            FieldPermission::Readwrite
        );
    }

    #[test]
    fn field_returns_hidden_when_collection_is_none() {
        let strict = create_privilege_set(
            "s",
            "S",
            PrivilegeSetOptions {
                collections: collections(&[("secret", CollectionPermission::None)]),
                ..Default::default()
            },
        );
        assert_eq!(
            get_field_permission(&strict, "secret", "data"),
            FieldPermission::Hidden
        );
    }

    #[test]
    fn derives_readwrite_from_create_permission() {
        let creator = create_privilege_set(
            "c",
            "C",
            PrivilegeSetOptions {
                collections: collections(&[("items", CollectionPermission::Create)]),
                ..Default::default()
            },
        );
        assert_eq!(
            get_field_permission(&creator, "items", "name"),
            FieldPermission::Readwrite
        );
    }

    // ── getLayoutPermission ─────────────────────────────────────────────────

    #[test]
    fn returns_specific_layout_permission() {
        let ps = create_privilege_set(
            "p",
            "P",
            PrivilegeSetOptions {
                collections: IndexMap::new(),
                layouts: Some(layouts(&[
                    ("admin-dashboard", LayoutPermission::Hidden),
                    ("public-view", LayoutPermission::Visible),
                ])),
                ..Default::default()
            },
        );
        assert_eq!(
            get_layout_permission(&ps, "admin-dashboard"),
            LayoutPermission::Hidden
        );
        assert_eq!(
            get_layout_permission(&ps, "public-view"),
            LayoutPermission::Visible
        );
    }

    #[test]
    fn layout_falls_back_to_wildcard() {
        let ps = create_privilege_set(
            "p",
            "P",
            PrivilegeSetOptions {
                collections: IndexMap::new(),
                layouts: Some(layouts(&[("*", LayoutPermission::Hidden)])),
                ..Default::default()
            },
        );
        assert_eq!(
            get_layout_permission(&ps, "any-layout"),
            LayoutPermission::Hidden
        );
    }

    #[test]
    fn layout_defaults_to_visible_when_empty() {
        let ps = create_privilege_set(
            "p",
            "P",
            PrivilegeSetOptions {
                collections: IndexMap::new(),
                ..Default::default()
            },
        );
        assert_eq!(
            get_layout_permission(&ps, "anything"),
            LayoutPermission::Visible
        );
    }

    // ── getScriptPermission ─────────────────────────────────────────────────

    #[test]
    fn returns_specific_script_permission() {
        let ps = create_privilege_set(
            "p",
            "P",
            PrivilegeSetOptions {
                collections: IndexMap::new(),
                scripts: Some(scripts(&[
                    ("safe-script", ScriptPermission::Execute),
                    ("dangerous-script", ScriptPermission::None),
                ])),
                ..Default::default()
            },
        );
        assert_eq!(
            get_script_permission(&ps, "safe-script"),
            ScriptPermission::Execute
        );
        assert_eq!(
            get_script_permission(&ps, "dangerous-script"),
            ScriptPermission::None
        );
    }

    #[test]
    fn script_falls_back_to_wildcard() {
        let ps = create_privilege_set(
            "p",
            "P",
            PrivilegeSetOptions {
                collections: IndexMap::new(),
                scripts: Some(scripts(&[("*", ScriptPermission::Execute)])),
                ..Default::default()
            },
        );
        assert_eq!(
            get_script_permission(&ps, "any-script"),
            ScriptPermission::Execute
        );
    }

    #[test]
    fn script_defaults_to_none_when_empty() {
        let ps = create_privilege_set(
            "p",
            "P",
            PrivilegeSetOptions {
                collections: IndexMap::new(),
                ..Default::default()
            },
        );
        assert_eq!(
            get_script_permission(&ps, "anything"),
            ScriptPermission::None
        );
    }

    // ── canWrite / canRead helpers ──────────────────────────────────────────

    #[test]
    fn can_write_true_for_full_and_create() {
        let ps = create_privilege_set(
            "p",
            "P",
            PrivilegeSetOptions {
                collections: collections(&[
                    ("a", CollectionPermission::Full),
                    ("b", CollectionPermission::Create),
                    ("c", CollectionPermission::Read),
                    ("d", CollectionPermission::None),
                ]),
                ..Default::default()
            },
        );
        assert!(can_write(&ps, "a"));
        assert!(can_write(&ps, "b"));
    }

    #[test]
    fn can_write_false_for_read_and_none() {
        let ps = create_privilege_set(
            "p",
            "P",
            PrivilegeSetOptions {
                collections: collections(&[
                    ("c", CollectionPermission::Read),
                    ("d", CollectionPermission::None),
                ]),
                ..Default::default()
            },
        );
        assert!(!can_write(&ps, "c"));
        assert!(!can_write(&ps, "d"));
    }

    #[test]
    fn can_read_true_for_full_create_and_read() {
        let ps = create_privilege_set(
            "p",
            "P",
            PrivilegeSetOptions {
                collections: collections(&[
                    ("a", CollectionPermission::Full),
                    ("b", CollectionPermission::Create),
                    ("c", CollectionPermission::Read),
                ]),
                ..Default::default()
            },
        );
        assert!(can_read(&ps, "a"));
        assert!(can_read(&ps, "b"));
        assert!(can_read(&ps, "c"));
    }

    #[test]
    fn can_read_false_for_none() {
        let ps = create_privilege_set(
            "p",
            "P",
            PrivilegeSetOptions {
                collections: collections(&[("d", CollectionPermission::None)]),
                ..Default::default()
            },
        );
        assert!(!can_read(&ps, "d"));
    }
}
