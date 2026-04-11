/**
 * Weak Reference System — automatic, content-derived cross-object edges.
 *
 * Lenses register WeakRefProviders that declare how their objects reference
 * foreign objects. The WeakRefEngine listens to TreeModel and EdgeModel
 * mutations, calls providers to extract references, and materializes them
 * as `behavior: 'weak'` edges in the EdgeModel.
 *
 * Key properties:
 *   - Edges are COMPUTED, not authored — the engine owns their lifecycle.
 *   - If the reference is removed from the source content, the edge disappears.
 *   - Weak-ref edges cannot be manually created/deleted via normal edge APIs.
 *   - The system is live — changes propagate on every tree mutation.
 *   - Providers can be registered via TypeScript, Luau, or visual builders.
 */

import type { GraphObject, ObjectId, ObjectEdge } from "./types.js";
import { objectId as toObjectId } from "./types.js";
import type { EdgeModel } from "./edge-model.js";
import type { TreeModel } from "./tree-model.js";
import type { ObjectRegistry, TreeNode, WeakRefChildNode } from "./registry.js";

// ── Types ───────────────────────────────────────────────────────────────────────

/**
 * A single extracted reference from a source object to a foreign target.
 */
export interface WeakRefExtraction {
  targetId: ObjectId | string;

  /**
   * Relation label describing the nature of the reference.
   * E.g. 'speaks-in', 'referenced-by', 'triggers', 'assigned-to'.
   */
  relation: string;

  /**
   * Where in the source object the reference occurs.
   * Used by "Go to source" to jump to the exact location.
   */
  location?: {
    field?: string;
    offset?: number;
  };

  /**
   * Edge scope for federation.
   * - `'local'` (default) — target is on this Node. targetId is a bare UUID.
   * - `'federated'` — target is on a remote Node. targetId is a full Prism address.
   */
  scope?: "local" | "federated";
}

/**
 * A Lens declares how its objects reference foreign objects.
 * Providers are called on every tree add/update for matching source types.
 * extractRefs() must be fast and synchronous — no I/O, no network.
 */
export interface WeakRefProvider {
  id: string;
  label?: string;
  sourceTypes: string[];
  extractRefs(object: GraphObject): WeakRefExtraction[];
}

/**
 * A weak-ref child as seen from the TARGET object's perspective.
 */
export interface WeakRefChild {
  object: GraphObject;
  relation: string;
  edgeId: string;
  providerId: string;
  providerLabel: string;
  location?: WeakRefExtraction["location"];
}

// ── Internal: tag for engine-managed edges ───────────────────────────────────

const WEAK_REF_TAG = "__weakRef" as const;

interface WeakRefEdgeData extends Record<string, unknown> {
  __weakRef: true;
  providerId: string;
  providerLabel: string;
  location?: WeakRefExtraction["location"];
}

function isWeakRefEdge(edge: ObjectEdge): boolean {
  return (edge.data as Record<string, unknown>)[WEAK_REF_TAG] === true;
}

// ── WeakRefEngine ───────────────────────────────────────────────────────────────

export type WeakRefEngineEvent =
  | { kind: "recomputed"; objectId: ObjectId; added: number; removed: number }
  | { kind: "rebuilt"; totalEdges: number }
  | { kind: "change" };

export type WeakRefEngineEventListener = (event: WeakRefEngineEvent) => void;

export interface WeakRefEngineOptions {
  tree: TreeModel;
  edges: EdgeModel;
  registry?: ObjectRegistry;
}

export class WeakRefEngine {
  private readonly tree: TreeModel;
  private readonly edges: EdgeModel;
  private readonly registry: ObjectRegistry | undefined;
  private readonly providers = new Map<string, WeakRefProvider>();
  private readonly listeners = new Set<WeakRefEngineEventListener>();

  private unsubTree: (() => void) | null = null;
  private active = false;

  constructor(options: WeakRefEngineOptions) {
    this.tree = options.tree;
    this.edges = options.edges;
    this.registry = options.registry;
  }

  // ── Provider registration ──────────────────────────────────────────────────

  registerProvider(provider: WeakRefProvider): this {
    this.providers.set(provider.id, provider);
    if (this.active) {
      for (const obj of this.tree.toArray()) {
        if (this.matchesProvider(provider, obj)) {
          this.recompute(obj.id);
        }
      }
    }
    return this;
  }

  unregisterProvider(id: string): this {
    const provider = this.providers.get(id);
    if (!provider) return this;
    this.providers.delete(id);
    if (this.active) {
      for (const edge of this.edges.getAll()) {
        if (
          isWeakRefEdge(edge) &&
          (edge.data as unknown as WeakRefEdgeData).providerId === id
        ) {
          this.edges.remove(edge.id);
        }
      }
    }
    return this;
  }

  getProvider(id: string): WeakRefProvider | undefined {
    return this.providers.get(id);
  }

