/**
 * FacetRuntime — runtime evaluation for facet features.
 *
 * Pure functions for:
 *   - Conditional formatting: evaluate expressions per-record, return styles
 *   - Merge field interpolation: resolve {{fieldName}} in TextSlot text
 *   - Value list resolution from CollectionStore
 */

import type { GraphObject } from "@prism/core/object-model";
import type { ConditionalFormat, FieldSlot, TextSlot, FacetDefinition } from "./facet-schema.js";
import type {
  DynamicValueListSource,
  ValueListItem,
  ValueListResolver,
} from "./value-list.js";
import type { FilterConfig } from "@prism/core/view";

// ── Conditional Formatting ──────────────────────────────────────────────────

export interface ComputedStyle {
  backgroundColor?: string;
  textColor?: string;
  fontWeight?: number;
  border?: string;
}

/**
 * Simple expression evaluator for conditional format expressions.
 * Supports patterns:
 *   [field:name] == "value"
 *   [field:amount] > 1000
 *   [field:status] != "archived"
 *   [field:count] >= 5
 *
 * Returns true if the expression matches the record.
 */
function evaluateSimpleExpression(
  expression: string,
  object: GraphObject,
): boolean {
  // Pattern: [field:path] op value
  const match = expression.match(
    /\[field:([^\]]+)\]\s*(==|!=|>=|<=|>|<)\s*(.+)$/,
  );
  if (!match) return false;

  const [, fieldPath, op, rawValue] = match as [string, string, string, string];
  const actual = object.data[fieldPath] ?? (object as unknown as Record<string, unknown>)[fieldPath];

  // Parse value
  let expected: unknown = rawValue.trim();
  if (typeof expected === "string") {
    if (expected.startsWith('"') && expected.endsWith('"')) {
      expected = expected.slice(1, -1);
    } else if (expected === "true") {
      expected = true;
    } else if (expected === "false") {
      expected = false;
    } else if (expected === "nil" || expected === "null") {
      expected = null;
    } else if (!isNaN(Number(expected))) {
      expected = Number(expected);
    }
  }

  switch (op) {
    case "==": return actual === expected;
    case "!=": return actual !== expected;
    case ">": return typeof actual === "number" && typeof expected === "number" && actual > expected;
    case "<": return typeof actual === "number" && typeof expected === "number" && actual < expected;
    case ">=": return typeof actual === "number" && typeof expected === "number" && actual >= expected;
    case "<=": return typeof actual === "number" && typeof expected === "number" && actual <= expected;
    default: return false;
  }
}

/**
 * Evaluate conditional formats for a field slot against a record.
 * Returns the merged computed styles from all matching rules (later rules override).
 */
export function evaluateConditionalFormats(
  formats: ConditionalFormat[],
  object: GraphObject,
): ComputedStyle {
  const result: ComputedStyle = {};

  for (const fmt of formats) {
    if (evaluateSimpleExpression(fmt.expression, object)) {
      if (fmt.backgroundColor) result.backgroundColor = fmt.backgroundColor;
      if (fmt.textColor) result.textColor = fmt.textColor;
      if (fmt.fontWeight !== undefined) result.fontWeight = fmt.fontWeight;
      if (fmt.border) result.border = fmt.border;
    }
  }

  return result;
}

/**
 * Evaluate conditional formats for a field slot, returning the style
 * or an empty object if no conditions match.
 */
export function computeFieldStyle(
  slot: FieldSlot,
  object: GraphObject,
): ComputedStyle {
  if (!slot.conditionalFormats || slot.conditionalFormats.length === 0) {
    return {};
  }
  return evaluateConditionalFormats(slot.conditionalFormats, object);
}

// ── Merge Field Interpolation ───────────────────────────────────────────────

/**
 * Resolve {{fieldName}} merge fields in text content.
 * Supports dot-notation paths: {{address.city}}
 *
 * @param text - Text with {{fieldName}} placeholders
 * @param object - GraphObject to pull field values from
 * @returns Interpolated text with field values substituted
 */
