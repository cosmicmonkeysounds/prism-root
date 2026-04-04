/**
 * Prism Manifest — create, parse, serialise, validate.
 *
 * A manifest is a named set of weak references to Collections inside a Vault.
 * It defines what data to show and how to configure the shell.
 * See manifest-types.ts for the full glossary (Vault / Collection / Manifest / Shell).
 */

import type {
  PrismManifest,
  CollectionRef,
} from "./manifest-types.js";
import { MANIFEST_VERSION } from "./manifest-types.js";

// ── Defaults ────────────────────────────────────────────────────────────────

export function defaultManifest(name: string, id: string): PrismManifest {
  return {
    id,
    name,
    version: MANIFEST_VERSION,
    storage: { backend: "loro", path: "./data/vault.loro" },
    schema: { modules: ["@prism/core"] },
    createdAt: new Date().toISOString(),
    visibility: "private",
  };
}

// ── Parse ───────────────────────────────────────────────────────────────────

export function parseManifest(json: string): PrismManifest {
  const data = JSON.parse(json) as Partial<PrismManifest>;
  if (!data.id) throw new Error("PrismManifest: missing required field \"id\"");
  if (!data.name) throw new Error("PrismManifest: missing required field \"name\"");
  if (!data.storage) throw new Error("PrismManifest: missing required field \"storage\"");
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

export function serialiseManifest(manifest: PrismManifest): string {
  return JSON.stringify(manifest, null, 2);
}

// ── Validate ────────────────────────────────────────────────────────────────

export interface ManifestValidationError {
  field: string;
  message: string;
}

export function validateManifest(
  manifest: PrismManifest,
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
        errors.push({ field: "collections", message: "collection ref missing id" });
      } else if (ids.has(col.id)) {
        errors.push({
          field: "collections",
          message: `duplicate collection ref id: ${col.id}`,
        });
      }
      ids.add(col.id);
    }
  }

  return errors;
}

// ── Collection ref helpers ──────────────────────────────────────────────────

export function addCollection(
  manifest: PrismManifest,
  collection: CollectionRef,
): PrismManifest {
  const existing = manifest.collections ?? [];
  if (existing.some((c) => c.id === collection.id)) {
    throw new Error(`Collection ref '${collection.id}' already exists`);
  }
  return { ...manifest, collections: [...existing, collection] };
}

export function removeCollection(
  manifest: PrismManifest,
  collectionId: string,
): PrismManifest {
  const existing = manifest.collections ?? [];
  return {
    ...manifest,
    collections: existing.filter((c) => c.id !== collectionId),
  };
}

export function updateCollection(
  manifest: PrismManifest,
  collectionId: string,
  patch: Partial<Omit<CollectionRef, "id">>,
): PrismManifest {
  const existing = manifest.collections ?? [];
  const idx = existing.findIndex((c) => c.id === collectionId);
  if (idx === -1) throw new Error(`Collection ref '${collectionId}' not found`);
  const updated = [...existing];
  updated[idx] = { ...existing[idx]!, ...patch, id: collectionId };
  return { ...manifest, collections: updated };
}

export function getCollection(
  manifest: PrismManifest,
  collectionId: string,
): CollectionRef | undefined {
  return (manifest.collections ?? []).find((c) => c.id === collectionId);
}
