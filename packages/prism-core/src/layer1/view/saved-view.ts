/**
 * SavedView — persistable named view configurations (FileMaker "Found Sets").
 *
 * A SavedView wraps a ViewConfig with identity, metadata, and sharing info.
 * Views can be saved to Loro for persistence across sessions, shared between
 * users, and pinned as favorites. This is the Prism equivalent of FileMaker's
 * "Found Set" concept — a named, saveable, shareable filtered projection.
 *
 * Usage:
 *   const view = createSavedView('active-contacts', 'contact', {
 *     filters: [{ field: 'status', op: 'eq', value: 'active' }],
 *     sorts: [{ field: 'name', dir: 'asc' }],
 *   });
 *
 *   const registry = createSavedViewRegistry();
 *   registry.add(view);
 *   registry.pin(view.id);
 *   const contactViews = registry.forObjectType('contact');
 */

import type { ViewConfig } from "./view-config.js";
import type { ViewMode } from "./view-def.js";

// ── SavedView ───────────────────────────────────────────────────────────────

export interface SavedView {
  /** Stable UUID for this view. */
  id: string;
  /** Human-readable name (e.g. "Active Contacts", "Overdue Invoices"). */
  name: string;
  /** Optional description. */
  description?: string;
  /** The entity type this view applies to. */
  objectType: string;
  /** View mode (list, table, kanban, etc.). */
  mode: ViewMode;
  /** The filter/sort/group configuration. */
  config: ViewConfig;
  /** Icon identifier for UI display. */
  icon?: string;
  /** Whether this view is pinned as a favorite. */
  pinned: boolean;
  /** Whether this view is shared (visible to other users). */
  shared: boolean;
  /** DID of the user who created this view. */
  ownerId?: string;
  /** ISO timestamp when this view was created. */
  createdAt: string;
  /** ISO timestamp when this view was last modified. */
  updatedAt: string;
  /** Color label for visual grouping. */
  color?: string;
}

// ── Factory ─────────────────────────────────────────────────────────────────

export function createSavedView(
  id: string,
  objectType: string,
  config: ViewConfig,
  name?: string,
): SavedView {
  const now = new Date().toISOString();
  return {
    id,
    name: name ?? id,
    objectType,
    mode: "list",
    config,
    pinned: false,
    shared: false,
    createdAt: now,
    updatedAt: now,
  };
}

// ── Registry ────────────────────────────────────────────────────────────────

export type SavedViewListener = (views: SavedView[]) => void;

export interface SavedViewRegistry {
  /** Add a new saved view. Throws if ID already exists. */
  add(view: SavedView): void;
  /** Update an existing saved view. Throws if not found. */
  update(id: string, patch: Partial<Omit<SavedView, "id" | "createdAt">>): void;
  /** Remove a saved view by ID. Returns true if found. */
  remove(id: string): boolean;
  /** Get a saved view by ID. */
  get(id: string): SavedView | undefined;
  /** Get all saved views. */
  all(): SavedView[];
  /** Get all views for a specific object type. */
  forObjectType(objectType: string): SavedView[];
  /** Get all pinned views. */
  pinned(): SavedView[];
  /** Toggle pin state on a view. */
  pin(id: string): void;
  /** Search views by name (case-insensitive substring match). */
  search(query: string): SavedView[];
  /** Subscribe to changes. Returns unsubscribe function. */
  subscribe(listener: SavedViewListener): () => void;
  /** Serialize all views for Loro persistence. */
  serialize(): SavedView[];
  /** Load views from Loro persistence (replaces all). */
  load(views: SavedView[]): void;
  /** Total view count. */
  readonly size: number;
}

export function createSavedViewRegistry(): SavedViewRegistry {
  const views = new Map<string, SavedView>();
  const listeners = new Set<SavedViewListener>();

  function notify(): void {
    const all = [...views.values()];
    for (const listener of listeners) {
      listener(all);
    }
  }

  return {
    add(view: SavedView): void {
      if (views.has(view.id)) {
        throw new Error(`SavedView with id "${view.id}" already exists`);
      }
      views.set(view.id, { ...view });
      notify();
    },

    update(id: string, patch: Partial<Omit<SavedView, "id" | "createdAt">>): void {
      const existing = views.get(id);
      if (!existing) {
        throw new Error(`SavedView with id "${id}" not found`);
      }
      const updated: SavedView = {
        ...existing,
        ...patch,
        id: existing.id,
        createdAt: existing.createdAt,
        updatedAt: new Date().toISOString(),
      };
      if (patch.config) {
        updated.config = { ...patch.config };
      }
      views.set(id, updated);
      notify();
    },

    remove(id: string): boolean {
      const deleted = views.delete(id);
      if (deleted) notify();
      return deleted;
    },

    get(id: string): SavedView | undefined {
      const view = views.get(id);
      return view ? { ...view } : undefined;
    },

    all(): SavedView[] {
      return [...views.values()];
    },

    forObjectType(objectType: string): SavedView[] {
      return [...views.values()].filter((v) => v.objectType === objectType);
    },

    pinned(): SavedView[] {
      return [...views.values()].filter((v) => v.pinned);
    },

    pin(id: string): void {
      const view = views.get(id);
      if (!view) {
        throw new Error(`SavedView with id "${id}" not found`);
      }
      view.pinned = !view.pinned;
      view.updatedAt = new Date().toISOString();
      notify();
    },

    search(query: string): SavedView[] {
      const lower = query.toLowerCase();
      return [...views.values()].filter(
        (v) =>
          v.name.toLowerCase().includes(lower) ||
          (v.description?.toLowerCase().includes(lower) ?? false),
      );
    },

    subscribe(listener: SavedViewListener): () => void {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },

    serialize(): SavedView[] {
      return [...views.values()].map((v) => ({ ...v }));
    },

    load(data: SavedView[]): void {
      views.clear();
      for (const view of data) {
        views.set(view.id, { ...view });
      }
      notify();
    },

    get size(): number {
      return views.size;
    },
  };
}
