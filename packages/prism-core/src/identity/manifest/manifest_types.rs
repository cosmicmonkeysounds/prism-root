//! On-disk definition types for a Prism vault. Ported from
//! `identity/manifest/manifest-types.ts`.
//!
//! Glossary (from `SPEC.md`): a **Vault** is the encrypted directory,
//! a **Collection** is a typed CRDT array holding the actual data, a
//! **Manifest** is a weak-reference file pointing to collections, and
//! the **Shell** is the chrome that renders whatever the manifest
//! references.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::privilege_set::{PrivilegeSet, RoleAssignment};

// ── Storage config ─────────────────────────────────────────────────

/// Tagged union of the three storage backends. JSON shape matches the
/// TS discriminated union on the `backend` key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "lowercase")]
pub enum StorageConfig {
    Loro {
        /// Path to the Loro document file, relative to vault root.
        path: String,
    },
    Memory,
    Fs {
        /// Directory path relative to vault root.
        directory: String,
    },
}

impl StorageConfig {
    /// Name of the active backend — `"loro"`, `"memory"`, or `"fs"`.
    /// Used by [`validate_manifest`] when we need to dump a backend
    /// name back into an error message.
    pub fn backend_name(&self) -> &'static str {
        match self {
            StorageConfig::Loro { .. } => "loro",
            StorageConfig::Memory => "memory",
            StorageConfig::Fs { .. } => "fs",
        }
    }
}

// ── Schema config ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaConfig {
    pub modules: Vec<String>,
}

// ── Sync config ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncMode {
    Off,
    Manual,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncConfig {
    pub mode: SyncMode,
    #[serde(
        default,
        rename = "intervalSeconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub interval_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peers: Option<Vec<String>>,
}

// ── Collection reference ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    Desc,
}

/// A reference to a Collection from within a Manifest. Weakly points
/// at the data, never contains it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionRef {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(
        default,
        rename = "objectTypes",
        skip_serializing_if = "Option::is_none"
    )]
    pub object_types: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, rename = "sortBy", skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<String>,
    #[serde(
        default,
        rename = "sortDirection",
        skip_serializing_if = "Option::is_none"
    )]
    pub sort_direction: Option<SortDirection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

impl CollectionRef {
    /// Convenience constructor used in tests and templates.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            object_types: None,
            tags: None,
            sort_by: None,
            sort_direction: None,
            icon: None,
        }
    }
}

// ── Manifest ───────────────────────────────────────────────────────

pub const MANIFEST_FILENAME: &str = ".prism.json";
pub const MANIFEST_VERSION: &str = "1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManifestVisibility {
    Private,
    Team,
    Public,
}

/// A Prism Manifest — the on-disk definition of a workspace. Field
/// naming follows the TS JSON layout (camelCase) so on-disk files
/// round-trip through serde without a rename pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrismManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub storage: StorageConfig,
    pub schema: SchemaConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync: Option<SyncConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collections: Option<Vec<CollectionRef>>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(
        default,
        rename = "lastOpenedAt",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_opened_at: Option<String>,

    // ── Modules & settings ───────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modules: Option<IndexMap<String, bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<IndexMap<String, Value>>,

    // ── Ownership ────────────────────────────────────────────────
    #[serde(default, rename = "ownerId", skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<ManifestVisibility>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    // ── Access control ───────────────────────────────────────────
    #[serde(
        default,
        rename = "privilegeSets",
        skip_serializing_if = "Option::is_none"
    )]
    pub privilege_sets: Option<Vec<PrivilegeSet>>,
    #[serde(
        default,
        rename = "roleAssignments",
        skip_serializing_if = "Option::is_none"
    )]
    pub role_assignments: Option<Vec<RoleAssignment>>,
}
