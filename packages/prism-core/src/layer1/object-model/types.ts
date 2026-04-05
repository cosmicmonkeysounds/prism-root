/**
 * @prism/core — Object Model Types
 *
 * Two distinct layers:
 *
 *   OBJECT — the graph primitive. Every node in the unified graph is an object.
 *            The graph can store, traverse, relate, filter, and display objects
 *            without knowing what they semantically ARE. An object has:
 *              - Shell:   universal structural fields (id, type, name, parentId, ...)
 *              - Payload: `data: Record<string, unknown>` — opaque to the core,
 *                         interpreted by the EntityDef registered for this type.
 *
 *   ENTITY — a schema-typed category of object. An EntityDef is the blueprint:
 *            authored in YAML/JSON, registered into ObjectRegistry. It specifies
 *            what fields the payload contains, what category the type belongs to
 *            (containment rules), what views/tabs it supports, and what edges it
 *            may participate in. Every entity is an object. Not every object must
 *            map to a rich entity schema (e.g. structural scaffolding like folders
 *            uses minimal entity-level schema).
 *
 * Relationships are first-class:
 *
 *   TREE (parentId)    — hierarchical containment. One parent, ordered by
 *                        `position`. The registry validates what types can
 *                        nest inside what.
 *
 *   GRAPH (ObjectEdge) — arbitrary typed connections between any two objects.
 *                        Edges carry the semantics that parentId cannot express.
 *
 * The core has no opinion on what entity types exist. Lenses register
 * their own types via ObjectRegistry. This file is domain-agnostic.
 */

// ── Branded identity ───────────────────────────────────────────────────────────

/**
 * Branded string type for object IDs.
 * Prevents accidental use of an EdgeId where an ObjectId is expected and vice versa.
 * Zero runtime cost — use the cast helpers below at API/persistence boundaries.
 */
export type ObjectId = string & { readonly __brand: "ObjectId" };

/** Branded string type for edge IDs. */
export type EdgeId = string & { readonly __brand: "EdgeId" };

/** Cast a raw string to ObjectId at a trust boundary (IPC response, Loro import, etc.) */
export const objectId = (id: string): ObjectId => id as ObjectId;

/** Cast a raw string to EdgeId at a trust boundary. */
export const edgeId = (id: string): EdgeId => id as EdgeId;

// ── Entity field definitions ───────────────────────────────────────────────────

/**
 * The set of value types an entity field may hold.
 * Used in EntityFieldDef.type and drives UI input selection.
 */
export type EntityFieldType =
  | "bool"
  | "int"
  | "float"
  | "string"
  | "text"
  | "color"
  | "enum"
  | "object_ref"
  | "date"
  | "datetime"
  | "url";

/**
 * Defines one typed field in an entity's payload schema.
 *
 * Fields live in the GraphObject.data JSONB payload.
 * Lens-contributed fields (via slots) use namespaced IDs to avoid collisions:
 *   e.g. 'kami_brain_state', 'crm_deal_stage', 'fin_tax_rate'
 */
export interface EntityFieldDef {
  /** Unique field identifier within this entity. Stored as the key in GraphObject.data. */
  id: string;

  /** Value type — drives UI input selection and validation. */
  type: EntityFieldType;

  /** Human-readable label. Defaults to a title-cased version of `id` if omitted. */
  label?: string;

  /** Help text shown below the field in edit mode. */
  description?: string;

  /** Whether this field must have a non-null value. Default: false. */
  required?: boolean;

  /** Default value when a new object of this type is created. */
  default?: boolean | number | string | null;

  /**
   * Formula for a computed field — evaluated by the expression engine at
   * render time. When present, the field value is derived rather than stored.
   *
   * @example  "subtotal + tax"
   * @example  "status == 'done' and daysOld > 7"
   */
  expression?: string;

  /**
   * For type='enum' — the available choices.
   * At least one option is expected when type is 'enum'.
   */
  enumOptions?: Array<{ value: string; label: string }>;

  /**
   * For type='object_ref' — which entity types are valid targets.
   * Empty/omitted means any type is allowed.
   */
  refTypes?: string[];

  /** UI rendering hints. Do not affect the stored value. */
  ui?: {
    /** Render a textarea instead of a text input. Applies to type='string'/'text'. */
    multiline?: boolean;
    /** Placeholder text shown when the field is empty. */
    placeholder?: string;
    /** Groups fields under a collapsible section heading in the detail form. */
    group?: string;
    /** Hide this field from the default UI (still stored, visible in advanced views). */
    hidden?: boolean;
    /** Show the field but prevent editing. */
    readonly?: boolean;
  };
}

