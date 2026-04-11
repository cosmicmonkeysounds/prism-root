/**
 * Field Resolver — computes values for formula, lookup, and rollup fields.
 *
 * A GraphObject stores raw payload values in its `data` map. Entity definitions
 * can declare additional fields that are **derived** at read time:
 *
 *   - `formula`  — expression over the object's own shell/data
 *   - `lookup`   — traverse an edge relation, pull a field from the target
 *   - `rollup`   — traverse an edge relation, aggregate across target values
 *
 * This module encapsulates that resolution. The stores are duck-typed so the
 * resolver stays pure-TS and does not couple to CollectionStore/TreeModel.
 */

import type { EntityFieldDef, GraphObject, ObjectEdge, ObjectId, RollupFunction } from "@prism/core/object-model";
import type { ExprValue } from "./expression-types.js";
import { evaluateExpression } from "./evaluator.js";

// ── Store interfaces ────────────────────────────────────────────────────────

/** Minimum surface area we need from an edge store. */
export interface EdgeLookup {
  /** Return all edges with the given relation starting at sourceId. */
  getEdges(sourceId: ObjectId, relation: string): ObjectEdge[];
}

/** Minimum surface area we need from an object store. */
export interface ObjectLookup {
  getObject(id: ObjectId): GraphObject | undefined;
}

export interface FieldResolverStores {
  edges: EdgeLookup;
  objects: ObjectLookup;
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/** Pull a value out of a GraphObject, honouring a dot-path into `data`. */
export function readObjectField(object: GraphObject, path: string): unknown {
  if (!path) return undefined;
  // Top-level shell fields first.
  const shell = object as unknown as Record<string, unknown>;
  if (path in shell && path !== "data") return shell[path];

  const segments = path.split(".");
  let current: unknown = object.data;
  for (const seg of segments) {
    if (current == null || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[seg];
  }
  return current;
}

function toNumber(v: unknown): number {
  if (typeof v === "number") return Number.isFinite(v) ? v : 0;
  if (typeof v === "boolean") return v ? 1 : 0;
  if (typeof v === "string") {
    const n = parseFloat(v);
    return Number.isNaN(n) ? 0 : n;
  }
  return 0;
}

function toExprValue(v: unknown): ExprValue {
  if (typeof v === "number" || typeof v === "boolean" || typeof v === "string") return v;
  if (v == null) return 0;
  return String(v);
}

/** Build an expression context from a GraphObject (shell + data flattened). */
export function buildFormulaContext(object: GraphObject): Record<string, ExprValue> {
  const ctx: Record<string, ExprValue> = {};
  // Shell fields first.
  ctx.id = object.id;
  ctx.type = object.type;
  ctx.name = object.name;
  ctx.status = object.status ?? "";
  ctx.description = object.description;
  ctx.date = object.date ?? "";
  ctx.endDate = object.endDate ?? "";
  ctx.pinned = object.pinned;
  // Data fields.
  for (const [k, v] of Object.entries(object.data)) {
    ctx[k] = toExprValue(v);
  }
  return ctx;
}

// ── Resolvers ───────────────────────────────────────────────────────────────

/** Resolve a formula field by evaluating its expression against the object. */
export function resolveFormulaField(object: GraphObject, fieldDef: EntityFieldDef): ExprValue {
  if (!fieldDef.expression) return 0;
  const ctx = buildFormulaContext(object);
  const { result } = evaluateExpression(fieldDef.expression, ctx);
  return result;
}

/** Resolve a lookup field by following an edge relation to the first target. */
export function resolveLookupField(
  object: GraphObject,
  fieldDef: EntityFieldDef,
  stores: FieldResolverStores,
): unknown {
  if (!fieldDef.lookupRelation || !fieldDef.lookupField) return undefined;
  const edges = stores.edges.getEdges(object.id, fieldDef.lookupRelation);
  const firstEdge = edges[0];
  if (!firstEdge) return undefined;
  const firstTarget = stores.objects.getObject(firstEdge.targetId);
  if (!firstTarget) return undefined;
  return readObjectField(firstTarget, fieldDef.lookupField);
}

/** Aggregate a list of raw values according to the given rollup function. */
export function aggregate(values: unknown[], fn: RollupFunction): ExprValue {
  if (fn === "count") return values.length;
  if (fn === "list") return values.map((v) => (v == null ? "" : String(v))).join(", ");
  if (values.length === 0) return 0;
  const nums = values.map(toNumber);
  switch (fn) {
    case "sum":
      return nums.reduce((a, b) => a + b, 0);
    case "avg":
      return nums.reduce((a, b) => a + b, 0) / nums.length;
    case "min":
      return Math.min(...nums);
    case "max":
      return Math.max(...nums);
    default:
      return 0;
  }
}

/** Resolve a rollup field by collecting target field values and aggregating. */
export function resolveRollupField(
  object: GraphObject,
  fieldDef: EntityFieldDef,
  stores: FieldResolverStores,
): ExprValue {
  if (!fieldDef.rollupRelation || !fieldDef.rollupField) return 0;
  const fn: RollupFunction = fieldDef.rollupFunction ?? "count";
  const edges = stores.edges.getEdges(object.id, fieldDef.rollupRelation);
  const values: unknown[] = [];
  for (const edge of edges) {
    const target = stores.objects.getObject(edge.targetId);
    if (!target) continue;
    values.push(readObjectField(target, fieldDef.rollupField));
  }
  return aggregate(values, fn);
}

/**
 * Dispatcher: resolve whichever computed-field type the definition declares.
 * Returns `undefined` if the field is not a computed type (caller should read
 * the stored value from `object.data[fieldDef.id]` instead).
 */
export function resolveComputedField(
  object: GraphObject,
  fieldDef: EntityFieldDef,
  stores: FieldResolverStores,
): ExprValue | undefined {
  // Formula may appear on any field type — check it first.
  if (fieldDef.expression) return resolveFormulaField(object, fieldDef);
  if (fieldDef.type === "lookup") return toExprValue(resolveLookupField(object, fieldDef, stores));
  if (fieldDef.type === "rollup") return resolveRollupField(object, fieldDef, stores);
  return undefined;
}
