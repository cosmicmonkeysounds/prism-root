/**
 * Dependency graph utilities — topological sort, cycle detection,
 * blocking chains, impact analysis.
 *
 * Operates on GraphObject arrays where `data.dependsOn` (or `data.blockedBy`)
 * declares predecessor IDs. The graph is directed: A → B means "A blocks B"
 * (B cannot start until A is done).
 *
 * All functions are pure and side-effect free.
 */

import type { GraphObject } from "../object-model/types.js";

// ── Types ────────────────────────────────────────────────────────────────────

/** Directed adjacency list: key → set of successor IDs it unblocks. */
export type DependencyGraph = Map<string, Set<string>>;

export interface SlipImpact {
  objectId: string;
  objectName: string;
  slipDays: number;
  depth: number;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function getPredecessors(obj: GraphObject): string[] {
  const d = obj.data as Record<string, unknown> | null;
  const dependsOn = Array.isArray(d?.dependsOn)
    ? (d.dependsOn as string[])
    : [];
  const blockedBy = Array.isArray(d?.blockedBy)
    ? (d.blockedBy as string[])
    : [];
  return [...new Set([...dependsOn, ...blockedBy])];
}

// ── Graph construction ───────────────────────────────────────────────────────

/** Build a "blocks" graph: A → B means A must finish before B can start. */
export function buildDependencyGraph(
  objects: GraphObject[],
): DependencyGraph {
  const graph: DependencyGraph = new Map();
  for (const obj of objects) {
    if (!graph.has(obj.id)) graph.set(obj.id, new Set());
  }
  for (const obj of objects) {
    for (const predId of getPredecessors(obj)) {
      if (!graph.has(predId)) graph.set(predId, new Set());
      graph.get(predId)?.add(obj.id);
    }
  }
  return graph;
}

/** Build the inverse: B → {A} means B is blocked by A. */
export function buildPredecessorGraph(
  objects: GraphObject[],
): DependencyGraph {
  const graph: DependencyGraph = new Map();
  for (const obj of objects) {
    if (!graph.has(obj.id)) graph.set(obj.id, new Set());
    for (const predId of getPredecessors(obj)) {
      graph.get(obj.id)?.add(predId);
    }
  }
  return graph;
}

// ── Topological sort (Kahn's algorithm) ──────────────────────────────────────

/**
 * Return object IDs in topological order (predecessors before successors).
 * Cyclic nodes are appended at the end.
 */
export function topologicalSort(objects: GraphObject[]): string[] {
  const graph = buildDependencyGraph(objects);
  const inDegree = new Map<string, number>();

  for (const obj of objects) {
    inDegree.set(obj.id, getPredecessors(obj).length);
  }

  const queue = [...inDegree.entries()]
    .filter(([, deg]) => deg === 0)
    .map(([id]) => id);
  const result: string[] = [];

  while (queue.length > 0) {
    const id = queue.shift() as string;
    result.push(id);
    for (const successor of graph.get(id) ?? []) {
      const newDeg = (inDegree.get(successor) ?? 0) - 1;
      inDegree.set(successor, newDeg);
      if (newDeg === 0) queue.push(successor);
    }
  }

  const resultSet = new Set(result);
  for (const obj of objects) {
    if (!resultSet.has(obj.id)) result.push(obj.id);
  }

  return result;
}

// ── Cycle detection ──────────────────────────────────────────────────────────

/** Return dependency cycles as arrays of IDs (empty = no cycles). */
export function detectCycles(objects: GraphObject[]): string[][] {
  const graph = buildDependencyGraph(objects);
  const visited = new Set<string>();
  const inStack = new Set<string>();
  const cycles: string[][] = [];

  function dfs(id: string, path: string[]) {
    if (inStack.has(id)) {
      const start = path.indexOf(id);
      if (start !== -1) cycles.push([...path.slice(start), id]);
      return;
    }
    if (visited.has(id)) return;

    visited.add(id);
    inStack.add(id);
    path.push(id);

    for (const succ of graph.get(id) ?? []) dfs(succ, path);

    path.pop();
    inStack.delete(id);
  }

  for (const obj of objects) {
    if (!visited.has(obj.id)) dfs(obj.id, []);
  }

  return cycles;
}

// ── Blocking chain ───────────────────────────────────────────────────────────

/**
 * Return all object IDs that are *transitively blocking* `objectId` (upstream).
 * Result is in BFS order (closest blockers first).
 */
export function findBlockingChain(
  objectId: string,
  objects: GraphObject[],
): string[] {
  const pred = buildPredecessorGraph(objects);
  const visited = new Set<string>();
  const queue = [objectId];
  const result: string[] = [];

  while (queue.length > 0) {
    const current = queue.shift() as string;
    for (const predecessor of pred.get(current) ?? []) {
      if (!visited.has(predecessor) && predecessor !== objectId) {
        visited.add(predecessor);
        result.push(predecessor);
        queue.push(predecessor);
      }
    }
  }

  return result;
}

// ── Impact analysis ──────────────────────────────────────────────────────────

/** Return all object IDs downstream of `objectId` (BFS order). */
export function findImpactedObjects(
  objectId: string,
  objects: GraphObject[],
): string[] {
  const graph = buildDependencyGraph(objects);
  const visited = new Set<string>();
  const queue = [objectId];
  const result: string[] = [];

  while (queue.length > 0) {
    const current = queue.shift() as string;
    for (const succ of graph.get(current) ?? []) {
      if (!visited.has(succ)) {
        visited.add(succ);
        result.push(succ);
        queue.push(succ);
      }
    }
  }

  return result;
}

/**
 * Compute how many days each downstream object would slip if `objectId`
 * slips by `slipDays`. BFS wave propagation, conservative (no float absorption).
 */
export function computeSlipImpact(
  objectId: string,
  slipDays: number,
  objects: GraphObject[],
): SlipImpact[] {
  const graph = buildDependencyGraph(objects);
  const objMap = new Map(objects.map((o) => [o.id as string, o]));

  const slipMap = new Map<string, number>([[objectId, slipDays]]);
  const depthMap = new Map<string, number>([[objectId, 0]]);
  const visited = new Set<string>([objectId]);
  const queue = [objectId];

  while (queue.length > 0) {
    const current = queue.shift() as string;
    const currentSlip = slipMap.get(current) ?? 0;
    const currentDepth = depthMap.get(current) ?? 0;

    for (const succ of graph.get(current) ?? []) {
      slipMap.set(succ, Math.max(slipMap.get(succ) ?? 0, currentSlip));
      depthMap.set(
        succ,
        Math.min(depthMap.get(succ) ?? Infinity, currentDepth + 1),
      );

      if (!visited.has(succ)) {
        visited.add(succ);
        queue.push(succ);
      }
    }
  }

  return [...slipMap.entries()]
    .filter(([id]) => id !== objectId)
    .map(([id, slip]) => ({
      objectId: id,
      objectName: objMap.get(id)?.name ?? id,
      slipDays: slip,
      depth: depthMap.get(id) ?? 0,
    }))
    .sort(
      (a, b) =>
        a.depth - b.depth || a.objectName.localeCompare(b.objectName),
    );
}
