/**
 * Pure helpers for the Behavior panel.
 *
 * Keeps all CRUD shape logic + validation plumbing out of the React panel so
 * vitest can exercise the contract without booting CodeMirror or the WASM
 * Luau parser. The panel pulls in `validateLuau` itself; these helpers only
 * manipulate plain data.
 */

import type { GraphObject, ObjectId } from "@prism/core/object-model";

export type BehaviorTrigger =
  | "onClick"
  | "onMount"
  | "onChange"
  | "onRouteEnter"
  | "onRouteLeave";

export interface BehaviorDraft {
  trigger: BehaviorTrigger;
  source: string;
  enabled: boolean;
  targetObjectId: string;
}

export interface BehaviorRow {
  id: string;
  name: string;
  trigger: BehaviorTrigger;
  source: string;
  enabled: boolean;
  targetObjectId: string;
}

const DEFAULT_TRIGGER: BehaviorTrigger = "onClick";

const TRIGGER_VALUES: ReadonlySet<BehaviorTrigger> = new Set<BehaviorTrigger>([
  "onClick",
  "onMount",
  "onChange",
  "onRouteEnter",
  "onRouteLeave",
]);

function asTrigger(v: unknown): BehaviorTrigger {
  return typeof v === "string" && (TRIGGER_VALUES as Set<string>).has(v)
    ? (v as BehaviorTrigger)
    : DEFAULT_TRIGGER;
}

/**
 * Project every `behavior` GraphObject targeting `targetObjectId` (walking
 * the data dict) into a friendly editable row. Sorts by creation time so the
 * oldest binding is shown first.
 */
export function listBehaviorsFor(
  targetObjectId: string,
  objects: ReadonlyArray<GraphObject>,
): BehaviorRow[] {
  const out: BehaviorRow[] = [];
  for (const o of objects) {
    if (o.type !== "behavior" || o.deletedAt) continue;
    const data = o.data as Record<string, unknown>;
    if (data["targetObjectId"] !== targetObjectId) continue;
    out.push({
      id: o.id as unknown as string,
      name: o.name,
      trigger: asTrigger(data["trigger"]),
      source: typeof data["source"] === "string" ? (data["source"] as string) : "",
      enabled: data["enabled"] !== false,
      targetObjectId,
    });
  }
  return out.sort((a, b) => a.id.localeCompare(b.id));
}

/**
 * Build the `createObject` input for a fresh behavior attached to a target.
 * The panel passes the result straight through `kernel.createObject`.
 */
export function newBehaviorDraft(
  targetObjectId: string,
  parentId: string,
  trigger: BehaviorTrigger = DEFAULT_TRIGGER,
): {
  type: "behavior";
  name: string;
  parentId: string;
  position: number;
  data: BehaviorDraft;
} {
  return {
    type: "behavior",
    name: `${trigger} behavior`,
    parentId,
    position: 0,
    data: {
      trigger,
      source: "",
      enabled: true,
      targetObjectId,
    },
  };
}

/**
 * Merge an edit into an existing behavior object's `data` bag, preserving
 * unrelated keys. Caller passes the result to `kernel.updateObject`.
 */
export function mergeBehaviorEdit(
  existing: GraphObject,
  patch: Partial<BehaviorDraft>,
): { data: Record<string, unknown> } {
  const base = (existing.data ?? {}) as Record<string, unknown>;
  const next: Record<string, unknown> = { ...base };
  if (patch.trigger !== undefined) next["trigger"] = patch.trigger;
  if (patch.source !== undefined) next["source"] = patch.source;
  if (patch.enabled !== undefined) next["enabled"] = patch.enabled;
  if (patch.targetObjectId !== undefined) next["targetObjectId"] = patch.targetObjectId;
  return { data: next };
}

/** Human-readable summary of a row for the list item subhead. */
export function summariseBehavior(row: BehaviorRow): string {
  const firstLine = row.source.split("\n").find((l) => l.trim().length > 0) ?? "";
  const trimmed = firstLine.length > 48 ? firstLine.slice(0, 45) + "…" : firstLine;
  const suffix = row.enabled ? "" : " (disabled)";
  return trimmed ? `${row.trigger} — ${trimmed}${suffix}` : `${row.trigger}${suffix}`;
}

/** Opaque reference for `kernel.updateObject` / `kernel.deleteObject`. */
export type BehaviorObjectId = ObjectId;
