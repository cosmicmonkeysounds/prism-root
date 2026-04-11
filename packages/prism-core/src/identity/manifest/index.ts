export type {
  StorageBackend,
  LoroStorageConfig,
  MemoryStorageConfig,
  FsStorageConfig,
  StorageConfig,
  SchemaConfig,
  SyncMode,
  SyncConfig,
  CollectionRef,
  ManifestVisibility,
  PrismManifest,
} from "./manifest-types.js";

export { MANIFEST_FILENAME, MANIFEST_VERSION } from "./manifest-types.js";

export {
  defaultManifest,
  parseManifest,
  serialiseManifest,
  validateManifest,
  addCollection,
  removeCollection,
  updateCollection,
  getCollection,
} from "./manifest.js";

export type { ManifestValidationError } from "./manifest.js";

// ── Privilege Sets (access control) ─────────────────────────────────────
export type {
  CollectionPermission,
  FieldPermission,
  LayoutPermission,
  ScriptPermission,
  PrivilegeSet,
  PrivilegeSetOptions,
  RoleAssignment,
} from "./privilege-set.js";
export {
  createPrivilegeSet,
  getCollectionPermission,
  getFieldPermission,
  getLayoutPermission,
  getScriptPermission,
  canWrite,
  canRead,
} from "./privilege-set.js";

// ── Privilege Enforcer (runtime evaluation) ─────────────────────────────
export type {
  PrivilegeContext,
  PrivilegeEnforcer,
} from "./privilege-enforcer.js";
export { createPrivilegeEnforcer } from "./privilege-enforcer.js";
