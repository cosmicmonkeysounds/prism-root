# xyflow/

Spatial node graph built on `@xyflow/react`, with Prism custom node types (CodeMirror, Markdown, Default), custom edge types (Hard Ref, Weak Ref, Stream), and elkjs-powered auto-layout. Backed by a Zustand `GraphStore` that projects from Loro.

Exported under TWO subpath aliases: `@prism/core/graph` (canonical) and `@prism/core/xyflow`.

```ts
import { PrismGraph, applyElkLayout } from "@prism/core/graph";
```

## Key exports

- `PrismGraph` — React Flow component bound to a Loro-backed `GraphStore`. Props: `{ store, onNodeDoubleClick?, onCanvasClick?, minimap?, className? }`.
- `prismNodeTypes` — registry of Prism custom node React components keyed for `<ReactFlow nodeTypes>`.
- `CodeMirrorNodeMemo`, `MarkdownNodeMemo`, `DefaultNodeMemo` — memoized custom node components.
- `prismEdgeTypes` — registry of custom edge components.
- `HardRefEdgeComponent`, `WeakRefEdgeComponent`, `StreamEdgeComponent` — edge renderers.
- `applyElkLayout(nodes, edges, options?)` — async auto-layout via elkjs. Orthogonal routing, configurable direction (`DOWN`/`RIGHT`/`UP`/`LEFT`), spacing, node dimensions. Returns new nodes (pure).
- Types: `PrismGraphProps`, `CodeMirrorNode`, `MarkdownNode`, `DefaultPrismNode`, `HardRefEdge`, `WeakRefEdge`, `StreamEdge`, `LayoutOptions`.

## Usage

```tsx
import { PrismGraph, applyElkLayout } from "@prism/core/graph";
import { createGraphStore } from "@prism/core/stores";

const store = createGraphStore(loroDoc);

function Canvas() {
  return (
    <PrismGraph
      store={store}
      minimap
      onNodeDoubleClick={(_, node) => openEditor(node.id)}
    />
  );
}

// Auto-layout a snapshot
const laidOut = await applyElkLayout(nodes, edges, { direction: "RIGHT" });
```
