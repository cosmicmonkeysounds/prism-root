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
| **Total** | **58** | **All Pass** |

## Next: Phase 3 — The Graph

Builder 2 -- spatial node graph over Loro state.

- @xyflow/react with custom CodeMirror/Markdown nodes
- Wire types: Hard Ref (elkjs) + Weak Ref (dashed Bezier)
- XState tool machine: Hand / Select / Edit
- PixiJS bird's-eye for massive graphs
