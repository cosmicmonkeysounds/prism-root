/**
 * Vault Persistence — load/save CollectionStores from/to storage backends.
 *
 * Two layers:
 *
 *   PersistenceAdapter  — pluggable I/O interface (memory, Tauri IPC, fs).
 *                         Synchronous — Loro CRDT ops are sync, so persistence
 *                         follows suit. Async adapters wrap internally.
 *
 *   VaultManager        — orchestrates a manifest's collections against an adapter.
 *                         Lazy-loads collection stores on first access, tracks
 *                         dirty state, saves snapshots on demand.
 *
 * The memory adapter is included for testing. Real adapters (Tauri IPC, fs)
 * are provided by the host environment.
 */

import type { CollectionRef, PrismManifest } from "../manifest/index.js";
import type { CollectionStore } from "./collection-store.js";
import { createCollectionStore } from "./collection-store.js";

// ── Persistence Adapter ───────────────────────────────────────────────────────

/**
 * Pluggable storage I/O for Loro snapshots.
 *
 * Paths are relative to the vault root. The adapter resolves them
 * against the actual filesystem / IPC backend.
 */
export interface PersistenceAdapter {
  /** Load a binary blob from storage. Returns null if not found. */
  load(path: string): Uint8Array | null;
  /** Save a binary blob to storage. Creates parent dirs as needed. */
  save(path: string, data: Uint8Array): void;
  /** Delete a blob from storage. Returns true if it existed. */
  delete(path: string): boolean;
  /** Check if a path exists. */
  exists(path: string): boolean;
  /** List entries in a directory (relative paths). */
  list(directory: string): string[];
}

// ── Memory Adapter ────────────────────────────────────────────────────────────

/**
 * In-memory PersistenceAdapter for testing and ephemeral workspaces.
 */
export function createMemoryAdapter(): PersistenceAdapter {
  const store = new Map<string, Uint8Array>();

  return {
    load(path: string): Uint8Array | null {
      return store.get(path) ?? null;
    },

    save(path: string, data: Uint8Array): void {
      store.set(path, data);
    },

    delete(path: string): boolean {
      return store.delete(path);
    },

    exists(path: string): boolean {
      return store.has(path);
    },

    list(directory: string): string[] {
      const prefix = directory.endsWith("/") ? directory : directory + "/";
      const entries: string[] = [];
      for (const key of store.keys()) {
        if (key.startsWith(prefix)) {
          // Return path relative to the directory
          const relative = key.slice(prefix.length);
          // Only direct children (no nested slashes)
          if (!relative.includes("/")) {
            entries.push(relative);
          }
        }
      }
      return entries.sort();
    },
  };
}

// ── Vault Manager ─────────────────────────────────────────────────────────────

export interface VaultManagerOptions {
  /** Loro peer ID for all collection stores created by this manager. */
  peerId?: bigint;
}

export interface VaultManager {
  /** The manifest this manager operates on. */
  readonly manifest: PrismManifest;

  /**
   * Open a collection by its ref ID. Returns a cached CollectionStore,
   * creating and hydrating from disk on first access.
   */
  openCollection(collectionId: string): CollectionStore;

  /**
   * Save a single collection's snapshot to the adapter.
   * No-op if the collection hasn't been opened or isn't dirty.
   */
  saveCollection(collectionId: string): void;

  /** Save all dirty collections. Returns the IDs that were saved. */
  saveAll(): string[];

  /**
   * Close a collection — saves if dirty, then evicts from cache.
   * No-op if the collection isn't open.
   */
  closeCollection(collectionId: string): void;

  /** Check if a collection has unsaved changes. */
  isDirty(collectionId: string): boolean;

  /** List IDs of currently open collections. */
  openCollections(): string[];

  /** Get the underlying adapter. */
  readonly adapter: PersistenceAdapter;
}

/**
 * Compute the storage path for a collection.
 * Uses `data/collections/{collectionId}.loro` by default.
 */
function collectionPath(collectionId: string): string {
  return `data/collections/${collectionId}.loro`;
}

export function createVaultManager(
  manifest: PrismManifest,
  adapter: PersistenceAdapter,
  options?: VaultManagerOptions,
): VaultManager {
  const cache = new Map<string, CollectionStore>();
  const dirty = new Set<string>();

  function resolveRef(collectionId: string): CollectionRef {
    const ref = (manifest.collections ?? []).find(
      (c) => c.id === collectionId,
    );
    if (!ref) {
      throw new Error(
        `Collection '${collectionId}' not found in manifest '${manifest.name}'`,
      );
    }
    return ref;
  }

  function openCollection(collectionId: string): CollectionStore {
    const existing = cache.get(collectionId);
    if (existing) return existing;

    // Validate the collection exists in the manifest
    resolveRef(collectionId);

    const storeOpts: import("./collection-store.js").CollectionStoreOptions = {};
    if (options?.peerId !== undefined) {
      storeOpts.peerId = options.peerId;
    }
    const store = createCollectionStore(storeOpts);
    const path = collectionPath(collectionId);

    // Hydrate from disk if data exists
    const snapshot = adapter.load(path);
    if (snapshot) {
      store.import(snapshot);
    }

    // Track dirty state via change subscription
    store.onChange(() => {
      dirty.add(collectionId);
    });

    cache.set(collectionId, store);
    return store;
  }

  function saveCollection(collectionId: string): void {
    const store = cache.get(collectionId);
    if (!store) return;
    if (!dirty.has(collectionId)) return;

    const snapshot = store.exportSnapshot();
    const path = collectionPath(collectionId);
    adapter.save(path, snapshot);
    dirty.delete(collectionId);
  }

  function saveAll(): string[] {
    const saved: string[] = [];
    for (const id of dirty) {
      const store = cache.get(id);
      if (store) {
        const snapshot = store.exportSnapshot();
        adapter.save(collectionPath(id), snapshot);
        saved.push(id);
      }
    }
    dirty.clear();
    return saved;
  }

  function closeCollection(collectionId: string): void {
    if (!cache.has(collectionId)) return;
    saveCollection(collectionId);
    cache.delete(collectionId);
    dirty.delete(collectionId);
  }

  function isDirty(collectionId: string): boolean {
    return dirty.has(collectionId);
  }

  function openCollections(): string[] {
    return [...cache.keys()];
  }

  return {
    manifest,
    adapter,
    openCollection,
    saveCollection,
    saveAll,
    closeCollection,
    isDirty,
    openCollections,
  };
}
