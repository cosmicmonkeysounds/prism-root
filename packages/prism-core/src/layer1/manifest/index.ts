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
