/**
 * @prism/core — Template Registry
 *
 * Catalog of reusable ObjectTemplates. Supports:
 *   - Register/unregister templates by ID
 *   - Filter by category, root type, or search string
 *   - Instantiate a template into live objects in a TreeModel
 *   - Create a template from an existing object subtree
 *   - Variable interpolation: {{name}}, {{date}}, etc.
 *
 * Usage:
 *   const registry = createTemplateRegistry({ tree, edges, undo });
 *   registry.register(myTemplate);
 *   const result = registry.instantiate('tpl-1', {
 *     variables: { name: 'Sprint 23', date: '2026-04-01' },
 *     parentId: 'project-1',
 *   });
 */

import type { GraphObject, ObjectEdge } from "../object-model/types.js";
import { objectId } from "../object-model/types.js";
import type { TreeModel } from "../object-model/tree-model.js";
import type { EdgeModel } from "../object-model/edge-model.js";
import type { UndoRedoManager } from "../undo/undo-manager.js";
import type { ObjectSnapshot } from "../undo/undo-types.js";

import type {
  ObjectTemplate,
  TemplateNode,
  TemplateEdge,
  TemplateFilter,
  InstantiateOptions,
  InstantiateResult,
} from "./template-types.js";

// ── Options ───────────────────────────────────────────────────────────────────

export interface TemplateRegistryOptions {
  tree: TreeModel;
  edges?: EdgeModel;
  undo?: UndoRedoManager;
  generateId?: () => string;
}

// ── Interface ─────────────────────────────────────────────────────────────────

export interface TemplateRegistry {
  /** Register a template. Overwrites if same ID exists. */
  register(template: ObjectTemplate): void;
  /** Remove a template by ID. */
  unregister(id: string): boolean;
  /** Get a template by ID. */
  get(id: string): ObjectTemplate | undefined;
  /** Check if a template exists. */
  has(id: string): boolean;
  /** Get all templates, optionally filtered. */
  list(filter?: TemplateFilter): ObjectTemplate[];
  /** Number of registered templates. */
  readonly size: number;
  /** Instantiate a template into live objects. */
  instantiate(
    templateId: string,
    options?: InstantiateOptions,
  ): InstantiateResult;
  /** Create a template from an existing object subtree. */
  createFromObject(
    objectId: string,
    meta: { id: string; name: string; description?: string; category?: string },
  ): ObjectTemplate;
}

// ── Variable interpolation ────────────────────────────────────────────────────

const VARIABLE_RE = /\{\{(\w+)\}\}/g;

function interpolateString(
  template: string,
  vars: Record<string, string>,
): string {
  return template.replace(VARIABLE_RE, (match, name: string) => {
    return name in vars ? (vars[name] ?? match) : match;
  });
}

function interpolateData(
  data: Record<string, unknown>,
  vars: Record<string, string>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(data)) {
    if (typeof value === "string") {
      result[key] = interpolateString(value, vars);
    } else {
      result[key] = value;
    }
  }
  return result;
}

// ── Default ID generator ──────────────────────────────────────────────────────

