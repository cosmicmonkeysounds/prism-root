//! Create, parse, serialise, and validate Prism Manifests.
//! Port of `identity/manifest/manifest.ts`. All field / error shapes
//! match the TS originals so existing on-disk `.prism.json` files
//! load through serde without a migration pass.

use std::collections::HashSet;

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use super::manifest_types::{
    CollectionRef, ManifestVisibility, PrismManifest, SchemaConfig, StorageConfig, MANIFEST_VERSION,
};

// ── Error types ────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("{0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("PrismManifest: missing required field \"{0}\"")]
    MissingField(&'static str),
    #[error("Collection ref '{0}' already exists")]
    DuplicateCollection(String),
    #[error("Collection ref '{0}' not found")]
    MissingCollection(String),
}

/// Non-fatal validation finding emitted by [`validate_manifest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestValidationError {
    pub field: String,
    pub message: String,
}

impl ManifestValidationError {
    fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

// ── Defaults ───────────────────────────────────────────────────────────────

/// Construct a default manifest with the Loro backend and the
/// `@prism/core` schema module — mirrors the TS `defaultManifest`.
pub fn default_manifest(name: impl Into<String>, id: impl Into<String>) -> PrismManifest {
    PrismManifest {
        id: id.into(),
        name: name.into(),
        version: MANIFEST_VERSION.to_string(),
        storage: StorageConfig::Loro {
            path: "./data/vault.loro".to_string(),
        },
        schema: SchemaConfig {
            modules: vec!["@prism/core".to_string()],
        },
        sync: None,
        collections: None,
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        last_opened_at: None,
        modules: None,
        settings: None,
        owner_id: None,
        visibility: Some(ManifestVisibility::Private),
        description: None,
        privilege_sets: None,
        role_assignments: None,
    }
}

// ── Parse ──────────────────────────────────────────────────────────────────

/// Parse a manifest JSON string. Required fields (`id`, `name`,
/// `storage`) must be present; missing optional fields are filled with
/// the same defaults the TS `parseManifest` used.
pub fn parse_manifest(json: &str) -> Result<PrismManifest, ManifestError> {
    let value: Value = serde_json::from_str(json)?;
    let Value::Object(map) = &value else {
        return Err(ManifestError::MissingField("id"));
    };

    if !map.get("id").map(is_nonempty_string).unwrap_or(false) {
        return Err(ManifestError::MissingField("id"));
    }
    if !map.get("name").map(is_nonempty_string).unwrap_or(false) {
        return Err(ManifestError::MissingField("name"));
    }
    if !map.contains_key("storage") || map.get("storage").map(Value::is_null).unwrap_or(true) {
        return Err(ManifestError::MissingField("storage"));
    }

    // Partial shape that only demands the required fields — everything
    // else is resolved below via the default/fallback pass. This
    // matches the TS `data: Partial<PrismManifest>` cast.
    #[derive(Deserialize)]
    struct Partial {
        id: String,
        name: String,
        version: Option<String>,
        storage: StorageConfig,
        schema: Option<SchemaConfig>,
        sync: Option<super::manifest_types::SyncConfig>,
        collections: Option<Vec<CollectionRef>>,
        #[serde(rename = "createdAt")]
        created_at: Option<String>,
        #[serde(rename = "lastOpenedAt")]
        last_opened_at: Option<String>,
        modules: Option<indexmap::IndexMap<String, bool>>,
        settings: Option<indexmap::IndexMap<String, Value>>,
        #[serde(rename = "ownerId")]
        owner_id: Option<String>,
        visibility: Option<ManifestVisibility>,
        description: Option<String>,
        #[serde(rename = "privilegeSets")]
        privilege_sets: Option<Vec<super::privilege_set::PrivilegeSet>>,
        #[serde(rename = "roleAssignments")]
        role_assignments: Option<Vec<super::privilege_set::RoleAssignment>>,
    }

    let p: Partial = serde_json::from_value(value)?;

    Ok(PrismManifest {
        id: p.id,
        name: p.name,
        version: p.version.unwrap_or_else(|| MANIFEST_VERSION.to_string()),
        storage: p.storage,
        schema: p.schema.unwrap_or_else(|| SchemaConfig {
            modules: vec!["@prism/core".to_string()],
        }),
        sync: p.sync,
        collections: p.collections,
        created_at: p
            .created_at
            .unwrap_or_else(|| Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)),
        last_opened_at: p.last_opened_at,
        modules: p.modules,
        settings: p.settings,
        owner_id: p.owner_id,
        visibility: p.visibility.or(Some(ManifestVisibility::Private)),
        description: p.description,
        privilege_sets: p.privilege_sets,
        role_assignments: p.role_assignments,
    })
}

fn is_nonempty_string(v: &Value) -> bool {
    matches!(v, Value::String(s) if !s.is_empty())
}

