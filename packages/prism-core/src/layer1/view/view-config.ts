/**
 * ViewConfig — filter, sort, and group configuration for derived views.
 *
 * Pure functions that transform a GraphObject[] according to a ViewConfig.
 * No side effects, no subscriptions — LiveView composes these with reactivity.
 *
 * Ported from legacy @core/ui/view filter/sort/group model.
 */

import type { GraphObject } from "../object-model/index.js";

// ── Filter ───────────────────────────────────────────────────────────────────

export type FilterOp =
  | "eq"
  | "neq"
  | "contains"
  | "starts"
  | "gt"
  | "gte"
  | "lt"
  | "lte"
  | "in"
  | "nin"
  | "empty"
  | "notempty";

export interface FilterConfig {
  field: string;
  op: FilterOp;
  /** Value to compare against. Ignored for 'empty' and 'notempty'. */
  value?: unknown;
}

// ── Sort ─────────────────────────────────────────────────────────────────────

export interface SortConfig {
  field: string;
  dir: "asc" | "desc";
}

// ── Group ────────────────────────────────────────────────────────────────────

export interface GroupConfig {
  field: string;
  /** Whether this group starts collapsed in the UI. Default: false. */
  collapsed?: boolean;
}

export interface GroupedResult {
  key: string;
  label: string;
  objects: GraphObject[];
  collapsed: boolean;
}

// ── ViewConfig ───────────────────────────────────────────────────────────────

export interface ViewConfig {
  filters?: FilterConfig[] | undefined;
  sorts?: SortConfig[] | undefined;
  groups?: GroupConfig[] | undefined;
  /** Visible column field IDs (for table mode). */
  columns?: string[] | undefined;
  /** Max objects to include. */
  limit?: number | undefined;
  /** Exclude soft-deleted objects. Default: true. */
  excludeDeleted?: boolean | undefined;
}

// ── Field access ─────────────────────────────────────────────────────────────

/**
 * Resolve a field value from a GraphObject.
 * Checks shell fields first, then data payload.
 */
export function getFieldValue(obj: GraphObject, field: string): unknown {
  // Shell fields
  if (field in obj) {
    return (obj as unknown as Record<string, unknown>)[field];
  }
  // Data payload
  return obj.data[field];
}

// ── Filter application ───────────────────────────────────────────────────────

function matchesFilter(obj: GraphObject, filter: FilterConfig): boolean {
  const actual = getFieldValue(obj, filter.field);
  const expected = filter.value;

  switch (filter.op) {
    case "empty":
      return actual === null || actual === undefined || actual === "" ||
        (Array.isArray(actual) && actual.length === 0);

    case "notempty":
      return actual !== null && actual !== undefined && actual !== "" &&
        !(Array.isArray(actual) && actual.length === 0);

    case "eq":
      return actual === expected;

    case "neq":
      return actual !== expected;

    case "contains": {
      if (typeof actual === "string" && typeof expected === "string") {
        return actual.toLowerCase().includes(expected.toLowerCase());
      }
      if (Array.isArray(actual)) {
        return actual.includes(expected);
      }
      return false;
    }

    case "starts": {
      if (typeof actual === "string" && typeof expected === "string") {
        return actual.toLowerCase().startsWith(expected.toLowerCase());
      }
      return false;
    }

    case "gt":
      return actual !== null && actual !== undefined && expected !== null && expected !== undefined &&
        (actual as number) > (expected as number);

    case "gte":
      return actual !== null && actual !== undefined && expected !== null && expected !== undefined &&
        (actual as number) >= (expected as number);

    case "lt":
      return actual !== null && actual !== undefined && expected !== null && expected !== undefined &&
        (actual as number) < (expected as number);

    case "lte":
      return actual !== null && actual !== undefined && expected !== null && expected !== undefined &&
        (actual as number) <= (expected as number);

    case "in": {
      if (Array.isArray(expected)) {
        return expected.includes(actual);
      }
      return false;
    }

    case "nin": {
      if (Array.isArray(expected)) {
        return !expected.includes(actual);
      }
      return true;
    }

    default:
      return true;
  }
}

/**
 * Apply filters to an array of objects. All filters are AND-combined.
 */
export function applyFilters(
  objects: GraphObject[],
  filters: FilterConfig[],
): GraphObject[] {
  if (filters.length === 0) return objects;
  return objects.filter((obj) => filters.every((f) => matchesFilter(obj, f)));
}

// ── Sort application ─────────────────────────────────────────────────────────

/**
 * Apply sort configs to an array of objects. Earlier sorts take priority.
 * Returns a new sorted array (does not mutate).
 */
export function applySorts(
  objects: GraphObject[],
  sorts: SortConfig[],
): GraphObject[] {
  if (sorts.length === 0) return objects;
  return [...objects].sort((a, b) => {
    for (const sort of sorts) {
      const av = getFieldValue(a, sort.field) ?? "";
      const bv = getFieldValue(b, sort.field) ?? "";
      const dir = sort.dir === "desc" ? -1 : 1;
      if (av < bv) return -dir;
      if (av > bv) return dir;
    }
    return 0;
  });
}

// ── Group application ────────────────────────────────────────────────────────

/**
 * Group objects by a field. Returns groups in order of first occurrence.
 */
export function applyGroups(
  objects: GraphObject[],
  groups: GroupConfig[],
): GroupedResult[] {
  if (groups.length === 0) {
    return [{ key: "__all__", label: "All", objects, collapsed: false }];
  }

  // Use the first group config (single-level grouping)
  const group = groups[0] as GroupConfig;
  const groupMap = new Map<string, GraphObject[]>();
  const groupOrder: string[] = [];

  for (const obj of objects) {
    const raw = getFieldValue(obj, group.field);
    const key = raw === null || raw === undefined ? "__none__" : String(raw);

    let bucket = groupMap.get(key);
    if (!bucket) {
      bucket = [];
      groupMap.set(key, bucket);
      groupOrder.push(key);
    }
    bucket.push(obj);
  }

  return groupOrder.map((key) => ({
    key,
    label: key === "__none__" ? "None" : key,
    objects: groupMap.get(key) ?? [],
    collapsed: group.collapsed ?? false,
  }));
}

// ── Full pipeline ────────────────────────────────────────────────────────────

/**
 * Apply a full ViewConfig pipeline: delete filter → filters → sorts → limit.
 * Returns the transformed objects array.
 */
export function applyViewConfig(
  objects: GraphObject[],
  config: ViewConfig,
): GraphObject[] {
  let result = objects;

  // Exclude deleted
  if (config.excludeDeleted !== false) {
    result = result.filter((o) => !o.deletedAt);
  }

  // Filters
  if (config.filters && config.filters.length > 0) {
    result = applyFilters(result, config.filters);
  }

  // Sorts
  if (config.sorts && config.sorts.length > 0) {
    result = applySorts(result, config.sorts);
  }

  // Limit
  if (config.limit !== undefined) {
    result = result.slice(0, config.limit);
  }

  return result;
}
