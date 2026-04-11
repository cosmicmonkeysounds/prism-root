/**
 * Planning Engine — Generic Critical Path Method (CPM).
 *
 * Works on any GraphObject that declares dependencies via
 * `data.dependsOn` and/or `data.blockedBy`. Useful for tasks, goals,
 * project phases, learning paths — any domain with ordering constraints.
 *
 * Duration priority:
 *   1. data.durationDays  — explicit integer days
 *   2. data.estimateMs    — millisecond estimate → days
 *   3. date + endDate     — span from scheduled dates
 *   4. default 1 day
 */

import type { GraphObject } from "@prism/core/object-model";
import { topologicalSort } from "./dependency-graph.js";

// ── Types ────────────────────────────────────────────────────────────────────

export interface PlanNode {
  id: string;
  name: string;
  type: string;
  durationDays: number;
  earlyStart: number;
  earlyFinish: number;
  lateStart: number;
  lateFinish: number;
  totalFloat: number;
  isCritical: boolean;
  predecessors: string[];
}

export interface PlanResult {
  totalDurationDays: number;
  criticalPath: string[];
  nodes: Map<string, PlanNode>;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

const MS_PER_DAY = 1_000 * 60 * 60 * 24;

function getDuration(obj: GraphObject): number {
  const d = obj.data as Record<string, unknown> | null;
  if (!d) return 1;
  if (typeof d.durationDays === "number" && d.durationDays > 0)
    return d.durationDays;
  if (typeof d.estimateMs === "number" && d.estimateMs > 0) {
    return Math.max(Math.ceil(d.estimateMs / MS_PER_DAY), 1);
  }
  if (typeof obj.date === "string" && typeof obj.endDate === "string") {
    const span =
      new Date(obj.endDate).getTime() - new Date(obj.date).getTime();
    if (span > 0) return Math.max(Math.ceil(span / MS_PER_DAY), 1);
  }
  return 1;
}

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

// ── CPM ──────────────────────────────────────────────────────────────────────

export function computePlan(objects: GraphObject[]): PlanResult {
  if (objects.length === 0) {
    return { totalDurationDays: 0, criticalPath: [], nodes: new Map() };
  }

  const order = topologicalSort(objects);
  const nodes = new Map<string, PlanNode>();

  for (const obj of objects) {
    nodes.set(obj.id, {
      id: obj.id,
      name: obj.name,
      type: obj.type,
      durationDays: getDuration(obj),
      earlyStart: 0,
      earlyFinish: 0,
      lateStart: 0,
      lateFinish: 0,
      totalFloat: 0,
      isCritical: false,
      predecessors: getPredecessors(obj),
    });
  }

  // Forward pass
  for (const id of order) {
    const node = nodes.get(id);
    if (!node) continue;
    const maxPredEF = node.predecessors.reduce(
      (max, predId) => Math.max(max, nodes.get(predId)?.earlyFinish ?? 0),
      0,
    );
    node.earlyStart = maxPredEF;
    node.earlyFinish = maxPredEF + node.durationDays;
  }

  const totalDuration = Math.max(
    ...[...nodes.values()].map((n) => n.earlyFinish),
    0,
  );

  // Successor index
  const succs = new Map<string, string[]>();
  for (const obj of objects) succs.set(obj.id, []);
  for (const node of nodes.values()) {
    for (const predId of node.predecessors) {
      if (!succs.has(predId)) succs.set(predId, []);
      succs.get(predId)?.push(node.id);
    }
  }

  // Backward pass
  for (const node of nodes.values()) {
    node.lateFinish = totalDuration;
    node.lateStart = totalDuration - node.durationDays;
  }

  for (const id of [...order].reverse()) {
    const node = nodes.get(id);
    if (!node) continue;
    const nodeSuccs = succs.get(id) ?? [];
    if (nodeSuccs.length > 0) {
      const minLS = nodeSuccs.reduce(
        (min, sid) => Math.min(min, nodes.get(sid)?.lateStart ?? Infinity),
        Infinity,
      );
      node.lateFinish = minLS;
      node.lateStart = minLS - node.durationDays;
    }
    node.totalFloat = node.lateStart - node.earlyStart;
    node.isCritical = node.totalFloat <= 0;
  }

  // Extract critical path
  const critSet = new Set(
    [...nodes.values()].filter((n) => n.isCritical).map((n) => n.id),
  );
  const starts = [...nodes.values()]
    .filter(
      (n) =>
        n.isCritical && n.predecessors.every((p) => !critSet.has(p)),
    )
    .sort((a, b) => a.earlyStart - b.earlyStart);

  const criticalPath: string[] = [];
  const visited = new Set<string>();

  function walk(id: string) {
    if (visited.has(id)) return;
    visited.add(id);
    criticalPath.push(id);
    const critSuccs = (succs.get(id) ?? [])
      .filter((s) => critSet.has(s))
      .sort(
        (a, b) =>
          (nodes.get(b)?.earlyFinish ?? 0) -
          (nodes.get(a)?.earlyFinish ?? 0),
      );
    if (critSuccs.length > 0) walk(critSuccs[0] as string);
  }

  for (const start of starts) walk(start.id);

  return { totalDurationDays: totalDuration, criticalPath, nodes };
}
