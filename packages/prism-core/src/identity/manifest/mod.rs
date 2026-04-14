//! `identity::manifest` — on-disk vault manifest + FileMaker-style
//! privilege sets.
//!
//! Port of `packages/prism-core/src/identity/manifest/` from the
//! legacy TS tree. Submodules:
//!
//! - [`privilege_set`]      — declarative access-control rules.
//! - [`privilege_enforcer`] — runtime evaluator that filters objects,
//!   redacts hidden fields, and answers read/write/see questions.
//! - [`manifest_types`]     — `PrismManifest` and the supporting
//!   `StorageConfig` / `SyncConfig` / `CollectionRef` plain-data
//!   shapes, with the `.prism.json` JSON layout preserved.
//! - [`manifest`]           — create, parse, serialise, validate,
//!   and the collection-ref edit helpers.

#[allow(clippy::module_inception)]
pub mod manifest;
pub mod manifest_types;
pub mod privilege_enforcer;
pub mod privilege_set;

pub use manifest::{
    add_collection, default_manifest, get_collection, parse_manifest, remove_collection,
    serialise_manifest, update_collection, validate_manifest, CollectionRefPatch, ManifestError,
    ManifestValidationError,
};
pub use manifest_types::{
    CollectionRef, ManifestVisibility, PrismManifest, SchemaConfig, SortDirection, StorageConfig,
    SyncConfig, SyncMode, MANIFEST_FILENAME, MANIFEST_VERSION,
};
pub use privilege_enforcer::{create_privilege_enforcer, PrivilegeContext, PrivilegeEnforcer};
pub use privilege_set::{
    can_read, can_write, create_privilege_set, get_collection_permission, get_field_permission,
    get_layout_permission, get_script_permission, CollectionPermission, FieldPermission,
    LayoutPermission, PrivilegeSet, PrivilegeSetOptions, RoleAssignment, ScriptPermission,
};
