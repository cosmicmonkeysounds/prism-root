/**
 * Workspace Manifest — create, parse, serialise, validate.
 *
 * A workspace is a vault/shell: identity + config envelope pointing to
 * where data lives. The manifest is the on-disk representation of that
 * envelope, stored as `.prism.json`.
 */

import type {
  WorkspaceManifest,
  CollectionDef,
} from "./manifest-types.js";
import { MANIFEST_VERSION } from "./manifest-types.js";

// ── Defaults ────────────────────────────────────────────────────────────────

export function defaultManifest(name: string, id: string): WorkspaceManifest {
  return {
    id,
    name,
    version: MANIFEST_VERSION,
    storage: { backend: "loro", path: "./data/workspace.loro" },
    schema: { modules: ["@prism/core"] },
    createdAt: new Date().toISOString(),
    visibility: "private",
  };
}

// ── Parse ───────────────────────────────────────────────────────────────────

export function parseManifest(json: string): WorkspaceManifest {
  const data = JSON.parse(json) as Partial<WorkspaceManifest>;
  if (!data.id) throw new Error("WorkspaceManifest: missing required field \"id\"");
  if (!data.name) throw new Error("WorkspaceManifest: missing required field \"name\"");
  if (!data.storage) throw new Error("WorkspaceManifest: missing required field \"storage\"");
  return {
    id: data.id,
    name: data.name,
    version: data.version ?? MANIFEST_VERSION,
    storage: data.storage,
    schema: data.schema ?? { modules: ["@prism/core"] },
    sync: data.sync,
    collections: data.collections,
    createdAt: data.createdAt ?? new Date().toISOString(),
    lastOpenedAt: data.lastOpenedAt,
    modules: data.modules,
    settings: data.settings,
    ownerId: data.ownerId,
    visibility: data.visibility ?? "private",
    description: data.description,
  };
}

// ── Serialise ───────────────────────────────────────────────────────────────

export function serialiseManifest(manifest: WorkspaceManifest): string {
  return JSON.stringify(manifest, null, 2);
}

// ── Validate ────────────────────────────────────────────────────────────────

export interface ManifestValidationError {
  field: string;
  message: string;
}

export function validateManifest(
  manifest: WorkspaceManifest,
): ManifestValidationError[] {
  const errors: ManifestValidationError[] = [];

  if (!manifest.id) errors.push({ field: "id", message: "id is required" });
  if (!manifest.name) errors.push({ field: "name", message: "name is required" });
  if (!manifest.storage) {
    errors.push({ field: "storage", message: "storage is required" });
  } else {
    const backend = manifest.storage.backend;
    if (!["loro", "memory", "fs"].includes(backend)) {
      errors.push({
        field: "storage.backend",
        message: `unknown storage backend: ${backend}`,
      });
    }
  }
  if (manifest.version && manifest.version !== MANIFEST_VERSION) {
    errors.push({
      field: "version",
      message: `unsupported version: ${manifest.version} (expected ${MANIFEST_VERSION})`,
    });
  }
  if (manifest.visibility && !["private", "team", "public"].includes(manifest.visibility)) {
    errors.push({
      field: "visibility",
      message: `unknown visibility: ${manifest.visibility}`,
    });
  }
  if (manifest.collections) {
    const ids = new Set<string>();
    for (const col of manifest.collections) {
      if (!col.id) {
        errors.push({ field: "collections", message: "collection missing id" });
      } else if (ids.has(col.id)) {
        errors.push({
          field: "collections",
          message: `duplicate collection id: ${col.id}`,
        });
      }
      ids.add(col.id);
    }
  }

  return errors;
}

// ── Collection helpers ──────────────────────────────────────────────────────

export function addCollection(
  manifest: WorkspaceManifest,
  collection: CollectionDef,
): WorkspaceManifest {
  const existing = manifest.collections ?? [];
  if (existing.some((c) => c.id === collection.id)) {
    throw new Error(`Collection '${collection.id}' already exists`);
  }
  return { ...manifest, collections: [...existing, collection] };
}

export function removeCollection(
  manifest: WorkspaceManifest,
  collectionId: string,
): WorkspaceManifest {
  const existing = manifest.collections ?? [];
  return {
    ...manifest,
    collections: existing.filter((c) => c.id !== collectionId),
  };
}

export function updateCollection(
  manifest: WorkspaceManifest,
  collectionId: string,
  patch: Partial<Omit<CollectionDef, "id">>,
): WorkspaceManifest {
  const existing = manifest.collections ?? [];
  const idx = existing.findIndex((c) => c.id === collectionId);
  if (idx === -1) throw new Error(`Collection '${collectionId}' not found`);
  const updated = [...existing];
  updated[idx] = { ...existing[idx]!, ...patch, id: collectionId };
  return { ...manifest, collections: updated };
}

export function getCollection(
  manifest: WorkspaceManifest,
  collectionId: string,
): CollectionDef | undefined {
  return (manifest.collections ?? []).find((c) => c.id === collectionId);
}
