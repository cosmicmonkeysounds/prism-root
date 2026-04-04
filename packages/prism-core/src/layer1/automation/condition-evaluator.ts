/**
 * Condition evaluator — pure function evaluation of AutomationCondition trees.
 *
 * Supports: field comparison (10 operators), type check, tag check (all/any),
 * logical combinators (and/or/not).
 */

import type { AutomationCondition, AutomationContext } from "./automation-types.js";

/** Resolve a dot-path on an object. */
export function getPath(obj: unknown, path: string): unknown {
  return path.split(".").reduce((cur, key) => {
    if (cur === null || cur === undefined) return undefined;
    return (cur as Record<string, unknown>)[key];
  }, obj as unknown);
}

/** Compare two values with the given operator. */
export function compare(
  actual: unknown,
  op: string,
  expected: unknown,
): boolean {
  switch (op) {
    case "eq":
      return actual === expected;
    case "neq":
      return actual !== expected;
    case "gt":
      return (actual as number) > (expected as number);
    case "gte":
      return (actual as number) >= (expected as number);
    case "lt":
      return (actual as number) < (expected as number);
    case "lte":
      return (actual as number) <= (expected as number);
    case "contains":
      return String(actual).includes(String(expected));
    case "startsWith":
      return String(actual).startsWith(String(expected));
    case "endsWith":
      return String(actual).endsWith(String(expected));
    case "matches":
      return new RegExp(String(expected)).test(String(actual));
    default:
      return false;
  }
}

/** Evaluate a condition tree against an automation context. */
export function evaluateCondition(
  cond: AutomationCondition,
  ctx: AutomationContext,
): boolean {
  switch (cond.type) {
    case "field": {
      const actual = getPath(ctx, cond.path);
      return compare(actual, cond.operator, cond.value);
    }
    case "type":
      return (ctx.object?.["type"] as string) === cond.objectType;
    case "tags": {
      const tags = (ctx.object?.["tags"] as string[]) ?? [];
      return cond.mode === "all"
        ? cond.tags.every((t) => tags.includes(t))
        : cond.tags.some((t) => tags.includes(t));
    }
    case "and":
      return cond.conditions.every((c) => evaluateCondition(c, ctx));
    case "or":
      return cond.conditions.some((c) => evaluateCondition(c, ctx));
    case "not":
      return !evaluateCondition(cond.condition, ctx);
    default:
      return true;
  }
}

/** Replace {{path}} placeholders in an action template with context values. */
export function interpolate<T>(template: T, ctx: AutomationContext): T {
  return JSON.parse(
    JSON.stringify(template).replace(
      /\{\{([\w.]+)\}\}/g,
      (_, path: string) => String(getPath(ctx, path) ?? ""),
    ),
  ) as T;
}

/** Check if an object event matches an object trigger's filters. */
export function matchesObjectTrigger(
  trigger: {
    objectTypes?: string[] | undefined;
    tags?: string[] | undefined;
    fieldMatch?: Record<string, unknown> | undefined;
  },
  event: { object: Record<string, unknown> },
): boolean {
  if (trigger.objectTypes?.length) {
    const objType = event.object["type"] as string;
    if (!trigger.objectTypes.includes(objType)) return false;
  }
  if (trigger.tags?.length) {
    const tags = (event.object["tags"] as string[]) ?? [];
    if (!trigger.tags.every((t) => tags.includes(t))) return false;
  }
  if (trigger.fieldMatch) {
    for (const [key, val] of Object.entries(trigger.fieldMatch)) {
      if (event.object[key] !== val) return false;
    }
  }
  return true;
}
