/**
 * Data binding — resolve `[obj:Name]`, `[obj:Name.field]`, and `[self:field]`
 * placeholders in strings against the current collection.
 *
 * This is the minimal mechanism that powers Tier 4B of the studio checklist:
 * text blocks, headings, and any string field can reference other objects
 * without a full expression engine. The syntax is intentionally limited so
 * the pipeline is O(n) and self-explanatory to non-programmers.
 *
 * Also exports `evaluateVisibleWhen()` for the Tier 4D conditional
 * visibility knob on every block's `visibleWhen` field.
 */

import type { GraphObject } from "@prism/core/object-model";

const REF_RE = /\[obj:([^\]]+)\]/g;
const SELF_RE = /\[self:([^\]]+)\]/g;

/** Read a dotted path out of a plain object, returning undefined on miss. */
export function readPath(root: unknown, path: string): unknown {
  if (root == null || !path) return undefined;
  const parts = path.split(".");
  let current: unknown = root;
  for (const part of parts) {
    if (current == null || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

/** Format a value for textual interpolation; objects become JSON. */
export function formatValue(value: unknown): string {
  if (value === undefined || value === null) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  try {
    return JSON.stringify(value);
  } catch {
    return "";
  }
}

/**
 * Resolve `[obj:Name.field]` and `[self:field]` placeholders against the
 * given object pool. `Name` matches a GraphObject.name (case-sensitive).
 * `field` can be a dotted path into `.data`, or any top-level GraphObject key.
 * Unknown refs are left in place so the author can see them.
 */
export function resolveObjectRefs(
  source: string | undefined | null,
  pool: ReadonlyArray<GraphObject>,
  self?: GraphObject,
): string {
  if (!source) return "";
  let result = source;

  result = result.replace(SELF_RE, (_match, fieldExpr: string) => {
    if (!self) return `[self:${fieldExpr}]`;
    const value = readField(self, fieldExpr);
    return value === undefined ? `[self:${fieldExpr}]` : formatValue(value);
  });

  result = result.replace(REF_RE, (_match, expr: string) => {
    const [name, ...rest] = expr.split(".");
    if (!name) return `[obj:${expr}]`;
    const target = pool.find((o) => o.name === name && !o.deletedAt);
    if (!target) return `[obj:${expr}]`;
    if (rest.length === 0) {
      // Default: name
      return target.name;
    }
    const value = readField(target, rest.join("."));
    return value === undefined ? `[obj:${expr}]` : formatValue(value);
  });

  return result;
}

/** Read a field off a GraphObject — top-level key or `.data.dotted.path`. */
function readField(obj: GraphObject, expr: string): unknown {
  if (!expr) return undefined;
  const top = (obj as unknown as Record<string, unknown>)[expr];
  if (top !== undefined) return top;
  return readPath(obj.data, expr);
}

/**
 * Evaluate a visibility expression. An empty/undefined expression means
 * "always visible". The expression supports:
 *   - plain booleans: "true" / "false"
 *   - numeric comparisons: "a > b", "a == b", "a != b"
 *   - `[obj:…]` / `[self:…]` placeholders (resolved first)
 *
 * Returns `true` on parse failure so authors aren't accidentally locked
 * out of their content by a typo.
 */
export function evaluateVisibleWhen(
  expression: string | undefined | null,
  pool: ReadonlyArray<GraphObject>,
  self?: GraphObject,
): boolean {
  if (!expression || typeof expression !== "string") return true;
  const trimmed = expression.trim();
  if (trimmed === "") return true;

  const resolved = resolveObjectRefs(trimmed, pool, self);
  if (resolved === "true") return true;
  if (resolved === "false") return false;

  // Very small comparison parser. Supports ==, !=, >, >=, <, <=.
  const ops: Array<{ op: string; fn: (a: number, b: number) => boolean }> = [
    { op: "==", fn: (a, b) => a === b },
    { op: "!=", fn: (a, b) => a !== b },
    { op: ">=", fn: (a, b) => a >= b },
    { op: "<=", fn: (a, b) => a <= b },
    { op: ">", fn: (a, b) => a > b },
    { op: "<", fn: (a, b) => a < b },
  ];
  for (const { op, fn } of ops) {
    const idx = resolved.indexOf(op);
    if (idx > 0) {
      const left = Number(resolved.slice(0, idx).trim());
      const right = Number(resolved.slice(idx + op.length).trim());
      if (!Number.isFinite(left) || !Number.isFinite(right)) return true;
      return fn(left, right);
    }
  }

  // Plain truthiness fallback.
  return !!resolved;
}