// ── Serialise ──────────────────────────────────────────────────────────────

/// Render a manifest as pretty-printed JSON (two-space indent, matching
/// the TS `JSON.stringify(_, null, 2)` output).
pub fn serialise_manifest(manifest: &PrismManifest) -> Result<String, ManifestError> {
    Ok(serde_json::to_string_pretty(manifest)?)
}

// ── Validate ───────────────────────────────────────────────────────────────

/// Run non-fatal validation checks. Returns an empty `Vec` when the
/// manifest passes; callers decide whether any reported error is a
/// hard failure.
pub fn validate_manifest(manifest: &PrismManifest) -> Vec<ManifestValidationError> {
    let mut errors = Vec::new();

    if manifest.id.is_empty() {
        errors.push(ManifestValidationError::new("id", "id is required"));
    }
    if manifest.name.is_empty() {
        errors.push(ManifestValidationError::new("name", "name is required"));
    }

    // StorageConfig always deserialises to a known backend because it
    // is a tagged enum; the TS "unknown backend" check only fires when
    // parsing raw JSON. We mirror that by round-tripping the declared
    // backend_name against the known set.
    let backend = manifest.storage.backend_name();
    if !matches!(backend, "loro" | "memory" | "fs") {
        errors.push(ManifestValidationError::new(
            "storage.backend",
            format!("unknown storage backend: {backend}"),
        ));
    }

    if !manifest.version.is_empty() && manifest.version != MANIFEST_VERSION {
        errors.push(ManifestValidationError::new(
            "version",
            format!(
                "unsupported version: {} (expected {})",
                manifest.version, MANIFEST_VERSION
            ),
        ));
    }

    // Visibility is tagged through serde — any bogus value would fail
    // to parse. This branch stays unreachable, and no TS behaviour is
    // lost.

    if let Some(collections) = manifest.collections.as_ref() {
        let mut ids: HashSet<&str> = HashSet::new();
        for col in collections {
            if col.id.is_empty() {
                errors.push(ManifestValidationError::new(
                    "collections",
                    "collection ref missing id",
                ));
            } else if !ids.insert(col.id.as_str()) {
                errors.push(ManifestValidationError::new(
                    "collections",
                    format!("duplicate collection ref id: {}", col.id),
                ));
            }
        }
    }

    errors
}

// ── Collection ref helpers ─────────────────────────────────────────────────

pub fn add_collection(
    manifest: &PrismManifest,
    collection: CollectionRef,
) -> Result<PrismManifest, ManifestError> {
    let existing = manifest.collections.clone().unwrap_or_default();
    if existing.iter().any(|c| c.id == collection.id) {
        return Err(ManifestError::DuplicateCollection(collection.id));
    }
    let mut next = manifest.clone();
    let mut cols = existing;
    cols.push(collection);
    next.collections = Some(cols);
    Ok(next)
}

pub fn remove_collection(manifest: &PrismManifest, collection_id: &str) -> PrismManifest {
    let existing = manifest.collections.clone().unwrap_or_default();
    let mut next = manifest.clone();
    next.collections = Some(
        existing
            .into_iter()
            .filter(|c| c.id != collection_id)
            .collect(),
    );
    next
}

/// Patch payload for [`update_collection`]. `id` is intentionally
/// absent — the target id is passed as a separate argument and cannot
/// be changed by the patch, matching TS `Partial<Omit<CollectionRef, "id">>`.
#[derive(Debug, Clone, Default)]
pub struct CollectionRefPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub object_types: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub sort_by: Option<String>,
    pub sort_direction: Option<super::manifest_types::SortDirection>,
    pub icon: Option<String>,
}

pub fn update_collection(
    manifest: &PrismManifest,
    collection_id: &str,
    patch: CollectionRefPatch,
) -> Result<PrismManifest, ManifestError> {
    let mut existing = manifest.collections.clone().unwrap_or_default();
    let idx = existing
        .iter()
        .position(|c| c.id == collection_id)
        .ok_or_else(|| ManifestError::MissingCollection(collection_id.to_string()))?;
    let target = &mut existing[idx];
    if let Some(v) = patch.name {
        target.name = v;
    }
    if let Some(v) = patch.description {
        target.description = Some(v);
    }
    if let Some(v) = patch.object_types {
        target.object_types = Some(v);
    }
    if let Some(v) = patch.tags {
        target.tags = Some(v);
    }
    if let Some(v) = patch.sort_by {
        target.sort_by = Some(v);
    }
    if let Some(v) = patch.sort_direction {
        target.sort_direction = Some(v);
    }
    if let Some(v) = patch.icon {
        target.icon = Some(v);
    }
    target.id = collection_id.to_string();
    let mut next = manifest.clone();
    next.collections = Some(existing);
    Ok(next)
}

