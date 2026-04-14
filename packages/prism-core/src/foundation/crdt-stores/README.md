# crdt-stores

Vanilla Zustand store factories that project Loro CRDT state into reactive stores. Stores subscribe to specific Loro containers (atomic) rather than the whole document, so consumers re-render only on relevant changes.

```ts
import { createCrdtStore, createGraphStore } from '@prism/core/stores';
```

## Key exports

- `createCrdtStore()` — builds a vanilla Zustand store backed by a `LoroBridge` key-value map. Exposes `connect(bridge)`, `set(key, value)`, `get(key)`, and `data`/`connected` state.
- `createGraphStore(doc)` — builds a store managing a spatial node graph inside a `LoroDoc`. Provides `addNode`, `moveNode`, `updateNodeData`, `removeNode`, `addEdge`, `removeEdge`, `syncFromLoro`.
- `CrdtStore`, `CrdtStoreState`, `CrdtStoreActions` — types for the key-value store.
- `GraphStore`, `GraphNode`, `GraphEdge`, `WireType` — types for the node-graph store. `WireType` is `'hard' | 'weak' | 'stream'` (stream renders as an animated dashed bezier via `StreamEdgeComponent`).

## Usage

```ts
import { LoroDoc } from 'loro-crdt';
import { createGraphStore } from '@prism/core/stores';

const doc = new LoroDoc();
const store = createGraphStore(doc);

store.getState().addNode({
  id: 'n1',
  type: 'task',
  x: 0, y: 0, width: 120, height: 64,
  data: { label: 'Ship it' },
});

const unsubscribe = store.subscribe((state) => console.log(state.nodes.length));
```
