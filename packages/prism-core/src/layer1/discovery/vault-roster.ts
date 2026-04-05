/**
 * VaultRoster — persistent registry of known vaults/manifests.
 *
 * Tracks every vault the user has opened, created, or discovered.
 * Persisted via a pluggable RosterStore (same pattern as ConfigStore).
 *
 * Responsibilities:
 *   - CRUD for roster entries (add, remove, update, get)
 *   - Sort by recency (lastOpenedAt), name, or createdAt
 *   - Pin/unpin entries for quick access
 *   - Touch (bump lastOpenedAt) on workspace open
 *   - Deduplication by vault path
 *   - Subscribe to roster changes
 *   - Serialization for persistence
 */

// ── Types ─────────────────────────────────────────────────────────────────────

/** A single entry in the vault roster. */
export interface RosterEntry {
  /** Manifest ID (from PrismManifest.id). */
  id: string;
  /** Display name (from PrismManifest.name). */
  name: string;
  /** Absolute path to the vault directory containing the manifest. */
  path: string;
  /** ISO timestamp when first added to the roster. */
  addedAt: string;
  /** ISO timestamp when last opened. Used for recency sort. */
  lastOpenedAt: string;
  /** Pinned entries appear at the top of the recent list. */
  pinned: boolean;
  /** Optional description (from PrismManifest.description). */
  description?: string;
  /** Number of collections in the manifest. */
  collectionCount?: number;
  /** Visibility from manifest. */
  visibility?: "private" | "team" | "public";
  /** Tags for user-defined categorization. */
  tags?: string[];
}

export type RosterSortField = "lastOpenedAt" | "name" | "addedAt";
export type RosterSortDir = "asc" | "desc";

export interface RosterListOptions {
  /** Sort field. Default: 'lastOpenedAt'. */
  sortBy?: RosterSortField;
  /** Sort direction. Default: 'desc' (most recent first). */
  sortDir?: RosterSortDir;
  /** Filter by pinned status. */
  pinned?: boolean;
  /** Filter by tag (entry must have ALL listed tags). */
  tags?: string[];
  /** Text search in name and description. */
  search?: string;
  /** Max entries to return. */
  limit?: number;
}

export type RosterChangeType = "add" | "remove" | "update";

export interface RosterChange {
  type: RosterChangeType;
  entry: RosterEntry;
}

export type RosterChangeHandler = (changes: RosterChange[]) => void;

/**
 * Pluggable persistence for the roster.
 * Synchronous — the roster is a small JSON file.
 */
export interface RosterStore {
  load(): RosterEntry[];
  save(entries: RosterEntry[]): void;
}

// ── In-memory RosterStore ─────────────────────────────────────────────────────

export function createMemoryRosterStore(): RosterStore {
  let data: RosterEntry[] = [];
  return {
    load(): RosterEntry[] {
      return structuredClone(data);
    },
    save(entries: RosterEntry[]): void {
      data = structuredClone(entries);
    },
  };
}

// ── VaultRoster ──────────────────────────────────────────────────────────────

export interface VaultRoster {
  /** Add or update an entry. Deduplicates by path. Returns the entry. */
  add(entry: Omit<RosterEntry, "addedAt"> & { addedAt?: string }): RosterEntry;

  /** Remove an entry by ID. Returns true if it existed. */
  remove(id: string): boolean;

  /** Get an entry by ID. */
  get(id: string): RosterEntry | undefined;

  /** Get an entry by vault path. */
  getByPath(path: string): RosterEntry | undefined;

  /** Update fields on an existing entry. Returns the updated entry or undefined. */
  update(id: string, patch: Partial<Omit<RosterEntry, "id">>): RosterEntry | undefined;

  /** Bump lastOpenedAt to now. Returns the updated entry. */
  touch(id: string): RosterEntry | undefined;

  /** Pin or unpin an entry. */
  pin(id: string, pinned: boolean): RosterEntry | undefined;

  /** List entries with optional filtering and sorting. */
  list(options?: RosterListOptions): RosterEntry[];

  /** Total number of entries. */
  size(): number;

  /** Subscribe to roster changes. Returns unsubscribe function. */
  onChange(handler: RosterChangeHandler): () => void;

  /** Persist current state to the RosterStore. */
  save(): void;

  /** Reload from the RosterStore. */
  reload(): void;

  /** Get all entries as a plain array. */
  all(): RosterEntry[];
}