function defaultIdGenerator(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 9)}`;
}

// ── Factory ───────────────────────────────────────────────────────────────────

export function createTemplateRegistry(
  options: TemplateRegistryOptions,
): TemplateRegistry {
  const { tree, edges, undo, generateId = defaultIdGenerator } = options;
  const templates = new Map<string, ObjectTemplate>();

  function register(template: ObjectTemplate): void {
    templates.set(template.id, template);
  }

  function unregister(id: string): boolean {
    return templates.delete(id);
  }

  function get(id: string): ObjectTemplate | undefined {
    return templates.get(id);
  }

  function has(id: string): boolean {
    return templates.has(id);
  }

  function list(filter?: TemplateFilter): ObjectTemplate[] {
    let result = [...templates.values()];

    if (filter?.category) {
      result = result.filter((t) => t.category === filter.category);
    }
    if (filter?.type) {
      result = result.filter((t) => t.root.type === filter.type);
    }
    if (filter?.search) {
      const lower = filter.search.toLowerCase();
      result = result.filter(
        (t) =>
          t.name.toLowerCase().includes(lower) ||
          (t.description?.toLowerCase().includes(lower) ?? false),
      );
    }

    return result;
  }

  function instantiate(
    templateId: string,
    instOptions: InstantiateOptions = {},
  ): InstantiateResult {
    const template = templates.get(templateId);
    if (!template) {
      throw new Error(`Template '${templateId}' not found`);
    }

    const vars = instOptions.variables ?? {};
    const parentId = instOptions.parentId ?? null;
    const position = instOptions.position;

    // Build placeholder -> real ID map
    const idMap = new Map<string, string>();
    const collectPlaceholders = (node: TemplateNode): void => {
      idMap.set(node.placeholderId, generateId());
      if (node.children) {
        for (const child of node.children) {
          collectPlaceholders(child);
        }
      }
    };
    collectPlaceholders(template.root);

    const created: GraphObject[] = [];
    const createdEdges: ObjectEdge[] = [];
    const snapshots: ObjectSnapshot[] = [];

    // Recursive instantiation
    const instantiateNode = (
      node: TemplateNode,
      targetParentId: string | null,
      pos?: number,
    ): void => {
      const realId = idMap.get(node.placeholderId)!;
      const obj = tree.add(
        {
          id: objectId(realId),
          type: node.type,
          name: interpolateString(node.name, vars),
          status: node.status != null ? interpolateString(node.status, vars) : null,
          tags: node.tags ?? [],
          description: node.description
            ? interpolateString(node.description, vars)
            : "",
          color: node.color ?? null,
          pinned: node.pinned ?? false,
          data: node.data ? interpolateData(node.data, vars) : {},
        },
        { parentId: targetParentId, ...(pos !== undefined ? { position: pos } : {}) },
      );
      created.push(obj);
      snapshots.push({
        kind: "object",
        before: null,
        after: structuredClone(obj),
      });

      // Add children in order
      if (node.children) {
        for (const child of node.children) {
          instantiateNode(child, realId);
        }
      }
    };

    instantiateNode(template.root, parentId, position);

    // Create edges
    if (edges && template.edges) {
      for (const edgeTpl of template.edges) {
        const sourceRealId = idMap.get(edgeTpl.sourcePlaceholderId);
        const targetRealId = idMap.get(edgeTpl.targetPlaceholderId);
        if (sourceRealId && targetRealId) {
          const edge = edges.add({
            sourceId: objectId(sourceRealId),
            targetId: objectId(targetRealId),
            relation: edgeTpl.relation,
            data: edgeTpl.data ? interpolateData(edgeTpl.data, vars) : {},
          });
          createdEdges.push(edge);
          snapshots.push({
            kind: "edge",
            before: null,
            after: structuredClone(edge),
          });
        }
      }
    }

    // Single undo entry
    if (undo && snapshots.length > 0) {
      undo.push(`Instantiate template "${template.name}"`, snapshots);
    }

    return { created, createdEdges, idMap };
  }

  function createFromObject(
    rootObjectId: string,
    meta: { id: string; name: string; description?: string; category?: string },
  ): ObjectTemplate {
    const root = tree.get(rootObjectId);
    if (!root) throw new Error(`Object '${rootObjectId}' not found`);

    const placeholderMap = new Map<string, string>();
    let placeholderCounter = 0;
    const getPlaceholder = (id: string): string => {
      if (!placeholderMap.has(id)) {
        placeholderMap.set(id, `placeholder-${++placeholderCounter}`);
      }
      return placeholderMap.get(id)!;
    };

    const buildNode = (obj: GraphObject): TemplateNode => {
      const children = tree.getChildren(obj.id);
      const node: TemplateNode = {
        placeholderId: getPlaceholder(obj.id),
        type: obj.type,
        name: obj.name,
        status: obj.status,
        tags: obj.tags.length > 0 ? [...obj.tags] : undefined,
        description: obj.description || undefined,
        color: obj.color,
        pinned: obj.pinned || undefined,
        data:
          Object.keys(obj.data).length > 0
            ? structuredClone(obj.data)
            : undefined,
        children:
          children.length > 0 ? children.map(buildNode) : undefined,
      };
      return node;
    };

    const templateRoot = buildNode(root);

    // Collect internal edges
    const allIds = new Set([
      rootObjectId,
      ...tree.getDescendants(rootObjectId).map((d) => d.id as string),
    ]);
    const templateEdges: TemplateEdge[] = [];
    if (edges) {
      for (const id of allIds) {
        for (const edge of edges.getFrom(id)) {
          if (allIds.has(edge.targetId as string)) {
            templateEdges.push({
              sourcePlaceholderId: getPlaceholder(edge.sourceId as string),
              targetPlaceholderId: getPlaceholder(edge.targetId as string),
              relation: edge.relation,
              data:
                Object.keys(edge.data).length > 0
                  ? structuredClone(edge.data)
                  : undefined,
            });
          }
        }
      }
    }

    const template: ObjectTemplate = {
      id: meta.id,
      name: meta.name,
      description: meta.description,
      category: meta.category,
      root: templateRoot,
      edges: templateEdges.length > 0 ? templateEdges : undefined,
      createdAt: new Date().toISOString(),
    };

    return template;
  }

  return {
    register,
    unregister,
    get,
    has,
    list,
    get size() {
      return templates.size;
    },
    instantiate,
    createFromObject,
  };
}
