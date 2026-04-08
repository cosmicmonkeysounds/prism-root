/**
 * PrivilegeSet — granular access control for Prism workspaces.
 *
 * Inspired by FileMaker Pro's privilege sets: declarative rules mapping
 * DID roles to collection/field/layout permissions with row-level filtering.
 *
 * A PrivilegeSet is defined in the Manifest and evaluated at runtime by the
 * Shell and CollectionStore to enforce access control.
 *
 * Usage:
 *   const adminSet = createPrivilegeSet('admin', 'Administrator', {
 *     collections: { '*': 'full' },
 *     layouts: { '*': 'visible' },
 *   });
 *
 *   const clientSet = createPrivilegeSet('client', 'Client View', {
 *     collections: {
 *       'invoices': 'read',
 *       'contacts': 'none',
 *     },
 *     fields: {
 *       'invoices.cost_breakdown': 'hidden',
 *       'invoices.internal_notes': 'hidden',
 *     },
 *     layouts: {
 *       'invoice-detail': 'visible',
 *       'admin-dashboard': 'hidden',
 *     },
 *     recordFilter: 'record.client_did == current_did',
 *   });
 */

// ── Permission Levels ───────────────────────────────────────────────────────

/** Collection-level access. */
export type CollectionPermission = "full" | "read" | "create" | "none";

/** Field-level access. */
export type FieldPermission = "readwrite" | "readonly" | "hidden";

/** Layout-level visibility. */
export type LayoutPermission = "visible" | "hidden";

/** Script execution permission. */
export type ScriptPermission = "execute" | "none";

// ── Privilege Set ───────────────────────────────────────────────────────────

export interface PrivilegeSet {
  /** Unique identifier for this privilege set. */
  id: string;
  /** Human-readable name (e.g. "Administrator", "Client View"). */
  name: string;
  /** Optional description. */
  description?: string;
  /**
   * Collection-level permissions.
   * Keys are collection IDs or '*' for default.
   * Values are access levels.
   */
  collections: Record<string, CollectionPermission>;
  /**
   * Field-level permissions (overrides collection-level).
   * Keys are "collectionId.fieldPath" (e.g. "invoices.cost_breakdown").
   * Values are field access levels.
   */
  fields?: Record<string, FieldPermission>;
  /**
   * Layout-level visibility.
   * Keys are facet definition IDs or '*' for default.
   */
  layouts?: Record<string, LayoutPermission>;
  /**
   * Script execution permissions.
   * Keys are automation IDs or '*' for default.
   */
  scripts?: Record<string, ScriptPermission>;
  /**
   * Row-level security filter expression.
   * Evaluated per record — only records matching this expression are visible.
   * Uses ExpressionEngine syntax. Available variables:
   *   - `record.*` — fields on the current record
   *   - `current_did` — DID of the current user
   *   - `current_role` — role name of the current user
   *
   * Example: "record.owner_did == current_did"
   */
  recordFilter?: string;
  /** Whether this is the default set for new/anonymous users. */
  isDefault?: boolean;
  /** Whether users with this set can manage other users' privilege sets. */
  canManageAccess?: boolean;
}

// ── Factory ─────────────────────────────────────────────────────────────────

export interface PrivilegeSetOptions {
  collections: Record<string, CollectionPermission>;
  fields?: Record<string, FieldPermission>;
  layouts?: Record<string, LayoutPermission>;
  scripts?: Record<string, ScriptPermission>;
  recordFilter?: string;
  isDefault?: boolean;
  canManageAccess?: boolean;
}

export function createPrivilegeSet(
  id: string,
  name: string,
  options: PrivilegeSetOptions,
): PrivilegeSet {
  const ps: PrivilegeSet = {
    id,
    name,
    collections: { ...options.collections },
  };
  if (options.fields) ps.fields = { ...options.fields };
  if (options.layouts) ps.layouts = { ...options.layouts };
  if (options.scripts) ps.scripts = { ...options.scripts };
  if (options.recordFilter) ps.recordFilter = options.recordFilter;
  if (options.isDefault !== undefined) ps.isDefault = options.isDefault;
  if (options.canManageAccess !== undefined) ps.canManageAccess = options.canManageAccess;
  return ps;
}

// ── Role Assignment ─────────────────────────────────────────────────────────

/**
 * Maps a DID to a privilege set. Stored in the Manifest.
 */
export interface RoleAssignment {
  /** DID of the user. */
  did: string;
  /** PrivilegeSet ID. */
  privilegeSetId: string;
  /** Optional display name for this user in the access list. */
  displayName?: string;
}

// ── Evaluation helpers ──────────────────────────────────────────────────────

/**
 * Check collection-level permission for a privilege set.
 * Falls back to '*' wildcard, then 'none'.
 */
export function getCollectionPermission(
  privilegeSet: PrivilegeSet,
  collectionId: string,
): CollectionPermission {
  return privilegeSet.collections[collectionId]
    ?? privilegeSet.collections["*"]
    ?? "none";
}

/**
 * Check field-level permission. Falls back to collection-level permission
 * mapped to a field equivalent (full/create → readwrite, read → readonly, none → hidden).
 */
export function getFieldPermission(
  privilegeSet: PrivilegeSet,
  collectionId: string,
  fieldPath: string,
): FieldPermission {
  const fieldKey = `${collectionId}.${fieldPath}`;
  if (privilegeSet.fields?.[fieldKey]) {
    return privilegeSet.fields[fieldKey] as FieldPermission;
  }
  // Fallback: derive from collection permission
  const collPerm = getCollectionPermission(privilegeSet, collectionId);
  switch (collPerm) {
    case "full":
    case "create":
      return "readwrite";
    case "read":
      return "readonly";
    case "none":
      return "hidden";
  }
}

/**
 * Check layout visibility. Falls back to '*' wildcard, then 'visible'.
 */
export function getLayoutPermission(
  privilegeSet: PrivilegeSet,
  layoutId: string,
): LayoutPermission {
  return privilegeSet.layouts?.[layoutId]
    ?? privilegeSet.layouts?.["*"]
    ?? "visible";
}

/**
 * Check script execution permission. Falls back to '*' wildcard, then 'none'.
 */
export function getScriptPermission(
  privilegeSet: PrivilegeSet,
  scriptId: string,
): ScriptPermission {
  return privilegeSet.scripts?.[scriptId]
    ?? privilegeSet.scripts?.["*"]
    ?? "none";
}

/**
 * Check if a collection allows write operations.
 */
export function canWrite(
  privilegeSet: PrivilegeSet,
  collectionId: string,
): boolean {
  const perm = getCollectionPermission(privilegeSet, collectionId);
  return perm === "full" || perm === "create";
}

/**
 * Check if a collection allows read operations.
 */
export function canRead(
  privilegeSet: PrivilegeSet,
  collectionId: string,
): boolean {
  const perm = getCollectionPermission(privilegeSet, collectionId);
  return perm !== "none";
}
