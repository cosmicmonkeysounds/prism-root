/**
 * LiveView — materialized, auto-updating projection of a CollectionStore.
 *
 * Wraps a CollectionStore + ViewConfig to produce a live result set that
 * re-materializes whenever the source data or configuration changes.
 *
 * Provides:
 *   - Materialized objects (filtered, sorted, limited)
 *   - Grouped results (when GroupConfig is set)
 *   - Faceted counts (types, tags)
 *   - Reactive subscriptions for UI binding
 *   - Config mutation methods (setFilters, setSorts, setGroups, etc.)
 */

import type { GraphObject, ObjectId } from "../object-model/index.js";
import type { CollectionStore } from "../persistence/collection-store.js";
import type {
  ViewConfig,
  FilterConfig,
  SortConfig,
  GroupConfig,
  GroupedResult,
} from "./view-config.js";
import { applyViewConfig, applyGroups } from "./view-config.js";
import type { ViewMode } from "./view-def.js";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface LiveViewSnapshot {
  /** Filtered, sorted, limited objects. */
  objects: GraphObject[];
  /** Grouped results (single group "All" when no GroupConfig). */
  groups: GroupedResult[];
  /** Total matching objects before limit. */
  total: number;
  /** Count by object type. */
  typeFacets: Record<string, number>;
  /** Count by tag. */
  tagFacets: Record<string, number>;
}

export type LiveViewListener = (snapshot: LiveViewSnapshot) => void;

export interface LiveViewOptions {
  /** Initial view mode. Default: 'list'. */
  mode?: ViewMode;
  /** Initial view config. */
  config?: ViewConfig;
}

// ── LiveView ─────────────────────────────────────────────────────────────────

export interface LiveView {
  /** Current materialized snapshot. */
  readonly snapshot: LiveViewSnapshot;

  /** Current view mode. */
  readonly mode: ViewMode;

  /** Current view config. */
  readonly config: ViewConfig;

  /** The source CollectionStore. */
  readonly store: CollectionStore;

  // ── Config mutations ──────────────────────────────────────────────────────

  /** Replace the entire config and re-materialize. */
  setConfig(config: ViewConfig): void;

  /** Replace filters. */
  setFilters(filters: FilterConfig[]): void;

  /** Replace sorts. */
  setSorts(sorts: SortConfig[]): void;

  /** Replace groups. */
  setGroups(groups: GroupConfig[]): void;

  /** Set visible columns (table mode). */
  setColumns(columns: string[]): void;

  /** Set result limit. */
  setLimit(limit: number | undefined): void;

  /** Change view mode. */
  setMode(mode: ViewMode): void;

  /** Toggle a group's collapsed state. */
  toggleGroupCollapsed(groupKey: string): void;

  // ── Object lookup ──────────────────────────────────────────────────────────

  /** Check if an object is in the current materialized set. */
  includes(objectId: ObjectId): boolean;

  // ── Subscription ──────────────────────────────────────────────────────────

  /** Subscribe to snapshot changes. Called immediately with current state. */
  subscribe(listener: LiveViewListener): () => void;

  /** Force re-materialization from the store. */
  refresh(): void;

  /** Detach from the CollectionStore (stop auto-updates). */
  dispose(): void;
}

