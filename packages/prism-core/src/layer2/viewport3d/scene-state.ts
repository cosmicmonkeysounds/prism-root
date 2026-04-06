/**
 * Loro-backed 3D scene state.
 *
 * The scene graph lives in a Loro CRDT document so every transform,
 * material change, and hierarchy edit is automatically mergeable
 * across peers.  This module is the sole writer; R3F components
 * read the projected state.
 *
 * Storage strategy: each node/material is stored as a JSON string
 * value in a top-level LoroMap.  This avoids nested container
 * complexity while still getting CRDT last-writer-wins on each
 * key.  For finer-grained field-level merge, a future iteration
 * can switch to nested LoroMaps with `.toJSON()` on read.
 */

import { LoroDoc, LoroMap, LoroList } from "loro-crdt";
import type {
  SceneNode,
  SceneGraph,
  MaterialDef,
  Transform,
  GeometryParams,
  LightParams,
  CameraParams,
} from "./viewport3d-types.js";
import {
  DEFAULT_MATERIAL,
} from "./viewport3d-types.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

let counter = 0;
function uid(): string {
  return `sn_${Date.now().toString(36)}_${(counter++).toString(36)}`;
}

function serializeNode(node: SceneNode): string {
  return JSON.stringify(node);
}

function deserializeNode(json: string): SceneNode {
  const raw = JSON.parse(json);
  const base = {
    id: raw.id as string,
    name: raw.name as string,
    kind: raw.kind as SceneNode["kind"],
    transform: {
      position: raw.transform.position,
      rotation: raw.transform.rotation,
      scale: raw.transform.scale,
    } as Transform,
    parentId: (raw.parentId as string | null) ?? null,
    visible: raw.visible as boolean,
    locked: raw.locked as boolean,
  };
  return Object.assign(
    base,
    raw.geometry != null ? { geometry: raw.geometry as GeometryParams } : {},
    raw.materialId != null ? { materialId: raw.materialId as string } : {},
    raw.light != null ? { light: raw.light as LightParams } : {},
    raw.camera != null ? { camera: raw.camera as CameraParams } : {},
  ) as SceneNode;
}

function serializeMaterial(mat: MaterialDef): string {
  return JSON.stringify(mat);
}

function deserializeMaterial(json: string): MaterialDef {
  return JSON.parse(json);
}

// ---------------------------------------------------------------------------
// SceneState
// ---------------------------------------------------------------------------

export type SceneStateListener = (graph: SceneGraph) => void;

export type SceneState = {
  getGraph(): SceneGraph;
  addNode(partial: Partial<SceneNode> & Pick<SceneNode, "kind">): string;
  removeNode(id: string): void;
  updateNode(id: string, patch: Partial<Omit<SceneNode, "id">>): void;
  setTransform(id: string, transform: Transform): void;
  reparent(id: string, newParentId: string | null): void;
  addMaterial(partial?: Partial<MaterialDef>): string;
  removeMaterial(id: string): void;
  updateMaterial(id: string, patch: Partial<Omit<MaterialDef, "id">>): void;
  subscribe(listener: SceneStateListener): () => void;
  getDoc(): LoroDoc;
  getChildren(parentId: string): readonly string[];
  getDescendants(id: string): readonly string[];
  exportState(): Uint8Array;
  importState(data: Uint8Array): void;
};

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

