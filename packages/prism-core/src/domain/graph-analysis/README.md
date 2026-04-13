# graph-analysis/

Dependency-graph analysis and Critical Path Method (CPM) planning over any `GraphObject[]` that declares predecessors via `data.dependsOn` or `data.blockedBy`. All functions are pure.

```ts
import { buildDependencyGraph, computePlan } from "@prism/core/graph-analysis";
```

## Key exports

- `buildDependencyGraph(objects)` — adjacency list where `A → B` means "A unblocks B".
- `buildPredecessorGraph(objects)` — inverse: `B → {A}` means "B is blocked by A".
- `topologicalSort(objects)` — Kahn's algorithm; cyclic nodes appended at the end.
- `detectCycles(objects)` — returns cycles found in the dependency graph.
- `findBlockingChain(objects, id)` — walk the predecessor chain blocking a given object.
- `findImpactedObjects(objects, id)` — downstream objects affected by a change.
- `computeSlipImpact(objects, id, slipDays)` — cascading slip analysis returning `SlipImpact[]`.
- `computePlan(objects)` — CPM plan with early/late start/finish, total float, and critical path. Duration priority: `data.durationDays` → `data.estimateMs` → `date`/`endDate` span → 1 day default.
- `DependencyGraph`, `SlipImpact`, `PlanNode`, `PlanResult` — supporting types.

## Usage

```ts
import { computePlan } from "@prism/core/graph-analysis";

const plan = computePlan(tasks);

console.log(`Project spans ${plan.totalDurationDays} days`);
console.log(`Critical path: ${plan.criticalPath.join(" -> ")}`);

for (const node of plan.nodes.values()) {
  if (node.isCritical) console.log(node.name, "float=0");
}
```
