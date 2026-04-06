/**
 * @prism/core — Template Types
 *
 * Object templates are reusable blueprints for creating subtrees.
 * A template captures the shape (types, names, children, edges, field values)
 * of an object subtree and can be instantiated with variable interpolation.
 */

import type { GraphObject, ObjectEdge } from "../object-model/types.js";

// ── Template Variable ─────────────────────────────────────────────────────────

export interface TemplateVariable {
  /** Variable name (used as {{name}} in templates). */
  name: string;
  /** Human-readable label for UI. */
  label?: string;
  /** Default value if not provided at instantiation. */
  defaultValue?: string;
  /** Whether a value is required at instantiation. */
  required?: boolean;
}

// ── Object Template ───────────────────────────────────���───────────────────────

/**
 * A node in the template tree. Mirrors GraphObject structure
 * but with placeholder IDs and optional template variables in string fields.
 */
export interface TemplateNode {
  /** Placeholder ID (used for internal references). */
  placeholderId: string;
  /** Object type. */
  type: string;
  /** Name — may contain {{variable}} placeholders. */
  name: string;
  /** Status — may contain {{variable}} placeholders. */
  status?: string | null | undefined;
  /** Tags. */
  tags?: string[] | undefined;
  /** Description — may contain {{variable}} placeholders. */
  description?: string | undefined;
  /** Color. */
  color?: string | null | undefined;
  /** Pinned flag. */
  pinned?: boolean | undefined;
  /** Data payload — string values may contain {{variable}} placeholders. */
  data?: Record<string, unknown> | undefined;
  /** Children in order. */
  children?: TemplateNode[] | undefined;
}

/**
 * An edge template — references placeholder IDs instead of real ObjectIds.
 */
export interface TemplateEdge {
  /** Placeholder ID of the source object. */
  sourcePlaceholderId: string;
  /** Placeholder ID of the target object. */
  targetPlaceholderId: string;
  /** Edge relation type. */
  relation: string;
  /** Edge data payload. */
  data?: Record<string, unknown> | undefined;
}

/**
 * A complete object template — root node, internal edges, variables, metadata.
 */
export interface ObjectTemplate {
  /** Unique template identifier. */
  id: string;
  /** Human-readable template name. */
  name: string;
  /** Optional description. */
  description?: string | undefined;
  /** Category for organizing templates (e.g. "productivity", "game-design"). */
  category?: string | undefined;
  /** The root node of the template tree. */
  root: TemplateNode;
  /** Internal edges between template nodes. */
  edges?: TemplateEdge[] | undefined;
  /** Declared variables for interpolation. */
  variables?: TemplateVariable[] | undefined;
  /** ISO timestamp of creation. */
  createdAt: string;
}

// ── Instantiation ─────────────────────────────────────────────────────────────

export interface InstantiateOptions {
  /** Variable values to interpolate. Keys = variable names. */
  variables?: Record<string, string>;
  /** Target parent for the root object. null = root level. */
  parentId?: string | null;
  /** Position among siblings. */
  position?: number;
}

export interface InstantiateResult {
  /** All created objects (root + descendants). */
  created: GraphObject[];
  /** All created edges. */
  createdEdges: ObjectEdge[];
  /** Map from placeholder ID -> real ObjectId. */
  idMap: Map<string, string>;
}

// ── Registry Options ───��──────────────────────────────���───────────────────────

export type TemplateFilter = {
  category?: string;
  type?: string;
  search?: string;
};
