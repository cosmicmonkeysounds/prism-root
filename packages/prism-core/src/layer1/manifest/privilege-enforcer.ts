/**
 * PrivilegeEnforcer — runtime evaluation of PrivilegeSets against data.
 *
 * Wraps access control logic: filters objects by row-level security,
 * redacts hidden fields, and enforces read/write permissions.
 *
 * Usage:
 *   const enforcer = createPrivilegeEnforcer(privilegeSet);
 *   const visible = enforcer.filterObjects('invoices', allObjects);
 *   const canEdit = enforcer.canEditField('invoices', 'cost_breakdown');
 *   const redacted = enforcer.redactObject('invoices', object);
 */

import type { GraphObject } from "../object-model/index.js";
import type { PrivilegeSet, CollectionPermission, FieldPermission } from "./privilege-set.js";
import {
  getCollectionPermission,
  getFieldPermission,
  getLayoutPermission,
  canWrite as canWriteCollection,
  canRead as canReadCollection,
} from "./privilege-set.js";

// ── Enforcer ────────────────────────────────────────────────────────────────

export interface PrivilegeContext {
  /** DID of the current user. */
  currentDid: string;
  /** Role name of the current user (optional). */
  currentRole?: string;
}

export interface PrivilegeEnforcer {
  /** The active privilege set. */
  readonly privilegeSet: PrivilegeSet;

  /** Check if a collection is readable. */
  canRead(collectionId: string): boolean;

  /** Check if a collection is writable. */
  canWrite(collectionId: string): boolean;

  /** Get the permission level for a specific field. */
  fieldPermission(collectionId: string, fieldPath: string): FieldPermission;

  /** Check if a field is editable. */
  canEditField(collectionId: string, fieldPath: string): boolean;

  /** Check if a field is visible. */
  canSeeField(collectionId: string, fieldPath: string): boolean;

  /** Check if a layout is visible. */
  canSeeLayout(layoutId: string): boolean;

  /** Get collection permission level. */
  collectionPermission(collectionId: string): CollectionPermission;

  /**
   * Filter objects by row-level security expression.
   * If no recordFilter is defined, returns all objects.
   */
  filterObjects(
    collectionId: string,
    objects: GraphObject[],
    context: PrivilegeContext,
  ): GraphObject[];

  /**
   * Redact an object by removing hidden fields from data.
   * Returns a shallow copy with hidden fields stripped.
   */
  redactObject(collectionId: string, object: GraphObject): GraphObject;

  /**
   * Get the list of visible field paths for a collection.
   * Returns all data keys that are not hidden.
   */
  visibleFields(collectionId: string, allFields: string[]): string[];
}

/**
 * Evaluate a simple row-level security expression.
 * Supports basic patterns:
 *   - record.field == value
 *   - record.field != value
 *   - record.field == current_did
 *   - record.field == current_role
 *
 * Returns true if the record passes the filter.
 */
function evaluateRecordFilter(
  expression: string,
  object: GraphObject,
  context: PrivilegeContext,
): boolean {
  // Parse simple expressions: "record.field op value"
  const match = expression.match(
    /^record\.(\w+)\s*(==|!=)\s*(.+)$/,
  );
  if (!match) return true; // Unparseable = allow

  const [, field, op, rawValue] = match as [string, string, string, string];
  const actual = object.data[field] ?? (object as unknown as Record<string, unknown>)[field];

  // Resolve special variables
  let expected: unknown = rawValue.trim();
  if (expected === "current_did") expected = context.currentDid;
  else if (expected === "current_role") expected = context.currentRole;
  else if (typeof expected === "string" && expected.startsWith('"') && expected.endsWith('"')) {
    expected = expected.slice(1, -1);
  }

  if (op === "==") return actual === expected;
  if (op === "!=") return actual !== expected;
  return true;
}

export function createPrivilegeEnforcer(
  privilegeSet: PrivilegeSet,
): PrivilegeEnforcer {
  return {
    get privilegeSet() {
      return privilegeSet;
    },

    canRead(collectionId: string): boolean {
      return canReadCollection(privilegeSet, collectionId);
    },

    canWrite(collectionId: string): boolean {
      return canWriteCollection(privilegeSet, collectionId);
    },

    fieldPermission(collectionId: string, fieldPath: string): FieldPermission {
      return getFieldPermission(privilegeSet, collectionId, fieldPath);
    },

    canEditField(collectionId: string, fieldPath: string): boolean {
      return getFieldPermission(privilegeSet, collectionId, fieldPath) === "readwrite";
    },

    canSeeField(collectionId: string, fieldPath: string): boolean {
      return getFieldPermission(privilegeSet, collectionId, fieldPath) !== "hidden";
    },

    canSeeLayout(layoutId: string): boolean {
      return getLayoutPermission(privilegeSet, layoutId) === "visible";
    },

    collectionPermission(collectionId: string): CollectionPermission {
      return getCollectionPermission(privilegeSet, collectionId);
    },

    filterObjects(
      collectionId: string,
      objects: GraphObject[],
      context: PrivilegeContext,
    ): GraphObject[] {
      if (!canReadCollection(privilegeSet, collectionId)) return [];
      if (!privilegeSet.recordFilter) return objects;
      return objects.filter((obj) =>
        evaluateRecordFilter(privilegeSet.recordFilter as string, obj, context),
      );
    },

    redactObject(collectionId: string, object: GraphObject): GraphObject {
      if (!privilegeSet.fields) return object;

      const redactedData: Record<string, unknown> = {};
      for (const [key, value] of Object.entries(object.data)) {
        const perm = getFieldPermission(privilegeSet, collectionId, key);
        if (perm !== "hidden") {
          redactedData[key] = value;
        }
      }

      return { ...object, data: redactedData };
    },

    visibleFields(collectionId: string, allFields: string[]): string[] {
      return allFields.filter(
        (field) => getFieldPermission(privilegeSet, collectionId, field) !== "hidden",
      );
    },
  };
}
