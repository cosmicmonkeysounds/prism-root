/**
 * Workspace Manifest types — the identity envelope for a Prism workspace.
 *
 * A workspace is a vault: a named container with identity, storage config,
 * schema references, and sync policy. It does NOT hold data — it points
 * to where data lives (Loro CRDT documents, filesystem, etc.).
 *
 * Stored as `.prism.json` at the root of the workspace folder.
 *
 * Ported from @core/project in legacy Helm. Adapted for Prism:
 *   - Loro CRDT is the primary storage, not SQLite
 *   - Tauri IPC, not HTTP — no http storage backend
 *   - Simplified sync to loro-based providers
 *   - Helm→Prism rename throughout
 */

// ── Storage config ─────────────────────────────────────────────────────────────

export type StorageBackend = "loro" | "memory" | "fs";

/** Loro CRDT document storage (default). */
export interface LoroStorageConfig {
  backend: "loro";
  /** Path to the Loro document file, relative to workspace root. */
  path: string;
}

/** In-memory storage (testing / ephemeral). */
export interface MemoryStorageConfig {
  backend: "memory";
}

/** Filesystem-backed storage (Tauri fs). */
export interface FsStorageConfig {
  backend: "fs";
  /** Directory path relative to workspace root. */
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

// ── Collection ─────────────────────────────────────────────────────────────────

/** A named collection of objects within the workspace. */
export interface CollectionDef {
  id: string;
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

// ── Workspace Manifest ──────────────────────────────────────────────────────

export const MANIFEST_FILENAME = ".prism.json";
export const MANIFEST_VERSION = "1";

export type WorkspaceVisibility = "private" | "team" | "public";

export interface WorkspaceManifest {
  /** Stable UUID — identifies this workspace uniquely. */
  id: string;
  /** Display name shown in the title bar and recent list. */
  name: string;
  /** Schema version of this manifest format. Currently '1'. */
  version: string;
  /** Storage backend configuration. */
  storage: StorageConfig;
  /** Schema module configuration. */
  schema: SchemaConfig;
  /** Optional sync configuration. */
  sync?: SyncConfig | undefined;
  /** Named object collections. */
  collections?: CollectionDef[] | undefined;
  /** ISO timestamp when this workspace was created. */
  createdAt: string;
  /** ISO timestamp when this workspace was last opened. */
  lastOpenedAt?: string | undefined;

  // ── Modules & settings ───────────────────────────────────────────────────

  /** Plugin/module enable flags. Keys = plugin IDs, values = enabled. */
  modules?: Record<string, boolean> | undefined;
  /** Workspace-scoped settings (dot-notation keys). */
  settings?: Record<string, unknown> | undefined;

  // ── Ownership ────────────────────────────────────────────────────────────

  /** DID or user ID of the workspace owner. */
  ownerId?: string | undefined;
  /** Visibility level. Default: 'private'. */
  visibility?: WorkspaceVisibility | undefined;
  /** Human-readable description. */
  description?: string | undefined;
}