  allProviders(): WeakRefProvider[] {
    return [...this.providers.values()];
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  start(): this {
    if (this.active) return this;
    this.active = true;

    this.unsubTree = this.tree.on((event) => {
      switch (event.kind) {
        case "add":
          this.recompute(event.object.id);
          break;
        case "update":
          this.recompute(event.object.id);
          break;
        case "remove":
          this.removeFor(event.object.id);
          for (const desc of event.descendants) {
            this.removeFor(desc.id);
          }
          break;
      }
    });

    return this;
  }

  stop(): this {
    this.active = false;
    this.unsubTree?.();
    this.unsubTree = null;
    return this;
  }

  dispose(): void {
    this.stop();
    this.providers.clear();
    this.listeners.clear();
  }

  // ── Core operations ────────────────────────────────────────────────────────

  recompute(objectId: ObjectId | string): void {
    const obj = this.tree.get(objectId);
    if (!obj) return;

    const desiredRefs: Array<
      WeakRefExtraction & { providerId: string; providerLabel: string }
    > = [];
    for (const provider of this.providers.values()) {
      if (!this.matchesProvider(provider, obj)) continue;
      try {
        const refs = provider.extractRefs(obj);
        for (const ref of refs) {
          desiredRefs.push({
            ...ref,
            providerId: provider.id,
            providerLabel: provider.label ?? provider.id,
          });
        }
      } catch {
        // Provider threw — skip silently
      }
    }

    const existingEdges = this.edges.getFrom(objectId).filter(isWeakRefEdge);

    const refKey = (
      targetId: string,
      relation: string,
      providerId: string,
    ) => `${targetId}::${relation}::${providerId}`;

    const desiredSet = new Map(
      desiredRefs.map((r) => [refKey(r.targetId, r.relation, r.providerId), r]),
    );
    const existingSet = new Map(
      existingEdges.map((e) => {
        const data = e.data as unknown as WeakRefEdgeData;
        return [refKey(e.targetId, e.relation, data.providerId), e];
      }),
    );

    let added = 0;
    let removed = 0;

    for (const [key, edge] of existingSet) {
      if (!desiredSet.has(key)) {
        this.edges.remove(edge.id);
        removed++;
      }
    }

    for (const [key, ref] of desiredSet) {
      if (!existingSet.has(key)) {
        const isFederated = ref.scope === "federated";
        if (isFederated || this.tree.has(ref.targetId)) {
          const edgeData: WeakRefEdgeData = {
            [WEAK_REF_TAG]: true,
            providerId: ref.providerId,
            providerLabel: ref.providerLabel,
          };
          if (ref.location) edgeData.location = ref.location;
          if (isFederated) (edgeData as Record<string, unknown>).federated = true;

          this.edges.add({
            sourceId: toObjectId(obj.id),
            targetId: toObjectId(ref.targetId as string),
            relation: ref.relation,
            data: edgeData as Record<string, unknown>,
          });
          added++;
        }
      }
    }

    if (added > 0 || removed > 0) {
      this.emitEvent({
        kind: "recomputed",
        objectId: objectId as ObjectId,
        added,
        removed,
      });
      this.emitEvent({ kind: "change" });
    }
  }

  rebuildAll(): void {
    for (const edge of this.edges.getAll()) {
      if (isWeakRefEdge(edge)) {
        this.edges.remove(edge.id);
      }
    }

    for (const obj of this.tree.toArray()) {
      this.recompute(obj.id);
    }

    const totalEdges = this.edges.getAll().filter(isWeakRefEdge).length;
    this.emitEvent({ kind: "rebuilt", totalEdges });
    this.emitEvent({ kind: "change" });
  }

  removeFor(objectId: ObjectId | string): void {
    const edges = this.edges.getFrom(objectId).filter(isWeakRefEdge);
    for (const edge of edges) {
      this.edges.remove(edge.id);
    }
    const inbound = this.edges.getTo(objectId).filter(isWeakRefEdge);
    for (const edge of inbound) {
      this.edges.remove(edge.id);
    }
  }

  // ── Queries ────────────────────────────────────────────────────────────────

  getWeakRefChildren(targetId: ObjectId | string): WeakRefChild[] {
    const inbound = this.edges.getTo(targetId).filter(isWeakRefEdge);
    const result: WeakRefChild[] = [];

    for (const edge of inbound) {
      const sourceObj = this.tree.get(edge.sourceId);
      if (!sourceObj) continue;

      const data = edge.data as unknown as WeakRefEdgeData;
      result.push({
        object: sourceObj,
        relation: edge.relation,
        edgeId: edge.id,
        providerId: data.providerId,
        providerLabel: data.providerLabel,
        location: data.location,
      });
    }

    return result;
  }

  getWeakRefParents(sourceId: ObjectId | string): WeakRefChild[] {
    const outbound = this.edges.getFrom(sourceId).filter(isWeakRefEdge);
    const result: WeakRefChild[] = [];

    for (const edge of outbound) {
      const targetObj = this.tree.get(edge.targetId);
      if (!targetObj) continue;

      const data = edge.data as unknown as WeakRefEdgeData;
      result.push({
        object: targetObj,
        relation: edge.relation,
        edgeId: edge.id,
        providerId: data.providerId,
        providerLabel: data.providerLabel,
        location: data.location,
      });
    }

    return result;
  }

  augmentTree(roots: TreeNode[]): TreeNode[] {
    const walk = (nodes: TreeNode[]) => {
      for (const node of nodes) {
        const children = this.getWeakRefChildren(node.object.id);
        if (children.length > 0) {
          node.weakRefChildren = children.map(
            (c): WeakRefChildNode => ({
              object: c.object,
              relation: c.relation,
              edgeId: c.edgeId,
              providerId: c.providerId,
              providerLabel: c.providerLabel,
            }),
          );
        }
        walk(node.children);
      }
    };
    walk(roots);
    return roots;
  }

  isWeakRefEdge(edge: ObjectEdge): boolean {
    return isWeakRefEdge(edge);
  }

  get weakRefCount(): number {
    return this.edges.getAll().filter(isWeakRefEdge).length;
  }

  // ── Events ─────────────────────────────────────────────────────────────────

  on(listener: WeakRefEngineEventListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  // ── Internal ───────────────────────────────────────────────────────────────

  private matchesProvider(
    provider: WeakRefProvider,
    obj: GraphObject,
  ): boolean {
    if (provider.sourceTypes.length === 0) return true;
    return provider.sourceTypes.includes(obj.type);
  }

  private emitEvent(event: WeakRefEngineEvent): void {
    for (const listener of this.listeners) listener(event);
  }
}
