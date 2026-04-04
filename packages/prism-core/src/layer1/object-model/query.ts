/**
 * ObjectQuery — typed query descriptor for filtering and sorting objects.
 *
 * Used by:
 *   - In-memory filtering against the Loro CRDT store
 *   - Relay REST endpoints (serialized into URL query params)
 *   - Tauri IPC commands (passed as structured args)
 *
 * The same query type travels through the whole stack.
 */

import type { GraphObject } from "./types.js";

export interface ObjectQuery {
  type?: string | string[];
  parentId?: string | null;
  status?: string | string[];
  tags?: string[];
  dateAfter?: string;
  dateBefore?: string;
  search?: string;
  pinned?: boolean;
  limit?: number;
  offset?: number;
  sortBy?: "name" | "date" | "createdAt" | "updatedAt" | "position" | "status";
  sortDir?: "asc" | "desc";
  includeDeleted?: boolean;
}

/**
 * Serialize an ObjectQuery to URL query params.
 * Arrays become repeated params: type=task&type=note
 */
export function queryToParams(q: ObjectQuery): URLSearchParams {
  const p = new URLSearchParams();
  for (const [key, val] of Object.entries(q)) {
    if (val === undefined) continue;
    if (val === null) {
      p.set(key, "null");
    } else if (Array.isArray(val)) {
      for (const v of val) p.append(key, String(v));
    } else {
      p.set(key, String(val));
    }
  }
  return p;
}

/**
 * Deserialize URL query params back to an ObjectQuery.
 */
export function paramsToQuery(params: URLSearchParams): ObjectQuery {
  const q: ObjectQuery = {};
  const types = params.getAll("type");
  if (types.length === 1) q.type = types[0];
  else if (types.length > 1) q.type = types;

  const statuses = params.getAll("status");
  if (statuses.length === 1) q.status = statuses[0];
  else if (statuses.length > 1) q.status = statuses;

  const tags = params.getAll("tags");
  if (tags.length > 0) q.tags = tags;

  if (params.has("parentId")) {
    const pid = params.get("parentId")!;
    q.parentId = pid === "null" ? null : pid;
  }
  if (params.has("search")) q.search = params.get("search")!;
  if (params.has("dateAfter")) q.dateAfter = params.get("dateAfter")!;
  if (params.has("dateBefore")) q.dateBefore = params.get("dateBefore")!;
  if (params.has("pinned")) q.pinned = params.get("pinned") === "true";
  if (params.has("limit")) q.limit = Number(params.get("limit"));
  if (params.has("offset")) q.offset = Number(params.get("offset"));
  if (params.has("sortBy"))
    q.sortBy = params.get("sortBy") as ObjectQuery["sortBy"];
  if (params.has("sortDir"))
    q.sortDir = params.get("sortDir") as "asc" | "desc";
  if (params.has("includeDeleted"))
    q.includeDeleted = params.get("includeDeleted") === "true";
  return q;
}

/**
 * Test whether a GraphObject matches an ObjectQuery.
 */
export function matchesQuery(obj: GraphObject, q: ObjectQuery): boolean {
  if (!q.includeDeleted && obj.deletedAt) return false;

  if (q.type !== undefined) {
    const types = Array.isArray(q.type) ? q.type : [q.type];
    if (!types.includes(obj.type)) return false;
  }
  if (q.parentId !== undefined) {
    if (q.parentId === null && obj.parentId !== null) return false;
    if (q.parentId !== null && obj.parentId !== q.parentId) return false;
  }
  if (q.status !== undefined) {
    const statuses = Array.isArray(q.status) ? q.status : [q.status];
    if (!obj.status || !statuses.includes(obj.status)) return false;
  }
  if (q.tags && q.tags.length > 0) {
    if (!q.tags.every((t) => obj.tags.includes(t))) return false;
  }
  if (q.pinned !== undefined && obj.pinned !== q.pinned) return false;
  if (q.dateAfter && obj.date && obj.date < q.dateAfter) return false;
  if (q.dateBefore && obj.date && obj.date > q.dateBefore) return false;
  if (q.search) {
    const needle = q.search.toLowerCase();
    if (
      !obj.name.toLowerCase().includes(needle) &&
      !obj.description.toLowerCase().includes(needle)
    )
      return false;
  }
  return true;
}

/** Sort an array of objects according to query sort options (mutates in place). */
export function sortObjects(
  objects: GraphObject[],
  q: ObjectQuery,
): GraphObject[] {
  const field = q.sortBy ?? "position";
  const dir = q.sortDir === "desc" ? -1 : 1;
  return objects.sort((a, b) => {
    const av = (a as unknown as Record<string, unknown>)[field] ?? "";
    const bv = (b as unknown as Record<string, unknown>)[field] ?? "";
    if (av < bv) return -dir;
    if (av > bv) return dir;
    return 0;
  });
}
