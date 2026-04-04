/**
 * Prism Manifest types — the on-disk definition file for a workspace.
 *
 * ## Glossary (from SPEC.md)
 *
 *   Vault       — Encrypted local directory. The physical security boundary.
 *                  Files appear as unreadable blobs unless the Prism Daemon is
 *                  unlocked. A vault contains Collections and Manifests.
 *
 *   Collection  — A typed CRDT array (e.g. `Contacts`, `Audio_Busses`, `Tasks`).
 *                  Collections hold the actual data. Multiple manifests can
 *                  reference the same collection.
 *
 *   Manifest    — A YAML/JSON file containing weak references to Collections.
 *                  A "workspace" in user-facing terms is just a Manifest —
 *                  it points to data nodes, it does not contain them.
 *                  Example: a "JJM Productions" manifest in Flux points to
 *                  Contacts, Projects, and Tasks collections. A personal and
 *                  a professional manifest can both point to the same Contacts
 *                  collection, filtering via tags.
 *
 *   Shell       — The IDE chrome that renders whatever a Manifest references.
 *                  The shell has no fixed layout; it derives content from
 *                  registries (LensRegistry, ViewRegistry).
 *
 * ## File format
 *
 * Stored as `.prism.json` at the root of a vault directory.
 *
 * Ported from @core/project in legacy Helm. Adapted for Prism:
 *   - Loro CRDT is the primary storage, not SQLite
 *   - Tauri IPC, not HTTP — no http storage backend
 *   - Simplified sync to peer-based CRDT replication
 *   - Helm→Prism rename throughout
 */

// ── Storage config ─────────────────────────────────────────────────────────────

export type StorageBackend = "loro" | "memory" | "fs";

/** Loro CRDT document storage (default). */
export interface LoroStorageConfig {
  backend: "loro";
  /** Path to the Loro document file, relative to vault root. */
  path: string;
}

/** In-memory storage (testing / ephemeral). */
export interface MemoryStorageConfig {
  backend: "memory";
}

/** Filesystem-backed storage (Tauri fs). */
export interface FsStorageConfig {
  backend: "fs";
  /** Directory path relative to vault root. */
  directory: string;
}

export type StorageConfig =
  | LoroStorageConfig
  | MemoryStorageConfig
  | FsStorageConfig;

// ── Schema config ──────────────────────────────────────────────────────────────

export interface SchemaConfig {
  /**
   * Schema module references, resolved in order.
   *   - Built-in module: '@prism/core'
   *   - Relative path: './schemas/custom.yaml'
   * @default ['@prism/core']
   */
  modules: string[];
}

// ── Sync config ────────────────────────────────────────────────────────────────

export type SyncMode = "off" | "manual" | "auto";

export interface SyncConfig {
  mode: SyncMode;
  /** Sync interval in seconds when mode is 'auto'. 0 = continuous. */
  intervalSeconds?: number | undefined;
  /** Peer addresses for CRDT sync. */
  peers?: string[] | undefined;
}

// ── Collection reference ──────────────────────────────────────────────────────

/**
 * A reference to a Collection from within a Manifest.
 *
 * Collections are typed CRDT arrays that hold the actual data.
 * A CollectionRef is the manifest's pointer to a collection,
 * optionally with filters to narrow what's visible in this context.
 */
export interface CollectionRef {
  /** Unique id of this reference within the manifest. */
  id: string;
  /** Display name for this collection in the shell. */
  name: string;
  description?: string | undefined;
  /** Object type filter — only objects of these types appear. Empty = all. */
  objectTypes?: string[] | undefined;
  /** Tag filter — objects must have all listed tags. */
  tags?: string[] | undefined;
  /** Sort field. */
  sortBy?: string | undefined;
  sortDirection?: ("asc" | "desc") | undefined;
  /** Icon identifier. */
  icon?: string | undefined;
}

// ── Manifest ────────────────────────────────────────────────────────────────

export const MANIFEST_FILENAME = ".prism.json";
export const MANIFEST_VERSION = "1";

export type ManifestVisibility = "private" | "team" | "public";

/**
 * A Prism Manifest — the on-disk definition of a "workspace".
 *
 * A manifest is a named set of weak references to Collections inside a Vault.
 * It defines *what data to show* and *how to configure the shell*, but it
 * does not hold data itself. Multiple manifests can reference the same
 * collections with different filters.
 */
export interface PrismManifest {
  /** Stable UUID — identifies this manifest uniquely. */
  id: string;
  /** Display name shown in the title bar and recent list. */
  name: string;
  /** Schema version of this manifest format. Currently '1'. */
  version: string;
  /** Storage backend configuration for the vault. */
  storage: StorageConfig;
  /** Schema module configuration. */
  schema: SchemaConfig;
  /** Optional sync configuration. */
  sync?: SyncConfig | undefined;
  /** Collection references — weak pointers to typed CRDT arrays. */
  collections?: CollectionRef[] | undefined;
  /** ISO timestamp when this manifest was created. */
  createdAt: string;
  /** ISO timestamp when this manifest was last opened. */
  lastOpenedAt?: string | undefined;

  // ── Modules & settings ───────────────────────────────────────────────────

  /** Plugin/module enable flags. Keys = plugin IDs, values = enabled. */
  modules?: Record<string, boolean> | undefined;
  /** Manifest-scoped settings (dot-notation keys). */
  settings?: Record<string, unknown> | undefined;

  // ── Ownership ────────────────────────────────────────────────────────────

  /** DID or user ID of the manifest owner. */
  ownerId?: string | undefined;
  /** Visibility level. Default: 'private'. */
  visibility?: ManifestVisibility | undefined;
  /** Human-readable description. */
  description?: string | undefined;
}
