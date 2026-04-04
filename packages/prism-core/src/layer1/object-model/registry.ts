/**
 * ObjectRegistry — runtime registry of entity/edge type definitions, category
 * rules, and Lens slot registrations.
 *
 * The registry is the single source of truth for:
 *   - What entity types exist (registered by the Lens or application)
 *   - What each type looks like (label, icon, color, tabs, fields)
 *   - What containment is valid (category rules + per-type overrides)
 *   - What Lens slots have been registered (view panels + field extensions)
 *
 * No type strings are hardcoded here. All containment logic is derived
 * from what the consumer has registered.
 */

import type {
  GraphObject,
  EntityDef,
  EdgeTypeDef,
  EntityFieldDef,
  CategoryRule,
  TabDefinition,
} from "./types.js";

// ── Slot types ─────────────────────────────────────────────────────────────────

/**
 * A Lens's contribution to an existing entity type.
 * Slots are additive — they extend without modifying the base EntityDef.
 *
 * Field IDs contributed by a Lens should be namespaced to avoid collisions:
 *   'kami_brain_state', 'crm_deal_stage', 'palette_inventory_capacity'
 */
export interface SlotDef {
  /**
   * Unique slot identifier — use 'lensId:slotName' convention.
   * e.g. 'kami:brain', 'palette:inventory', 'crm:pipeline'
   */
  id: string;

  description?: string;

  /** Tabs/panels this slot contributes to the entity's detail view */
  tabs?: TabDefinition[];

  /**
   * Fields this slot contributes to the entity's payload schema.
   * Use namespaced IDs to prevent collisions with base fields and other slots.
   */
  fields?: EntityFieldDef[];
}

/**
 * Registers a SlotDef against one or more entity types / categories.
 * Both forTypes and forCategories are OR'd — a slot applies to any type
 * that matches either list.
 */
export interface SlotRegistration {
  slot: SlotDef;
  /** Specific entity types this slot applies to */
  forTypes?: string[];
  /** Entity categories whose members this slot applies to */
  forCategories?: string[];
}

// ── TreeNode ───────────────────────────────────────────────────────────────────

export interface TreeNode {
  object: GraphObject;
  children: TreeNode[];
  depth?: number;

  /**
   * Virtual children — objects from OTHER Lenses that reference this object.
   * Populated by WeakRefEngine, not by tree hierarchy.
   * These are read-only: cannot be moved, reparented, or deleted from here.
   */
  weakRefChildren?: WeakRefChildNode[];
}

/**
 * A virtual child node in the tree, derived from a weak-ref edge.
 * Displayed with distinct styling (dimmed, dashed border, link icon).
 * Non-draggable, non-reorderable.
 */
export interface WeakRefChildNode {
  /** The foreign object that references the parent. */
  object: GraphObject;
  /** Edge relation label (e.g. 'speaks-in', 'referenced-by'). */
  relation: string;
  /** Edge ID in the EdgeModel. */
  edgeId: string;
  /** Provider that created this ref (e.g. 'loom'). */
  providerId: string;
  /** Provider display label (e.g. 'Loom'). */
  providerLabel: string;
}

// ── ObjectRegistry ─────────────────────────────────────────────────────────────

export class ObjectRegistry<TIcon = unknown> {
  private readonly types = new Map<string, EntityDef<TIcon>>();
  private readonly rules = new Map<string, CategoryRule>();
  private readonly edgeTypes = new Map<string, EdgeTypeDef>();
  private readonly slots: SlotRegistration[] = [];

  constructor(categoryRules: CategoryRule[] = []) {
    for (const rule of categoryRules) {
      this.rules.set(rule.category, rule);
    }
  }

  // ── Entity Type Registration ─────────────────────────────────────────────────

  register(def: EntityDef<TIcon>): this {
    this.types.set(def.type, def);
    return this;
  }

  registerAll(defs: EntityDef<TIcon>[]): this {
    for (const def of defs) this.register(def);
    return this;
  }

  addCategoryRule(rule: CategoryRule): this {
    this.rules.set(rule.category, rule);
    return this;
  }

  // ── Slot Registration ────────────────────────────────────────────────────────

  /**
   * Register a Lens slot that extends one or more entity types.
   * Slots are additive — they never modify the base EntityDef.
   */
  registerSlot(registration: SlotRegistration): this {
    this.slots.push(registration);
    return this;
  }