export function interpolateMergeFields(
  text: string,
  object: GraphObject,
): string {
  return text.replace(/\{\{(\w+(?:\.\w+)*)\}\}/g, (_match, fieldPath: string) => {
    const parts = fieldPath.split(".");
    let current: unknown = object.data;

    for (const part of parts) {
      if (current === null || current === undefined) return "";
      current = (current as Record<string, unknown>)[part];
    }

    // Fall back to shell fields (name, type, status, etc.)
    if (current === null || current === undefined) {
      current = (object as unknown as Record<string, unknown>)[fieldPath];
    }

    if (current === null || current === undefined) return "";
    return String(current);
  });
}

/**
 * Interpolate a TextSlot's text content against a record.
 */
export function renderTextSlot(slot: TextSlot, object: GraphObject): string {
  return interpolateMergeFields(slot.text, object);
}

// ── CollectionStore ValueList Resolver ───────────────────────────────────────

/**
 * Interface for a minimal collection data source.
 * Matches the subset of CollectionStore needed for value list resolution.
 */
export interface ValueListDataSource {
  allObjects(): GraphObject[];
}

/**
 * Create a ValueListResolver that sources dynamic value lists from a data source.
 * Applies the DynamicValueListSource config: collection filtering, field extraction,
 * optional sorting, and limit.
 */
export function createCollectionValueListResolver(
  collections: Record<string, ValueListDataSource>,
): ValueListResolver {
  return {
    resolve(source: DynamicValueListSource): ValueListItem[] {
      const collection = collections[source.collectionId];
      if (!collection) return [];

      let objects = collection.allObjects();

      // Apply filter if present
      if (source.filter) {
        objects = applySimpleFilter(objects, source.filter);
      }

      // Sort if configured
      if (source.sortField) {
        const dir = source.sortDirection === "desc" ? -1 : 1;
        objects = [...objects].sort((a, b) => {
          const av = getNestedValue(a, source.sortField as string);
          const bv = getNestedValue(b, source.sortField as string);
          if (av === bv) return 0;
          if (av === null || av === undefined) return 1;
          if (bv === null || bv === undefined) return -1;
          return (av < bv ? -1 : 1) * dir;
        });
      }

      // Apply limit
      if (source.limit) {
        objects = objects.slice(0, source.limit);
      }

      // Extract value/display pairs
      return objects.map((obj) => ({
        value: String(getNestedValue(obj, source.valueField) ?? obj.id),
        label: String(getNestedValue(obj, source.displayField) ?? obj.name),
      }));
    },
  };
}

function getNestedValue(obj: GraphObject, path: string): unknown {
  // Check data first
  const parts = path.split(".");
  let current: unknown = obj.data;
  for (const part of parts) {
    if (current === null || current === undefined) break;
    current = (current as Record<string, unknown>)[part];
  }
  if (current !== undefined) return current;

  // Fall back to shell fields
  return (obj as unknown as Record<string, unknown>)[path];
}

function applySimpleFilter(objects: GraphObject[], filter: FilterConfig): GraphObject[] {
  return objects.filter((obj) => {
    const actual = getNestedValue(obj, filter.field);
    switch (filter.op) {
      case "eq": return actual === filter.value;
      case "neq": return actual !== filter.value;
      case "contains":
        return typeof actual === "string" && typeof filter.value === "string" &&
          actual.toLowerCase().includes(filter.value.toLowerCase());
      case "empty":
        return actual === null || actual === undefined || actual === "";
      case "notempty":
        return actual !== null && actual !== undefined && actual !== "";
      default: return true;
    }
  });
}

// ── Facet Definition Helpers ────────────────────────────────────────────────

/**
 * Get the value list binding for a field in a facet definition.
 */
export function getValueListId(
  definition: FacetDefinition,
  fieldPath: string,
): string | undefined {
  return definition.valueListBindings?.[fieldPath];
}

/**
 * Get all field paths that have value list bindings.
 */
export function getBoundFields(
  definition: FacetDefinition,
): Array<{ fieldPath: string; valueListId: string }> {
  if (!definition.valueListBindings) return [];
  return Object.entries(definition.valueListBindings).map(
    ([fieldPath, valueListId]) => ({ fieldPath, valueListId }),
  );
}