// ── Shell ──────────────────────────────────────────────────────────────────────

export interface GraphObject {
  // Identity
  id: ObjectId;
  /** Lens-defined type string. The registry maps this to display metadata. */
  type: string;
  name: string;

  // Tree placement
  parentId: ObjectId | null;
  position: number;

  // Universal metadata — applicable across virtually all types
  status: string | null;
  tags: string[];
  date: string | null;
  endDate: string | null;
  description: string;

  // Display hints
  color: string | null;
  image: string | null;
  pinned: boolean;

  // Arbitrary type-specific payload — opaque to the core
  data: Record<string, unknown>;

  // Timestamps
  createdAt: string;
  updatedAt: string;
  deletedAt?: string | null;
}

// ── Graph Edges ────────────────────────────────────────────────────────────────

/**
 * An edge between two objects in the graph.
 *
 * Same shell + payload pattern as GraphObject:
 *   - Shell:   id, sourceId, targetId, relation, position, createdAt
 *   - Payload: `data` — opaque to the core, interpreted by edge type definitions
 *
 * `relation` is a registered edge type string, just as `type` is a registered
 * entity type string. Lenses register their edge types via EdgeTypeDef.
 */
export interface ObjectEdge {
  id: EdgeId;
  sourceId: ObjectId;
  targetId: ObjectId;
  /** Registered edge type string (e.g. 'depends-on', 'assigned-to') */
  relation: string;
  /** Manual ordering within a relation group. Optional. */
  position?: number;
  createdAt: string;
  data: Record<string, unknown>;
}

/**
 * An edge with its target object resolved inline.
 */
export type ResolvedEdge<TObject = GraphObject> = ObjectEdge & {
  target: TObject;
  /** Present on roll-up results — the intermediate container this came through */
  via?: { id: ObjectId; name: string; type: string };
};

// ── Edge Type Definition ───────────────────────────────────────────────────────

/**
 * Semantic classification of an edge type.
 *
 *   weak       — Loose reference, no cascade. The canonical wiki [[link]].
 *   strong     — Target is "part of" source; typically cascade-delete the edge.
 *   dependency — Source cannot proceed without target (DAG semantics).
 *   membership — Source belongs to a group / collection (target).
 *   assignment — Source is assigned to a person / role (target).
 */
export type EdgeBehavior =
  | "weak"
  | "strong"
  | "dependency"
  | "membership"
  | "assignment";

/**
 * Defines one edge type (relation) to the registry.
 */
export interface EdgeTypeDef {
  /** Unique relation string — stored in ObjectEdge.relation */
  relation: string;

  /**
   * Globally stable NSID for cross-Node edge type interoperability.
   * @example 'io.prismapp.graph.blocks'
   */
  nsid?: string;

  label: string;
  description?: string;

  behavior?: EdgeBehavior;

  /** If true, the edge is undirected (source<->target are interchangeable) */
  undirected?: boolean;

  /**
   * If false, only one edge of this relation is allowed between a given
   * source->target pair. Default: true.
   */
  allowMultiple?: boolean;

  /**
   *   'none'          — edge left orphaned (consumer cleans up later)
   *   'delete-edge'   — edge removed, target survives  [DEFAULT]
   *   'delete-target' — edge and target both deleted (strong ownership)
   */
  cascade?: "none" | "delete-edge" | "delete-target";

  /** Surfaced in inline [[...]] link autocomplete */
  suggestInline?: boolean;

  color?: string;

  // ── Type constraints ────────────────────────────────────────────────────────
  // OR logic within each slot: sourceTypes OR sourceCategories grants permission.
  // The two slots (source / target) are evaluated independently.

  sourceTypes?: string[];
  sourceCategories?: string[];
  targetTypes?: string[];
  targetCategories?: string[];

  // ── Federation ──────────────────────────────────────────────────────────────

  /**
   * Edge scope for federation readiness.
   *
   * - `'local'`     — edge exists only within this Node (default).
   * - `'federated'` — edge can span Nodes (source and target may be on different Prism Nodes).
   *
   * Federated edges store full object addresses (`prism://did:web:node/objects/id`)
   * in sourceId/targetId rather than bare UUIDs.
   */
  scope?: "local" | "federated";
}

