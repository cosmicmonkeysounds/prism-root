# Current Plan

## Phase 1: The Heartbeat (Complete)

Loro CRDT round-trips between browser and Rust daemon via Tauri IPC. All tests passing.

## Phase 2: The Eyes (Complete)

Visual editing of CRDT state.

### Completed

- [x] CodeMirror 6 editing `LoroText` with real-time bidirectional sync
  - `loroSync()` extension: CM edits -> Loro, Loro changes -> CM update
  - `createLoroTextDoc()` helper for creating editor documents
  - `useCodemirror()` React hook with lifecycle management
  - `prismEditorSetup()` shared base configuration
- [x] Puck: drag -> Loro, Loro -> Puck `data` prop
  - `createPuckLoroBridge()` stores layout data in Loro root map
  - `usePuckLoro()` React hook for reactive Puck data
  - CRDT merge support (peer sync through Loro export/import)
- [x] KBar command palette with focus-depth routing
  - `createActionRegistry()` with Global -> App -> Plugin -> Cursor depths
  - `PrismKBarProvider` wrapping kbar with depth-aware filtering
  - `usePrismKBar()` hook for action registration
- [x] `prism-studio`: multi-panel IDE layout
  - Editor tab: CodeMirror + CRDT Inspector (resizable panels)
  - Layout tab: Puck visual builder with Heading/Text/Card components
  - CRDT tab: full-width state inspector
  - KBar palette (CMD+K) with navigation actions
- [x] `notify` file watcher in prism-daemon
  - `watch_directory()` with create/modify/remove event detection
  - Non-blocking `poll_events()` and blocking `wait_event()`
  - Rust tests: file create, modify, remove detection

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (loro-bridge) | 9 | Pass |
| Vitest (use-crdt-store) | 7 | Pass |
| Vitest (lua-runtime) | 8 | Pass |
| Vitest (loro-sync CM) | 6 | Pass |
| Vitest (puck-loro-bridge) | 7 | Pass |
| Vitest (focus-depth) | 9 | Pass |
| Rust (crdt commands) | 3 | Pass |
| Rust (lua commands) | 6 | Pass |
| Rust (file watcher) | 3 | Pass |
| **Phase 1+2 Total** | **58** | **All Pass** |

## Phase 0: The Object Model (Complete)

Ported from legacy Helm codebase with Helm→Prism rename. Foundation for the Object-Graph.

### Completed

- [x] `GraphObject` — universal graph node with shell + payload pattern
- [x] `ObjectEdge` — typed edges between objects with behavior semantics
- [x] `EntityDef` — schema-typed entity blueprints with category, tabs, fields
- [x] `ObjectRegistry` — runtime registry of entity/edge types, category rules, slot system
- [x] `TreeModel` — stateful in-memory tree with add/remove/move/reorder/duplicate/update
- [x] `EdgeModel` — stateful in-memory edge store with hooks and events
- [x] `WeakRefEngine` — automatic content-derived cross-object edges via providers
- [x] `NSID` — namespaced identifiers for cross-Node type interoperability
- [x] `PrismAddress` — `prism://did:web:node/objects/id` addressing scheme
- [x] `NSIDRegistry` — NSID↔local type bidirectional mapping
- [x] `ObjectQuery` — typed query descriptor with filtering, sorting, serialization
- [x] Branded ID types (`ObjectId`, `EdgeId`) with zero-cost type safety
- [x] Slot system for Lens extensions (tabs + fields contributed without modifying base EntityDef)

### Axed from Legacy

- `interfaces.ts` — premature abstraction; concrete classes serve as the interface
- `api-config.ts` — Prism uses Tauri IPC, not REST route generation
- `command-palette.ts` — KBar already handles this
- `tree-clipboard.ts` + `cascade.ts` — premature; will add when needed
- `context-engine.ts` — deferred to Phase 3
- `lua-bridge.ts` — Prism has its own Lua integration
- `presets/` — domain-specific; Lenses define their own

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (registry) | 19 | Pass |
| Vitest (tree-model) | 22 | Pass |
| Vitest (edge-model) | 13 | Pass |
| Vitest (nsid) | 15 | Pass |
| Vitest (query) | 15 | Pass |
| **Phase 0 Total** | **84** | **All Pass** |

## Next: Phase 3 — The Graph

Builder 2 -- spatial node graph over Loro state.

- @xyflow/react with custom CodeMirror/Markdown nodes
- Wire types: Hard Ref (elkjs) + Weak Ref (dashed Bezier)
- XState tool machine: Hand / Select / Edit
- PixiJS bird's-eye for massive graphs
