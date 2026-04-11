/**
 * @prism/core — TreeClipboard
 *
 * Cut/copy/paste for GraphObject subtrees with edge preservation.
 * Objects are deep-cloned on copy; IDs are remapped on paste to avoid conflicts.
 * Cut = copy + delete sources on paste.
 *
 * Usage:
 *   const clipboard = createTreeClipboard({ tree, edges, undo });
 *   clipboard.copy(['obj-1', 'obj-2']);
 *   const result = clipboard.paste({ parentId: 'folder-1' });
 */

import type { GraphObject, ObjectEdge } from "@prism/core/object-model";
import { objectId } from "@prism/core/object-model";
import type { TreeModel } from "@prism/core/object-model";
import type { EdgeModel } from "@prism/core/object-model";
import type { UndoRedoManager } from "@prism/core/undo";
import type { ObjectSnapshot } from "@prism/core/undo";

import type {
  SerializedSubtree,
  ClipboardEntry,
  ClipboardMode,
  PasteOptions,
  PasteResult,
} from "./clipboard-types.js";

// ── Options ───────────────────────────────────────────────────────────────────

export interface TreeClipboardOptions {
  tree: TreeModel;
  edges?: EdgeModel;
  undo?: UndoRedoManager;
  generateId?: () => string;
}

// ── Interface ─────────────────────────────────────────────────────────────────