export function createLiveView(
  store: CollectionStore,
  options?: LiveViewOptions,
): LiveView {
  let mode: ViewMode = options?.mode ?? "list";
  let config: ViewConfig = options?.config ?? {};
  let disposed = false;

  const listeners = new Set<LiveViewListener>();

  // Track group collapse state separately (not in ViewConfig)
  const collapsedGroups = new Set<string>();

  // Initialize collapsed state from config
  if (config.groups) {
    for (const g of config.groups) {
      if (g.collapsed) collapsedGroups.add(g.field);
    }
  }

  let currentSnapshot: LiveViewSnapshot = materialize();

  function materialize(): LiveViewSnapshot {
    const allObjects = store.allObjects();

    // Apply config pipeline (without limit for total count)
    const configWithoutLimit = { ...config, limit: undefined };
    const filtered = applyViewConfig(allObjects, configWithoutLimit);
    const total = filtered.length;

    // Apply limit
    const limited = config.limit !== undefined
      ? filtered.slice(0, config.limit)
      : filtered;

    // Compute groups
    const groups = applyGroups(limited, config.groups ?? []);

    // Apply collapse state
    for (const group of groups) {
      if (collapsedGroups.has(group.key)) {
        group.collapsed = true;
      }
    }

    // Compute facets from filtered set (before limit)
    const typeFacets: Record<string, number> = {};
    const tagFacets: Record<string, number> = {};
    for (const obj of filtered) {
      typeFacets[obj.type] = (typeFacets[obj.type] ?? 0) + 1;
      for (const tag of obj.tags) {
        tagFacets[tag] = (tagFacets[tag] ?? 0) + 1;
      }
    }

    return { objects: limited, groups, total, typeFacets, tagFacets };
  }

  function notify(): void {
    currentSnapshot = materialize();
    for (const listener of listeners) {
      listener(currentSnapshot);
    }
  }

  // Subscribe to store changes for auto-update
  const unsubStore = store.onChange(() => {
    if (!disposed) notify();
  });

  // Build the object ID set for fast `includes` lookups
  function buildIdSet(): Set<string> {
    return new Set(currentSnapshot.objects.map((o) => o.id));
  }
  let idSet = buildIdSet();

  function refreshIdSet(): void {
    idSet = buildIdSet();
  }

  return {
    get snapshot() {
      return currentSnapshot;
    },
    get mode() {
      return mode;
    },
    get config() {
      return config;
    },
    get store() {
      return store;
    },

    setConfig(newConfig: ViewConfig): void {
      config = newConfig;
      // Sync collapsed state
      collapsedGroups.clear();
      if (newConfig.groups) {
        for (const g of newConfig.groups) {
          if (g.collapsed) collapsedGroups.add(g.field);
        }
      }
      notify();
      refreshIdSet();
    },

    setFilters(filters: FilterConfig[]): void {
      config = { ...config, filters };
      notify();
      refreshIdSet();
    },

    setSorts(sorts: SortConfig[]): void {
      config = { ...config, sorts };
      notify();
      refreshIdSet();
    },

    setGroups(groups: GroupConfig[]): void {
      config = { ...config, groups };
      collapsedGroups.clear();
      for (const g of groups) {
        if (g.collapsed) collapsedGroups.add(g.field);
      }
      notify();
      refreshIdSet();
    },

    setColumns(columns: string[]): void {
      config = { ...config, columns };
      // Columns don't affect materialized data, but notify for UI
      notify();
    },

    setLimit(limit: number | undefined): void {
      config = { ...config, limit };
      notify();
      refreshIdSet();
    },

    setMode(newMode: ViewMode): void {
      mode = newMode;
      // Mode change doesn't affect data, but notify for UI
      notify();
    },

    toggleGroupCollapsed(groupKey: string): void {
      if (collapsedGroups.has(groupKey)) {
        collapsedGroups.delete(groupKey);
      } else {
        collapsedGroups.add(groupKey);
      }
      // Re-apply collapse state
      for (const group of currentSnapshot.groups) {
        group.collapsed = collapsedGroups.has(group.key);
      }
      // Notify without full re-materialize (just collapse state changed)
      for (const listener of listeners) {
        listener(currentSnapshot);
      }
    },

    includes(objectId: ObjectId): boolean {
      return idSet.has(objectId);
    },

    subscribe(listener: LiveViewListener): () => void {
      listeners.add(listener);
      listener(currentSnapshot);
      return () => {
        listeners.delete(listener);
      };
    },

    refresh(): void {
      notify();
      refreshIdSet();
    },

    dispose(): void {
      disposed = true;
      unsubStore();
      listeners.clear();
    },
  };
}