  /**
   * All slot definitions that apply to a given entity type.
   * A slot applies if the type matches forTypes OR if the type's category
   * matches forCategories.
   */
  getSlots(type: string): SlotDef[] {
    const category = this.getCategory(type);
    return this.slots
      .filter(
        (r) =>
          (r.forTypes?.includes(type) ?? false) ||
          (r.forCategories?.includes(category) ?? false),
      )
      .map((r) => r.slot);
  }

  /**
   * Effective tabs for a type = base EntityDef tabs + all slot-contributed tabs.
   * Base tabs win on id collision (slots cannot override base tabs).
   * Falls back to a minimal default if neither the EntityDef nor any slot defines tabs.
   */
  getEffectiveTabs(type: string): TabDefinition[] {
    const base = this.types.get(type)?.tabs ?? [
      { id: "overview", label: "Overview" },
      { id: "children", label: "Children", dynamic: true },
      { id: "linked", label: "Linked", dynamic: true },
    ];
    const slotTabs = this.getSlots(type).flatMap((s) => s.tabs ?? []);
    const seen = new Set(base.map((t) => t.id));
    const merged = [...base];
    for (const t of slotTabs) {
      if (!seen.has(t.id)) {
        seen.add(t.id);
        merged.push(t);
      }
    }
    return merged;
  }

  /**
   * All field definitions for a type = EntityDef.fields + slot-contributed fields.
   * Base fields win on id collision (slots cannot override base fields).
   */
  getEntityFields(type: string): EntityFieldDef[] {
    const base = this.types.get(type)?.fields ?? [];
    const slotFields = this.getSlots(type).flatMap((s) => s.fields ?? []);
    const seen = new Set(base.map((f) => f.id));
    const merged = [...base];
    for (const f of slotFields) {
      if (!seen.has(f.id)) {
        seen.add(f.id);
        merged.push(f);
      }
    }
    return merged;
  }

  // ── Edge Type Registration ───────────────────────────────────────────────────

  registerEdge(def: EdgeTypeDef): this {
    this.edgeTypes.set(def.relation, def);
    return this;
  }

  registerEdges(defs: EdgeTypeDef[]): this {
    for (const def of defs) this.registerEdge(def);
    return this;
  }

  getEdgeType(relation: string): EdgeTypeDef | undefined {
    return this.edgeTypes.get(relation);
  }

  getEdgeLabel(relation: string): string {
    return this.edgeTypes.get(relation)?.label ?? relation;
  }

  allEdgeTypes(): string[] {
    return [...this.edgeTypes.keys()];
  }

  allEdgeDefs(): EdgeTypeDef[] {
    return [...this.edgeTypes.values()];
  }

  /**
   * Can an edge of `relation` type go from an object of `sourceType`
   * to an object of `targetType`?
   *
   * If no edge type is registered, any connection is allowed.
   */
  canConnect(
    relation: string,
    sourceType: string,
    targetType: string,
  ): boolean {
    const edgeDef = this.edgeTypes.get(relation);
    if (!edgeDef) return true;

    if (edgeDef.sourceTypes || edgeDef.sourceCategories) {
      const srcCat = this.getCategory(sourceType);
      const byType = edgeDef.sourceTypes?.includes(sourceType) ?? false;
      const byCat = edgeDef.sourceCategories?.includes(srcCat) ?? false;
      if (!byType && !byCat) return false;
    }

    if (edgeDef.targetTypes || edgeDef.targetCategories) {
      const tgtCat = this.getCategory(targetType);
      const byType = edgeDef.targetTypes?.includes(targetType) ?? false;
      const byCat = edgeDef.targetCategories?.includes(tgtCat) ?? false;
      if (!byType && !byCat) return false;
    }

    return true;
  }

  getEdgesFrom(sourceType: string): EdgeTypeDef[] {
    const cat = this.getCategory(sourceType);
    return this.allEdgeDefs().filter((def) => {
      if (!def.sourceTypes && !def.sourceCategories) return true;
      return (
        (def.sourceTypes?.includes(sourceType) ?? false) ||
        (def.sourceCategories?.includes(cat) ?? false)
      );
    });
  }

  getEdgesTo(targetType: string): EdgeTypeDef[] {
    const cat = this.getCategory(targetType);
    return this.allEdgeDefs().filter((def) => {
      if (!def.targetTypes && !def.targetCategories) return true;
      return (
        (def.targetTypes?.includes(targetType) ?? false) ||
        (def.targetCategories?.includes(cat) ?? false)
      );
    });
  }

  getEdgesBetween(sourceType: string, targetType: string): EdgeTypeDef[] {
    return this.allEdgeDefs().filter((def) =>
      this.canConnect(def.relation, sourceType, targetType),
    );
  }