export function createVaultRoster(store?: RosterStore): VaultRoster {
  const entries = new Map<string, RosterEntry>();
  const pathIndex = new Map<string, string>(); // path → id
  const listeners = new Set<RosterChangeHandler>();
  const backingStore = store;

  // Hydrate from store if provided
  if (backingStore) {
    for (const entry of backingStore.load()) {
      entries.set(entry.id, entry);
      pathIndex.set(entry.path, entry.id);
    }
  }

  function notify(changes: RosterChange[]): void {
    for (const handler of listeners) {
      handler(changes);
    }
  }

  function add(
    input: Omit<RosterEntry, "addedAt"> & { addedAt?: string },
  ): RosterEntry {
    // Dedup by path — if same path exists, update instead
    const existingId = pathIndex.get(input.path);
    if (existingId && existingId !== input.id) {
      // Remove old entry at this path
      const old = entries.get(existingId);
      entries.delete(existingId);
      pathIndex.delete(input.path);
      if (old) {
        notify([{ type: "remove", entry: old }]);
      }
    }

    const now = new Date().toISOString();
    const entry: RosterEntry = {
      ...input,
      addedAt: input.addedAt ?? now,
    };

    const isUpdate = entries.has(entry.id);
    entries.set(entry.id, entry);
    pathIndex.set(entry.path, entry.id);

    notify([{ type: isUpdate ? "update" : "add", entry }]);
    return entry;
  }

  function remove(id: string): boolean {
    const entry = entries.get(id);
    if (!entry) return false;
    entries.delete(id);
    pathIndex.delete(entry.path);
    notify([{ type: "remove", entry }]);
    return true;
  }

  function get(id: string): RosterEntry | undefined {
    return entries.get(id);
  }

  function getByPath(path: string): RosterEntry | undefined {
    const id = pathIndex.get(path);
    if (!id) return undefined;
    return entries.get(id);
  }

  function update(
    id: string,
    patch: Partial<Omit<RosterEntry, "id">>,
  ): RosterEntry | undefined {
    const existing = entries.get(id);
    if (!existing) return undefined;

    // If path changes, update path index
    if (patch.path && patch.path !== existing.path) {
      pathIndex.delete(existing.path);
      pathIndex.set(patch.path, id);
    }

    const updated: RosterEntry = { ...existing, ...patch, id };
    entries.set(id, updated);
    notify([{ type: "update", entry: updated }]);
    return updated;
  }

  function touch(id: string): RosterEntry | undefined {
    return update(id, { lastOpenedAt: new Date().toISOString() });
  }

  function pin(id: string, pinned: boolean): RosterEntry | undefined {
    return update(id, { pinned });
  }

  function list(options?: RosterListOptions): RosterEntry[] {
    let result = [...entries.values()];

    // Filters
    if (options?.pinned !== undefined) {
      result = result.filter((e) => e.pinned === options.pinned);
    }
    if (options?.tags && options.tags.length > 0) {
      const tags = options.tags;
      result = result.filter(
        (e) => e.tags && tags.every((t) => e.tags?.includes(t)),
      );
    }
    if (options?.search) {
      const needle = options.search.toLowerCase();
      result = result.filter(
        (e) =>
          e.name.toLowerCase().includes(needle) ||
          (e.description ?? "").toLowerCase().includes(needle),
      );
    }

    // Sort — pinned always float to top within the sort
    const sortBy = options?.sortBy ?? "lastOpenedAt";
    const sortDir = options?.sortDir ?? "desc";
    const dir = sortDir === "desc" ? -1 : 1;

    result.sort((a, b) => {
      // Pinned entries first
      if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;

      const av = a[sortBy] ?? "";
      const bv = b[sortBy] ?? "";
      if (av < bv) return -dir;
      if (av > bv) return dir;
      return 0;
    });

    if (options?.limit !== undefined) {
      result = result.slice(0, options.limit);
    }

    return result;
  }

  function save(): void {
    if (backingStore) {
      backingStore.save([...entries.values()]);
    }
  }

  function reload(): void {
    if (!backingStore) return;
    entries.clear();
    pathIndex.clear();
    for (const entry of backingStore.load()) {
      entries.set(entry.id, entry);
      pathIndex.set(entry.path, entry.id);
    }
  }

  return {
    add,
    remove,
    get,
    getByPath,
    update,
    touch,
    pin,
    list,
    size: () => entries.size,
    onChange(handler: RosterChangeHandler): () => void {
      listeners.add(handler);
      return () => {
        listeners.delete(handler);
      };
    },
    save,
    reload,
    all: () => [...entries.values()],
  };
}