pub fn get_collection<'a>(
    manifest: &'a PrismManifest,
    collection_id: &str,
) -> Option<&'a CollectionRef> {
    manifest
        .collections
        .as_ref()
        .and_then(|cols| cols.iter().find(|c| c.id == collection_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::manifest::manifest_types::{
        ManifestVisibility, StorageConfig, SyncConfig, SyncMode,
    };

    // ── defaultManifest ─────────────────────────────────────────────────────

    #[test]
    fn default_manifest_has_required_fields() {
        let m = default_manifest("My Project", "m-1");
        assert_eq!(m.id, "m-1");
        assert_eq!(m.name, "My Project");
        assert_eq!(m.version, MANIFEST_VERSION);
        assert!(matches!(m.storage, StorageConfig::Loro { .. }));
        assert_eq!(m.schema.modules, vec!["@prism/core".to_string()]);
        assert_eq!(m.visibility, Some(ManifestVisibility::Private));
        assert!(!m.created_at.is_empty());
    }

    // ── parseManifest ───────────────────────────────────────────────────────

    #[test]
    fn parses_minimal_manifest() {
        let json = r#"{
            "id": "m-1",
            "name": "Test",
            "storage": { "backend": "loro", "path": "./data/vault.loro" }
        }"#;
        let m = parse_manifest(json).unwrap();
        assert_eq!(m.id, "m-1");
        assert_eq!(m.name, "Test");
        assert_eq!(m.version, MANIFEST_VERSION);
        assert_eq!(m.schema.modules, vec!["@prism/core".to_string()]);
        assert_eq!(m.visibility, Some(ManifestVisibility::Private));
    }

    #[test]
    fn preserves_all_optional_fields() {
        let full = PrismManifest {
            id: "m-2".into(),
            name: "Full".into(),
            version: "1".into(),
            storage: StorageConfig::Memory,
            schema: SchemaConfig {
                modules: vec!["@prism/core".into(), "./custom.yaml".into()],
            },
            sync: Some(SyncConfig {
                mode: SyncMode::Auto,
                interval_seconds: Some(30),
                peers: Some(vec!["peer1".into()]),
            }),
            collections: Some(vec![CollectionRef::new("c1", "Tasks")]),
            created_at: "2026-01-01T00:00:00Z".into(),
            last_opened_at: Some("2026-04-01T00:00:00Z".into()),
            modules: Some(
                [("editor".to_string(), true), ("graph".to_string(), false)]
                    .into_iter()
                    .collect(),
            ),
            settings: Some(
                [("ui.theme".to_string(), Value::String("dark".into()))]
                    .into_iter()
                    .collect(),
            ),
            owner_id: Some("user-1".into()),
            visibility: Some(ManifestVisibility::Team),
            description: Some("A test manifest".into()),
            privilege_sets: None,
            role_assignments: None,
        };
        let json = serde_json::to_string(&full).unwrap();
        let m = parse_manifest(&json).unwrap();
        assert_eq!(m.sync.as_ref().unwrap().mode, SyncMode::Auto);
        assert_eq!(
            m.sync.as_ref().unwrap().peers.as_deref(),
            Some(&["peer1".to_string()][..])
        );
        assert_eq!(m.collections.as_ref().unwrap().len(), 1);
        assert_eq!(
            m.modules.as_ref().unwrap().get("editor").copied(),
            Some(true)
        );
        assert_eq!(
            m.settings.as_ref().unwrap().get("ui.theme"),
            Some(&Value::String("dark".into()))
        );
        assert_eq!(m.owner_id.as_deref(), Some("user-1"));
        assert_eq!(m.visibility, Some(ManifestVisibility::Team));
        assert_eq!(m.description.as_deref(), Some("A test manifest"));
    }

    #[test]
    fn throws_on_missing_id() {
        let err = parse_manifest(r#"{"name":"X","storage":{"backend":"memory"}}"#).unwrap_err();
        assert!(err.to_string().contains("missing required field \"id\""));
    }

    #[test]
    fn throws_on_missing_name() {
        let err = parse_manifest(r#"{"id":"x","storage":{"backend":"memory"}}"#).unwrap_err();
        assert!(err.to_string().contains("missing required field \"name\""));
    }

    #[test]
    fn throws_on_missing_storage() {
        let err = parse_manifest(r#"{"id":"x","name":"X"}"#).unwrap_err();
        assert!(err
            .to_string()
            .contains("missing required field \"storage\""));
    }

    // ── serialiseManifest ───────────────────────────────────────────────────

    #[test]
    fn serialise_roundtrips_through_parse() {
        let m = default_manifest("RoundTrip", "rt-1");
        let json = serialise_manifest(&m).unwrap();
        let parsed = parse_manifest(&json).unwrap();
        assert_eq!(parsed.id, "rt-1");
        assert_eq!(parsed.name, "RoundTrip");
        assert_eq!(parsed.storage, m.storage);
    }

    #[test]
    fn serialise_produces_formatted_json() {
        let m = default_manifest("Formatted", "fmt-1");
        let json = serialise_manifest(&m).unwrap();
        assert!(json.contains('\n'));
        assert!(json.contains("  "));
    }

    // ── validateManifest ────────────────────────────────────────────────────

    #[test]
    fn validate_valid_manifest_has_no_errors() {
        let m = default_manifest("Valid", "v-1");
        assert!(validate_manifest(&m).is_empty());
    }

    #[test]
    fn validate_reports_missing_id() {
        let mut m = default_manifest("X", "x");
        m.id = String::new();
        let errors = validate_manifest(&m);
        assert!(errors.iter().any(|e| e.field == "id"));
    }

    #[test]
    fn validate_reports_missing_name() {
        let mut m = default_manifest("X", "x");
        m.name = String::new();
        let errors = validate_manifest(&m);
        assert!(errors.iter().any(|e| e.field == "name"));
    }

    #[test]
    fn validate_reports_unsupported_version() {
        let mut m = default_manifest("X", "x");
        m.version = "99".into();
        let errors = validate_manifest(&m);
        assert!(errors.iter().any(|e| e.field == "version"));
    }

    #[test]
    fn validate_reports_duplicate_collection_ref_ids() {
        let mut m = default_manifest("X", "x");
        m.collections = Some(vec![
            CollectionRef::new("c1", "A"),
            CollectionRef::new("c1", "B"),
        ]);
        let errors = validate_manifest(&m);
        assert!(errors.iter().any(|e| e.message.contains("duplicate")));
    }

    #[test]
    fn validate_reports_collection_ref_missing_id() {
        let mut m = default_manifest("X", "x");
        m.collections = Some(vec![CollectionRef::new("", "A")]);
        let errors = validate_manifest(&m);
        assert!(errors.iter().any(|e| e.message.contains("missing id")));
    }

    // ── Collection ref helpers ──────────────────────────────────────────────

    fn col_tasks() -> CollectionRef {
        let mut c = CollectionRef::new("tasks", "Tasks");
        c.object_types = Some(vec!["task".into()]);
        c
    }

    fn col_goals() -> CollectionRef {
        let mut c = CollectionRef::new("goals", "Goals");
        c.object_types = Some(vec!["goal".into()]);
        c
    }

    #[test]
    fn add_collection_adds_to_manifest() {
        let base = default_manifest("Test", "t-1");
        let m = add_collection(&base, col_tasks()).unwrap();
        let cols = m.collections.as_ref().unwrap();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].id, "tasks");
    }

    #[test]
    fn add_collection_errors_on_duplicate() {
        let base = default_manifest("Test", "t-1");
        let m = add_collection(&base, col_tasks()).unwrap();
        let err = add_collection(&m, col_tasks()).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn remove_collection_removes_by_id() {
        let base = default_manifest("Test", "t-1");
        let m = add_collection(&base, col_tasks()).unwrap();
        let m = add_collection(&m, col_goals()).unwrap();
        let m = remove_collection(&m, "tasks");
        let cols = m.collections.as_ref().unwrap();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].id, "goals");
    }

    #[test]
    fn remove_collection_is_safe_for_missing_id() {
        let base = default_manifest("Test", "t-1");
        let m = add_collection(&base, col_tasks()).unwrap();
        let m2 = remove_collection(&m, "nonexistent");
        assert_eq!(m2.collections.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn update_collection_patches_fields() {
        let base = default_manifest("Test", "t-1");
        let m = add_collection(&base, col_tasks()).unwrap();
        let patch = CollectionRefPatch {
            name: Some("All Tasks".into()),
            sort_by: Some("date".into()),
            ..Default::default()
        };
        let m = update_collection(&m, "tasks", patch).unwrap();
        let c = &m.collections.as_ref().unwrap()[0];
        assert_eq!(c.name, "All Tasks");
        assert_eq!(c.sort_by.as_deref(), Some("date"));
        assert_eq!(c.id, "tasks");
    }

    #[test]
    fn update_collection_errors_for_missing() {
        let base = default_manifest("Test", "t-1");
        let err = update_collection(&base, "missing", CollectionRefPatch::default()).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn get_collection_returns_ref_by_id() {
        let base = default_manifest("Test", "t-1");
        let m = add_collection(&base, col_tasks()).unwrap();
        let c = get_collection(&m, "tasks").unwrap();
        assert_eq!(c.id, "tasks");
        assert_eq!(c.name, "Tasks");
    }

    #[test]
    fn get_collection_returns_none_for_missing() {
        let base = default_manifest("Test", "t-1");
        assert!(get_collection(&base, "missing").is_none());
    }
}