export interface TreeClipboard {
  /** Copy objects to clipboard (deep clone subtrees). */
  copy(ids: string[]): void;
  /** Cut objects to clipboard (will delete sources on paste). */
  cut(ids: string[]): void;
  /** Paste clipboard contents under a target parent. */
  paste(options?: PasteOptions): PasteResult | null;
  /** Whether the clipboard has content. */
  readonly hasContent: boolean;
  /** Current clipboard entry (read-only). */
  readonly entry: ClipboardEntry | null;
  /** Clear clipboard contents. */
  clear(): void;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function defaultIdGenerator(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 9)}`;
}

function serializeSubtree(
  rootId: string,
  tree: TreeModel,
  edges?: EdgeModel,
): SerializedSubtree | null {
  const root = tree.get(rootId);
  if (!root) return null;

  const descendants = tree.getDescendants(rootId);
  const allIds = new Set([rootId, ...descendants.map((d) => d.id as string)]);

  // Collect edges where both endpoints are within the subtree
  const internalEdges: ObjectEdge[] = [];
  if (edges) {
    for (const id of allIds) {
      for (const edge of edges.getFrom(id)) {
        if (allIds.has(edge.targetId as string)) {
          internalEdges.push(structuredClone(edge));
        }
      }
    }
  }

  return {
    root: structuredClone(root),
    descendants: descendants.map((d) => structuredClone(d)),
    internalEdges,
  };
}

// ── Factory ───────────────────────────────────────────────────────────────────

export function createTreeClipboard(
  options: TreeClipboardOptions,
): TreeClipboard {
  const { tree, edges, undo, generateId = defaultIdGenerator } = options;
  let current: ClipboardEntry | null = null;

  function setClipboard(mode: ClipboardMode, ids: string[]): void {
    const subtrees: SerializedSubtree[] = [];
    for (const id of ids) {
      const subtree = serializeSubtree(id, tree, edges);
      if (subtree) subtrees.push(subtree);
    }
    if (subtrees.length === 0) return;

    current = {
      mode,
      subtrees,
      sourceIds: ids.filter((id) => tree.has(id)),
      timestamp: Date.now(),
    };
  }

  function copy(ids: string[]): void {
    setClipboard("copy", ids);
  }

  function cut(ids: string[]): void {
    setClipboard("cut", ids);
  }

  function paste(pasteOptions: PasteOptions = {}): PasteResult | null {
    if (!current || current.subtrees.length === 0) return null;

    const { parentId = null, position } = pasteOptions;
    const allCreated: GraphObject[] = [];
    const allCreatedEdges: ObjectEdge[] = [];
    const globalIdMap = new Map<string, string>();
    const snapshots: ObjectSnapshot[] = [];

    let insertPos = position;

    for (const subtree of current.subtrees) {
      const idMap = new Map<string, string>();

      // Generate new IDs for all objects in the subtree
      const allObjects = [subtree.root, ...subtree.descendants];
      for (const obj of allObjects) {
        idMap.set(obj.id, generateId());
      }

      // Remap and add root
      const rootNewId = idMap.get(subtree.root.id) ?? generateId();
      const rootObj = tree.add(
        {
          type: subtree.root.type,
          name: subtree.root.name,
          id: objectId(rootNewId),
          status: subtree.root.status,
          tags: [...subtree.root.tags],
          date: subtree.root.date,
          endDate: subtree.root.endDate,
          description: subtree.root.description,
          color: subtree.root.color,
          image: subtree.root.image,
          pinned: subtree.root.pinned,
          data: structuredClone(subtree.root.data),
        },
        { parentId, ...(insertPos !== undefined ? { position: insertPos } : {}) },
      );
      allCreated.push(rootObj);
      snapshots.push({
        kind: "object",
        before: null,
        after: structuredClone(rootObj),
      });

      if (insertPos !== undefined) insertPos++;

      // Add descendants in depth-first order, preserving tree structure
      const addDescendants = (
        originalParentId: string,
        newParentId: string,
      ): void => {
        const children = subtree.descendants
          .filter((d) => d.parentId === originalParentId)
          .sort((a, b) => a.position - b.position);

        for (const child of children) {
          const childNewId = idMap.get(child.id) ?? generateId();
          const childObj = tree.add(
            {
              type: child.type,
              name: child.name,
              id: objectId(childNewId),
              status: child.status,
              tags: [...child.tags],
              date: child.date,
              endDate: child.endDate,
              description: child.description,
              color: child.color,
              image: child.image,
              pinned: child.pinned,
              data: structuredClone(child.data),
            },
            { parentId: newParentId },
          );
          allCreated.push(childObj);
          snapshots.push({
            kind: "object",
            before: null,
            after: structuredClone(childObj),
          });
          addDescendants(child.id, childNewId);
        }
      };

      addDescendants(subtree.root.id, rootNewId);

      // Remap and add internal edges
      if (edges) {
        for (const edge of subtree.internalEdges) {
          const newSourceId = idMap.get(edge.sourceId as string);
          const newTargetId = idMap.get(edge.targetId as string);
          if (newSourceId && newTargetId) {
            const newEdge = edges.add({
              sourceId: objectId(newSourceId),
              targetId: objectId(newTargetId),
              relation: edge.relation,
              data: structuredClone(edge.data),
            });
            allCreatedEdges.push(newEdge);
            snapshots.push({
              kind: "edge",
              before: null,
              after: structuredClone(newEdge),
            });
          }
        }
      }

      // Merge into global map
      for (const [oldId, newId] of idMap) {
        globalIdMap.set(oldId, newId);
      }
    }

    // Handle cut: delete source objects
    if (current.mode === "cut") {
      for (const sourceId of current.sourceIds) {
        const source = tree.get(sourceId);
        if (source) {
          const descendants = tree.getDescendants(sourceId);
          for (const d of [source, ...descendants]) {
            snapshots.push({
              kind: "object",
              before: structuredClone(d),
              after: null,
            });
          }
          tree.remove(sourceId);
        }
      }
      // Cut is one-time — clear clipboard after paste
      current = null;
    }

    // Push single undo entry
    if (undo && snapshots.length > 0) {
      undo.push("Paste", snapshots);
    }

    return {
      created: allCreated,
      createdEdges: allCreatedEdges,
      idMap: globalIdMap,
    };
  }

  function clear(): void {
    current = null;
  }

  return {
    copy,
    cut,
    paste,
    get hasContent() {
      return current !== null && current.subtrees.length > 0;
    },
    get entry() {
      return current;
    },
    clear,
  };
}