  // ── Lookup ───────────────────────────────────────────────────────────────────

  get(type: string): EntityDef<TIcon> | undefined {
    return this.types.get(type);
  }

  has(type: string): boolean {
    return this.types.has(type);
  }

  allTypes(): string[] {
    return [...this.types.keys()];
  }

  allDefs(): EntityDef<TIcon>[] {
    return [...this.types.values()];
  }

  getLabel(type: string): string {
    return this.types.get(type)?.label ?? type;
  }

  getPluralLabel(type: string): string {
    const def = this.types.get(type);
    return def?.pluralLabel ?? def?.label ?? type;
  }

  getColor(type: string): string {
    return this.types.get(type)?.color ?? "#888888";
  }

  getIcon(type: string): TIcon | undefined {
    return this.types.get(type)?.icon;
  }

  getCategory(type: string): string {
    return this.types.get(type)?.category ?? "";
  }

  /**
   * Base tabs only — does not include slot contributions.
   * Use getEffectiveTabs() to get the full merged tab list.
   */
  getTabs(type: string): TabDefinition[] {
    return this.types.get(type)?.tabs ?? [
      { id: "overview", label: "Overview" },
      { id: "children", label: "Children", dynamic: true },
      { id: "linked", label: "Linked", dynamic: true },
    ];
  }

  // ── Containment ──────────────────────────────────────────────────────────────

  /**
   * Can an object of `childType` be placed inside `parentType`?
   *
   * Resolution order:
   *   1. parent.extraChildTypes includes childType -> allowed
   *   2. child.extraParentTypes includes parentType -> allowed
   *   3. parent category rule canParent includes child category -> allowed
   *   4. Otherwise -> denied
   */
  canBeChildOf(childType: string, parentType: string): boolean {
    const child = this.types.get(childType);
    const parent = this.types.get(parentType);
    if (!child || !parent) return false;

    if (parent.extraChildTypes?.includes(childType)) return true;
    if (child.extraParentTypes?.includes(parentType)) return true;

    const rule = this.rules.get(parent.category);
    if (rule?.canParent.includes(child.category)) return true;

    return false;
  }

  /**
   * Can this type exist at root level (no parent)?
   * childOnly types are always false.
   * Category canBeRoot: false overrides to false.
   * Default: true.
   */
  canBeRoot(type: string): boolean {
    const def = this.types.get(type);
    if (!def) return false;
    if (def.childOnly) return false;
    const rule = this.rules.get(def.category);
    if (rule?.canBeRoot === false) return false;
    return true;
  }

  /**
   * Can this type have children?
   * true if category rule has at least one canParent entry,
   * or if the type has extraChildTypes defined.
   */
  canHaveChildren(type: string): boolean {
    const def = this.types.get(type);
    if (!def) return false;
    if (def.childOnly) return false;
    if (def.extraChildTypes && def.extraChildTypes.length > 0) return true;
    const rule = this.rules.get(def.category);
    return (rule?.canParent.length ?? 0) > 0;
  }

  /** All registered types that can be placed inside `parentType`. */
  getAllowedChildTypes(parentType: string): string[] {
    return this.allTypes().filter((t) => this.canBeChildOf(t, parentType));
  }

  // ── Tree Utilities ───────────────────────────────────────────────────────────

  buildTree(objects: GraphObject[]): TreeNode[] {
    const nodeMap = new Map<string, TreeNode>();
    for (const obj of objects) {
      nodeMap.set(obj.id, { object: obj, children: [] });
    }
    const roots: TreeNode[] = [];
    for (const node of nodeMap.values()) {
      const parent = node.object.parentId
        ? nodeMap.get(node.object.parentId)
        : null;
      if (parent) {
        parent.children.push(node);
      } else {
        roots.push(node);
      }
    }
    const sortByPosition = (nodes: TreeNode[]): void => {
      nodes.sort((a, b) => a.object.position - b.object.position);
      for (const n of nodes) sortByPosition(n.children);
    };
    sortByPosition(roots);
    return roots;
  }

  getAncestors(
    id: string,
    objectMap: Map<string, GraphObject>,
  ): GraphObject[] {
    const ancestors: GraphObject[] = [];
    let current = objectMap.get(id);
    while (current?.parentId) {
      const parent = objectMap.get(current.parentId);
      if (!parent) break;
      ancestors.push(parent);
      current = parent;
    }
    return ancestors;
  }
}
