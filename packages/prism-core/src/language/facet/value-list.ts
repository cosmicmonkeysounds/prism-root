/**
 * ValueList — named sets of allowed values for constrained field input.
 *
 * Inspired by FileMaker Pro's Value Lists: predefined sets of allowed values
 * that appear as dropdowns, radio buttons, or checkboxes. Can be static
 * (hardcoded values) or dynamic (sourced from a relationship/collection).
 *
 * Usage:
 *   // Static value list
 *   const statusList = createStaticValueList('status-list', 'Status', [
 *     { value: 'active', label: 'Active' },
 *     { value: 'inactive', label: 'Inactive' },
 *     { value: 'archived', label: 'Archived' },
 *   ]);
 *
 *   // Dynamic value list (sourced from related records)
 *   const clientList = createDynamicValueList('client-list', 'Clients', {
 *     collectionId: 'contacts',
 *     valueField: 'id',
 *     displayField: 'name',
 *     sortField: 'name',
 *     sortDirection: 'asc',
 *     filter: { field: 'type', op: 'eq', value: 'client' },
 *   });
 *
 *   const registry = createValueListRegistry();
 *   registry.register(statusList);
 *   registry.register(clientList);
 *   const resolved = registry.resolve('client-list', resolver);
 */

import type { FilterConfig } from "@prism/core/view";

// ── Value List Items ────────────────────────────────────────────────────────

export interface ValueListItem {
  /** The stored value. */
  value: string | number | boolean;
  /** Display label (falls back to stringified value if omitted). */
  label?: string;
  /** Whether this item is disabled (shown but not selectable). */
  disabled?: boolean;
  /** Optional icon identifier. */
  icon?: string;
  /** Optional color for visual grouping. */
  color?: string;
}

// ── Value List Sources ──────────────────────────────────────────────────────

/** Static: hardcoded list of items. */
export interface StaticValueListSource {
  kind: "static";
  items: ValueListItem[];
}

/** Dynamic: items sourced from a collection via field lookup. */
export interface DynamicValueListSource {
  kind: "dynamic";
  /** Collection ID to source values from. */
  collectionId: string;
  /** Field path to use as the stored value. */
  valueField: string;
  /** Field path to use as the display label. */
  displayField: string;
  /** Optional sort field. */
  sortField?: string;
  /** Sort direction. Default: 'asc'. */
  sortDirection?: "asc" | "desc";
  /** Optional filter to narrow source records. */
  filter?: FilterConfig;
  /** Include only first N items. */
  limit?: number;
}

export type ValueListSource = StaticValueListSource | DynamicValueListSource;

// ── Value List Definition ───────────────────────────────────────────────────

export type ValueListDisplay = "dropdown" | "radio" | "checkbox" | "combobox";

export interface ValueList {
  /** Unique identifier for this value list. */
  id: string;
  /** Human-readable name. */
  name: string;
  /** Optional description. */
  description?: string;
  /** Data source (static items or dynamic query). */
  source: ValueListSource;
  /** Preferred UI display mode. */
  display?: ValueListDisplay;
  /** Allow values not in the list (custom entry). */
  allowCustom?: boolean;
  /** Show the "none" / empty option. */
  allowEmpty?: boolean;
}

// ── Factories ───────────────────────────────────────────────────────────────

export function createStaticValueList(
  id: string,
  name: string,
  items: ValueListItem[],
): ValueList {
  return {
    id,
    name,
    source: { kind: "static", items: [...items] },
  };
}

export function createDynamicValueList(
  id: string,
  name: string,
  config: Omit<DynamicValueListSource, "kind">,
): ValueList {
  return {
    id,
    name,
    source: { kind: "dynamic", ...config },
  };
}

// ── Resolution ──────────────────────────────────────────────────────────────

/**
 * Adapter interface for resolving dynamic value lists.
 * Implementations query a CollectionStore to produce ValueListItems.
 */
export interface ValueListResolver {
  resolve(source: DynamicValueListSource): ValueListItem[];
}

/**
 * Resolve a value list to its concrete items.
 * Static lists return immediately; dynamic lists use the resolver.
 */
export function resolveValueList(
  list: ValueList,
  resolver?: ValueListResolver,
): ValueListItem[] {
  if (list.source.kind === "static") {
    return [...list.source.items];
  }

  if (!resolver) {
    return [];
  }

  return resolver.resolve(list.source);
}

// ── Registry ────────────────────────────────────────────────────────────────

export type ValueListListener = (lists: ValueList[]) => void;

export interface ValueListRegistry {
  /** Register a value list. Overwrites if ID exists. */
  register(list: ValueList): void;
  /** Remove a value list by ID. Returns true if found. */
  remove(id: string): boolean;
  /** Get a value list by ID. */
  get(id: string): ValueList | undefined;
  /** Get all registered value lists. */
  all(): ValueList[];
  /** Resolve a value list to its concrete items. */
  resolve(id: string, resolver?: ValueListResolver): ValueListItem[];
  /** Search lists by name (case-insensitive substring). */
  search(query: string): ValueList[];
  /** Subscribe to changes. */
  subscribe(listener: ValueListListener): () => void;
  /** Serialize for persistence. */
  serialize(): ValueList[];
  /** Load from persistence. */
  load(lists: ValueList[]): void;
  /** Total count. */
  readonly size: number;
}

export function createValueListRegistry(): ValueListRegistry {
  const lists = new Map<string, ValueList>();
  const listeners = new Set<ValueListListener>();

  function notify(): void {
    const all = [...lists.values()];
    for (const listener of listeners) {
      listener(all);
    }
  }

  return {
    register(list: ValueList): void {
      lists.set(list.id, { ...list });
      notify();
    },

    remove(id: string): boolean {
      const deleted = lists.delete(id);
      if (deleted) notify();
      return deleted;
    },

    get(id: string): ValueList | undefined {
      const list = lists.get(id);
      return list ? { ...list } : undefined;
    },

    all(): ValueList[] {
      return [...lists.values()];
    },

    resolve(id: string, resolver?: ValueListResolver): ValueListItem[] {
      const list = lists.get(id);
      if (!list) return [];
      return resolveValueList(list, resolver);
    },

    search(query: string): ValueList[] {
      const lower = query.toLowerCase();
      return [...lists.values()].filter(
        (l) =>
          l.name.toLowerCase().includes(lower) ||
          (l.description?.toLowerCase().includes(lower) ?? false),
      );
    },

    subscribe(listener: ValueListListener): () => void {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },

    serialize(): ValueList[] {
      return [...lists.values()].map((l) => ({ ...l }));
    },

    load(data: ValueList[]): void {
      lists.clear();
      for (const list of data) {
        lists.set(list.id, { ...list });
      }
      notify();
    },

    get size(): number {
      return lists.size;
    },
  };
}