export function createSceneState(existingDoc?: LoroDoc): SceneState {
  const doc = existingDoc ?? new LoroDoc();
  const listeners = new Set<SceneStateListener>();

  function nodesMap(): LoroMap {
    return doc.getMap("nodes");
  }
  function materialsMap(): LoroMap {
    return doc.getMap("materials");
  }
  function rootList(): LoroList {
    return doc.getList("rootIds");
  }

  // ------ projection ------

  function project(): SceneGraph {
    const nm = nodesMap();
    const mm = materialsMap();
    const rl = rootList();

    const nodes = new Map<string, SceneNode>();
    for (const key of nm.keys()) {
      const raw = nm.get(key);
      if (typeof raw === "string") {
        nodes.set(key, deserializeNode(raw));
      }
    }

    const materials = new Map<string, MaterialDef>();
    for (const key of mm.keys()) {
      const raw = mm.get(key);
      if (typeof raw === "string") {
        materials.set(key, deserializeMaterial(raw));
      }
    }

    const rootIds: string[] = [];
    for (let i = 0; i < rl.length; i++) {
      const v = rl.get(i);
      if (typeof v === "string") rootIds.push(v);
    }

    return { nodes, materials, rootIds };
  }

  function getNodeDirect(id: string): SceneNode | undefined {
    const raw = nodesMap().get(id);
    if (typeof raw === "string") return deserializeNode(raw);
    return undefined;
  }

  function notify(): void {
    const graph = project();
    for (const listener of listeners) {
      listener(graph);
    }
  }

  // ------ mutations ------

  function addNode(partial: Partial<SceneNode> & Pick<SceneNode, "kind">): string {
    const id = partial.id ?? uid();
    const node: SceneNode = Object.assign(
      {
        id,
        name: partial.name ?? partial.kind,
        kind: partial.kind,
        transform: partial.transform ?? { position: [0, 0, 0], rotation: [0, 0, 0, "XYZ"], scale: [1, 1, 1] } as Transform,
        parentId: partial.parentId ?? null,
        visible: partial.visible ?? true,
        locked: partial.locked ?? false,
      },
      partial.geometry !== undefined ? { geometry: partial.geometry } : {},
      partial.materialId !== undefined ? { materialId: partial.materialId } : {},
      partial.light !== undefined ? { light: partial.light } : {},
      partial.camera !== undefined ? { camera: partial.camera } : {},
    ) as SceneNode;

    nodesMap().set(id, serializeNode(node));

    if (node.parentId === null) {
      rootList().push(id);
    }

    doc.commit();
    notify();
    return id;
  }

  function getChildren(parentId: string): readonly string[] {
    const graph = project();
    const children: string[] = [];
    for (const [, node] of graph.nodes) {
      if (node.parentId === parentId) children.push(node.id);
    }
    return children;
  }

  function getDescendants(id: string): readonly string[] {
    const result: string[] = [];
    const stack = [id];
    while (stack.length > 0) {
      const current = stack.pop();
      if (current === undefined) break;
      const children = getChildren(current);
      for (const child of children) {
        result.push(child);
        stack.push(child);
      }
    }
    return result;
  }

  function removeNode(id: string): void {
    const descendants = getDescendants(id);
    const toRemove = [id, ...descendants];

    for (const rid of toRemove) {
      nodesMap().delete(rid);
    }

    const rl = rootList();
    for (let i = rl.length - 1; i >= 0; i--) {
      if (toRemove.includes(rl.get(i) as string)) {
        rl.delete(i, 1);
      }
    }

    doc.commit();
    notify();
  }

  function updateNode(id: string, patch: Partial<Omit<SceneNode, "id">>): void {
    const existing = getNodeDirect(id);
    if (!existing) return;

    const updated: SceneNode = { ...existing, ...patch, id };
    nodesMap().set(id, serializeNode(updated));

    doc.commit();
    notify();
  }

  function setTransform(id: string, transform: Transform): void {
    updateNode(id, { transform });
  }

  function reparent(id: string, newParentId: string | null): void {
    const existing = getNodeDirect(id);
    if (!existing) return;
    const oldParentId = existing.parentId;

    updateNode(id, { parentId: newParentId });

    const rl = rootList();
    if (oldParentId === null && newParentId !== null) {
      for (let i = rl.length - 1; i >= 0; i--) {
        if (rl.get(i) === id) {
          rl.delete(i, 1);
          break;
        }
      }
    } else if (oldParentId !== null && newParentId === null) {
      rl.push(id);
    }

    doc.commit();
    notify();
  }

  function addMaterial(partial?: Partial<MaterialDef>): string {
    const id = partial?.id ?? `mat_${uid()}`;
    const mat: MaterialDef = {
      ...DEFAULT_MATERIAL,
      ...partial,
      id,
    };

    materialsMap().set(id, serializeMaterial(mat));
    doc.commit();
    notify();
    return id;
  }

  function removeMaterial(id: string): void {
    materialsMap().delete(id);
    doc.commit();
    notify();
  }

  function updateMaterial(id: string, patch: Partial<Omit<MaterialDef, "id">>): void {
    const raw = materialsMap().get(id);
    if (typeof raw !== "string") return;
    const existing = deserializeMaterial(raw);
    const updated: MaterialDef = { ...existing, ...patch, id };

    materialsMap().set(id, serializeMaterial(updated));
    doc.commit();
    notify();
  }

  return {
    getGraph: project,
    addNode,
    removeNode,
    updateNode,
    setTransform,
    reparent,
    addMaterial,
    removeMaterial,
    updateMaterial,
    subscribe(listener) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    getDoc: () => doc,
    getChildren,
    getDescendants,
    exportState: () => doc.export({ mode: "snapshot" }),
    importState(data) {
      doc.import(data);
      notify();
    },
  };
}