// ── Entity Type Definition ─────────────────────────────────────────────────────

/**
 * Describes one entity type to the registry — the blueprint for objects of this type.
 *
 * TIcon is generic so consumers inject their own icon type
 * (LucideIcon, SVGComponent, string, etc.) — the core has no icon dependency.
 *
 * Containment is controlled by:
 *   1. `category` + registry CategoryRules  (coarse-grained, declarative)
 *   2. `extraChildTypes` / `extraParentTypes` (per-type fine-grained overrides)
 *
 * Field definitions (`fields`) describe the entity's payload schema. Additional
 * fields can be contributed by Lens slot registrations without modifying the
 * base EntityDef (see ObjectRegistry.registerSlot).
 */
export interface EntityDef<TIcon = unknown> {
  /** Unique type identifier — stored in GraphObject.type */
  type: string;

  /**
   * Globally stable NSID in reverse-DNS format for cross-Node interoperability.
   * When two Nodes register the same NSID, they share a type vocabulary.
   * Optional — types without an NSID are Node-local.
   *
   * @example 'io.prismapp.productivity.task'
   */
  nsid?: string;

  /** Category membership — primary mechanism for containment rules */
  category: string;

  label: string;
  pluralLabel?: string;
  description?: string;

  /** Injected by consumer — no icon library in core */
  icon?: TIcon;
  color?: string;

  /** Default view when rendering children of this type */
  defaultChildView?: "list" | "kanban" | "grid" | "timeline" | "graph";

  /** Base tabs shown in the object detail view.
   *  Effective tabs = these + slot-contributed tabs (see ObjectRegistry.getEffectiveTabs) */
  tabs?: TabDefinition[];

  // ── Containment overrides ──────────────────────────────────────────────────

  /** This type may never be a root-level object — must always have a parent */
  childOnly?: boolean;

  /** Additional child types beyond what category rules permit */
  extraChildTypes?: string[];

  /** Additional parent types beyond what category rules permit */
  extraParentTypes?: string[];

  /**
   * Base field definitions for this entity's payload schema.
   * Additional fields can be contributed by Lens slot registrations
   * (see ObjectRegistry.registerSlot) without modifying this def.
   */
  fields?: EntityFieldDef[];

  /**
   * API configuration for automatic REST route generation.
   * When present, the server factory generates CRUD routes for this type.
   * Types without `api` are not exposed via REST.
   */
  api?: ObjectTypeApiConfig;
}

// ── API Config ────────────────────────────────────────────────────────────────

/**
 * Operations that can be generated as REST routes.
 */
export type ApiOperation =
  | "list"
  | "get"
  | "create"
  | "update"
  | "delete"
  | "restore"
  | "move"
  | "duplicate";

/**
 * API configuration for automatic REST route generation.
 * Attached to EntityDef.api to control which routes the server factory generates.
 */
export interface ObjectTypeApiConfig {
  /** URL path segment (e.g. 'tasks'). Default: type string. */
  path?: string;
  /** Which operations to generate. Default: ['list', 'get', 'create', 'update', 'delete']. */
  operations?: ApiOperation[];
  /** Enable soft-delete. Default: true. */
  softDelete?: boolean;
  /** Include in global search. Default: true. */
  searchable?: boolean;
  /** Shell fields usable as query filters. */
  filterBy?: string[];
  /** Default sort when no sort specified. */
  defaultSort?: { field: string; dir: "asc" | "desc" };
  /** Server-side edge cascade on delete. Default: true. */
  cascadeEdges?: boolean;
  /** Named server-side lifecycle hooks (by name, not by function reference). */
  hooks?: Record<string, string>;
}

// ── Category Rules ─────────────────────────────────────────────────────────────

/**
 * Declares the parenting capability of a category.
 *
 * @example
 *   { category: 'container', canParent: ['container', 'content', 'record'], canBeRoot: true }
 *   { category: 'content',   canParent: [] }  // leaf node
 */
export interface CategoryRule {
  category: string;
  /**
   * Which categories this category's members can contain as children.
   * Empty = leaf node (may still parent via extraChildTypes).
   */
  canParent: string[];
  /** If false, members cannot exist at root level. Default: true. */
  canBeRoot?: boolean;
}

// ── Tabs ───────────────────────────────────────────────────────────────────────

export interface TabDefinition {
  id: string;
  label: string;
  /** If true, this tab is hidden until the object has relevant data */
  dynamic?: boolean;
}
