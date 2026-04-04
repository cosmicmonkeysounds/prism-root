# Phase 1 Checklist: The Heartbeat

Loro CRDT round-trips between browser and Rust daemon via Tauri IPC.

## Monorepo Scaffold

- [x] Root `package.json` with Turborepo + pnpm scripts
- [x] `pnpm-workspace.yaml` defining package locations
- [x] `turbo.json` with build/dev/typecheck/lint/test tasks
- [x] `tsconfig.base.json` with strict mode, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`
- [x] `tsconfig.json` root with project references
- [x] ESLint 9 flat config with typescript-eslint (no `any` rule)
- [x] Prettier config
- [x] Vitest config with `@prism/*` aliases
- [x] Husky pre-commit hook (typecheck + lint + test)
- [x] `.gitignore` updated for Rust targets

## packages/shared

- [x] Core types: `NodeId`, `VaultId`, `CrdtSnapshot`, `CrdtUpdate`, `LuaResult`, `Manifest`
- [x] IPC types: `CrdtWriteRequest`, `CrdtReadRequest`, `LuaExecRequest`, `IpcCommands`
- [x] `package.json` with `exports` field
- [x] `CLAUDE.md`

## packages/prism-core (Layer 1)

- [x] **Loro Bridge** (`loro-bridge.ts`): `createLoroBridge()` wrapping `LoroDoc`
  - [x] `set()` / `get()` / `delete()` on root map
  - [x] `exportSnapshot()` / `exportUpdate()` / `import()`
  - [x] `onChange()` subscription with cleanup
  - [x] `toJSON()` serialization
- [x] **Zustand Store** (`use-crdt-store.ts`): `createCrdtStore()`
  - [x] `connect(bridge)` hydrates from Loro and subscribes
  - [x] `set()` writes through to Loro (optimistic update)
  - [x] `get()` reads from local state
  - [x] Disconnect cleanup
- [x] **Lua Runtime** (`lua-runtime.ts`): wasmoon browser Lua 5.4
  - [x] `executeLua(script, args)` with global injection
  - [x] `createLuaEngine()` for persistent contexts
  - [x] Error handling with `LuaResult` type
- [x] Package exports: `@prism/core/layer1`, `@prism/core/stores`, `@prism/core/lua`
- [x] `CLAUDE.md`

## packages/prism-daemon (Rust)

- [x] `Cargo.toml` with `loro`, `mlua` (lua54 + vendored), `serde`, `thiserror`
- [x] `DocManager` for multi-document CRDT management
- [x] **CRDT commands** (`commands/crdt.rs`)
  - [x] `crdt_write()` / `crdt_read()` / `crdt_export()` / `crdt_import()`
  - [x] Rust tests: write/read, export/import, merge two peers
- [x] **Lua commands** (`commands/lua.rs`)
  - [x] `lua_exec()` with JSON args injection
  - [x] JSON <-> Lua value conversion
  - [x] Rust tests: arithmetic, args, strings, tables, errors, factorial round-trip
- [x] `DaemonError` enum with thiserror
- [x] `CLAUDE.md`

## packages/prism-studio (Vite + Tauri)

- [x] Vite SPA config with React plugin and `@prism/*` aliases
- [x] `index.html` + `main.tsx` + `App.tsx` (Phase 1 demo UI)
- [x] **IPC Bridge** (`ipc-bridge.ts`): typed wrappers around Tauri `invoke()`
  - [x] `ipcCrdtWrite()` / `ipcCrdtRead()` / `ipcCrdtExport()`
  - [x] `ipcLuaExec()`
- [x] **Tauri shell** (`src-tauri/`)
  - [x] `Cargo.toml` depending on `prism-daemon`
  - [x] `tauri.conf.json` (Tauri 2.0)
  - [x] `main.rs` wiring `DocManager` as Tauri managed state
  - [x] `commands.rs` delegating to `prism-daemon`
- [x] `CLAUDE.md`

## Tests

- [x] **Loro Bridge tests** (`loro-bridge.test.ts`)
  - [x] Set/get/delete operations
  - [x] Snapshot export/import
  - [x] Two-peer merge convergence
  - [x] Concurrent edit resolution (CRDT determinism)
  - [x] onChange subscription and cleanup
- [x] **Zustand Store tests** (`use-crdt-store.test.ts`)
  - [x] Connect/disconnect lifecycle
  - [x] Write-through to Loro
  - [x] React to external bridge changes
  - [x] Zustand subscribe for reactive updates
  - [x] Error on write without connection
- [x] **Lua round-trip tests** (`lua-runtime.test.ts`)
  - [x] Simple arithmetic
  - [x] Argument injection
  - [x] String operations
  - [x] Table returns
  - [x] Error handling
  - [x] Factorial parity test (same script as Rust mlua)
- [x] **Rust CRDT tests** (in `commands/crdt.rs`)
- [x] **Rust Lua tests** (in `commands/lua.rs`)

## Infrastructure

- [x] Claude Code hooks (`scripts/hooks/`)
  - [x] `post-edit-check.sh` — reminds about test files
  - [x] `pre-stop-check.sh` — runs typecheck + lint before task completion
- [x] `.claude/settings.json` with hook configuration
- [x] Root `CLAUDE.md` with project overview and workflow rules
- [x] `docs/dev/current-plan.md` living scratchpad
- [x] `e2e/` Playwright package placeholder

## Verification (Post-Scaffold)

- [x] Run `pnpm install` and verify all dependencies resolve
- [x] Run `pnpm typecheck` and fix any type errors
- [x] Run `pnpm test` and verify all Vitest tests pass (24/24)
- [x] Run `cargo test` in prism-daemon and verify Rust tests pass (9/9)
- [x] Run `cargo clippy` with zero warnings
- [x] Write ADR-001: "Why Loro over Yjs" (`docs/adr/001-loro-over-yjs.md`)
- [x] Playwright E2E test scaffolded (`e2e/tests/heartbeat.spec.ts`)

---

# Phase 2 Checklist: The Eyes

Visual editing of CRDT state.

## CodeMirror 6 + LoroText

- [x] `loroSync()` CM extension — bidirectional sync with LoroText
  - [x] CM edits → LoroText (insert, delete, replace)
  - [x] External Loro changes → CM state update
  - [x] Feedback loop prevention (remote change flag)
- [x] `createLoroTextDoc()` helper for document creation
- [x] `prismEditorSetup()` shared base configuration
- [x] `prismJSLang()` / `prismJSONLang()` language extensions
- [x] `useCodemirror()` React hook with lifecycle management
- [x] Tests: CM→Loro insert/delete/replace, createLoroTextDoc (6 tests)

## Puck + Loro

- [x] `createPuckLoroBridge()` — Puck data ↔ Loro root map
  - [x] `getData()` / `setData()` with JSON serialization
  - [x] `subscribe()` for reactive updates
  - [x] CRDT merge support (peer sync)
- [x] `usePuckLoro()` React hook for reactive Puck data
- [x] Tests: empty state, store/retrieve, overwrite, subscribe, unsubscribe, merge (7 tests)

## KBar Command Palette

- [x] `createActionRegistry()` with focus-depth routing
  - [x] Four depths: Global → App → Plugin → Cursor
  - [x] Depth-scoped action visibility
  - [x] Dynamic register/unregister
  - [x] Subscribe to changes
- [x] `PrismKBarProvider` — React provider with CMD+K palette
- [x] `usePrismKBar()` hook for depth and registry access
- [x] Tests: depth filtering, registration, deduplication, notifications (9 tests)

## File Watcher (prism-daemon)

- [x] `watch_directory()` using `notify` crate
- [x] `FileChangeEvent` with kind + paths
- [x] `poll_events()` non-blocking + `wait_event()` blocking
- [x] Rust tests: file create, modify, remove detection (3 tests)

## Prism Studio UI

- [x] Multi-panel IDE layout with `react-resizable-panels`
- [x] Editor tab: CodeMirror 6 editing LoroText + CRDT Inspector sidebar
- [x] Layout tab: Puck visual builder with Heading/Text/Card components
- [x] CRDT tab: full-width state inspector
- [x] KBar palette (CMD+K) with navigation actions

## Verification

- [x] `pnpm typecheck` — zero errors
- [x] `pnpm test` — 46/46 pass
- [x] `pnpm lint` — clean
- [x] `cargo test` — 12/12 pass
- [x] `cargo clippy` — zero warnings

---

# Phase 3 Checklist: The Graph

Spatial node graph with custom nodes, edges, and auto-layout.

## Layer 1 — Graph Store + Tool Machine

- [x] **Graph Store** (`use-graph-store.ts`): `createGraphStore(doc)`
  - [x] LoroList<LoroMap> backing for nodes and edges
  - [x] `addNode()` / `moveNode()` / `updateNodeData()` / `removeNode()`
  - [x] `addEdge()` / `removeEdge()`
  - [x] Cascade edge deletion on node removal
  - [x] `syncFromLoro()` for external peer updates
  - [x] Auto-subscribe to LoroDoc changes
- [x] **Tool Machine** (`tool.machine.ts`): XState v5 FSM
  - [x] States: hand → select → edit
  - [x] Events: SWITCH_HAND, SWITCH_SELECT, SWITCH_EDIT, DOUBLE_CLICK_NODE, CLICK_CANVAS, PRESS_ESCAPE
  - [x] `createToolActor()` / `getToolMode()` helpers
- [x] Package exports: `@prism/core/machines`, `@prism/core/graph`

## Layer 2 — Graph Renderer

- [x] **Custom Nodes** (`custom-nodes.tsx`)
  - [x] CodeMirrorNode — embedded code display
  - [x] MarkdownNode — react-markdown with remark-gfm
  - [x] DefaultPrismNode — simple labeled node
  - [x] `prismNodeTypes` registry
- [x] **Custom Edges** (`custom-edges.tsx`)
  - [x] HardRefEdge — solid orthogonal (SmoothStep) path
  - [x] WeakRefEdge — dashed Bezier curve
  - [x] `prismEdgeTypes` registry
- [x] **Auto-Layout** (`auto-layout.ts`)
  - [x] `applyElkLayout()` using elkjs layered algorithm
  - [x] Orthogonal edge routing
  - [x] Configurable direction and spacing
- [x] **PrismGraph** (`prism-graph.tsx`)
  - [x] ReactFlow wrapper with custom node/edge types
  - [x] Loro-backed graph store integration
  - [x] Node position changes sync back to Loro
  - [x] Edge removal syncs to Loro

## Prism Studio

- [x] Graph tab with seed demo nodes (CodeMirror, Markdown, Default)
- [x] Hard Ref and Weak Ref demo edges
- [x] KBar navigation action for Graph tab

## Tests

- [x] **Graph Store tests** (`use-graph-store.test.ts`) — 10 tests
  - [x] Empty initial state
  - [x] Add/move/update/remove nodes
  - [x] Add/remove edges with wire types
  - [x] Cascade edge deletion
  - [x] Loro peer sync (export/import)
  - [x] Graceful no-op on missing IDs
- [x] **Tool Machine tests** (`tool.machine.test.ts`) — 11 tests
  - [x] Initial state, all transitions
  - [x] Escape behavior per state
  - [x] Invalid event handling
- [x] **Auto-Layout tests** (`auto-layout.test.ts`) — 5 tests
  - [x] Distinct positions assigned
  - [x] Data preservation
  - [x] Input immutability
  - [x] Empty input handling
  - [x] Direction option respected
- [x] **Playwright E2E** (`e2e/tests/graph.spec.ts`) — 5 tests
  - [x] ReactFlow canvas renders
  - [x] Seed nodes visible
  - [x] Seed edges visible
  - [x] Custom node content renders
  - [x] KBar navigation to Graph tab

## Verification

- [x] `pnpm typecheck` — zero errors
- [x] `pnpm test` — 72/72 pass (Vitest)
- [ ] `npx playwright test` — E2E (requires dev server)
