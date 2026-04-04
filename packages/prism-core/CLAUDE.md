# @prism/core

Client-side execution environment — Layer 1 (agnostic TS) + Layer 2 (React renderers).

## Build
- `pnpm typecheck` / `pnpm test` / `pnpm test:watch`

## Architecture
- **Layer 1**: Zustand stores, Loro bridge, Lua runtime (wasmoon), XState machines
- **Layer 2**: CodeMirror 6 (LoroText sync), Puck (Loro layout), KBar (focus-depth), Spatial Graph (@xyflow/react + elkjs)
- Loro CRDT is the hidden buffer. Editors project Loro state.
- Zustand stores subscribe to specific Loro node IDs (atomic)

## Exports
- `@prism/core/layer1` — all Layer 1 primitives
- `@prism/core/layer2` — all Layer 2 renderers
- `@prism/core/stores` — Zustand store factories
- `@prism/core/lua` — browser Lua runtime (wasmoon)
- `@prism/core/codemirror` — CodeMirror + LoroText sync
- `@prism/core/puck` — Puck + Loro layout bridge
- `@prism/core/kbar` — KBar with focus-depth routing
- `@prism/core/graph` — Spatial node graph (PrismGraph, custom nodes/edges, elkjs layout)
- `@prism/core/machines` — XState tool machine (hand/select/edit FSM)
