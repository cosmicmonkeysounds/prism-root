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
- [x] `npx playwright test` — 12/12 E2E pass (7 heartbeat + 5 graph)

---

# Phase 0 Checklist: The Object Model

Foundation data model ported from legacy Helm codebase with Helm→Prism rename.
Pure Layer 1 TypeScript — no UI surface, no Playwright E2E needed.

## Types & Branded IDs

- [x] `ObjectId` / `EdgeId` branded string types with zero-cost type safety
- [x] `objectId()` / `edgeId()` cast helpers for trust boundaries
- [x] `EntityFieldType` — 11 value types (bool, int, float, string, text, color, enum, object_ref, date, datetime, url)
- [x] `EntityFieldDef` — field schema with validation, defaults, computed expressions, enum options, ref types, UI hints
- [x] `GraphObject` — universal graph node (shell + payload pattern)
- [x] `ObjectEdge` — typed edge with relation, position, data payload
- [x] `ResolvedEdge` — edge with target object resolved inline
- [x] `EdgeBehavior` — weak, strong, dependency, membership, assignment
- [x] `EdgeTypeDef` — edge type definition with constraints, cascade, federation scope
- [x] `EntityDef` — entity blueprint (category, tabs, fields, containment overrides)
- [x] `CategoryRule` — category parenting rules
- [x] `TabDefinition` — detail view tabs

## ObjectRegistry

- [x] Entity type registration: `register()`, `registerAll()`, `addCategoryRule()`
- [x] Lookup: `get()`, `has()`, `allTypes()`, `allDefs()`, `getLabel()`, `getPluralLabel()`, `getColor()`, `getIcon()`, `getCategory()`
- [x] Tabs: `getTabs()`, `getEffectiveTabs()` (base + slot merge, base wins collision)
- [x] Fields: `getEntityFields()` (base + slot merge)
- [x] Containment: `canBeChildOf()`, `canBeRoot()`, `canHaveChildren()`, `getAllowedChildTypes()`
- [x] Edge types: `registerEdge()`, `registerEdges()`, `getEdgeType()`, `canConnect()`, `getEdgesFrom()`, `getEdgesTo()`, `getEdgesBetween()`
- [x] Slots: `registerSlot()`, `getSlots()` (forTypes + forCategories OR logic)
- [x] Tree utilities: `buildTree()`, `getAncestors()`

## TreeModel

- [x] `add()` with containment validation and position management
- [x] `remove()` with cascade descendant deletion and position compaction
- [x] `move()` with circular reference detection and containment validation
- [x] `reparent()` convenience wrapper
- [x] `reorder()` within same parent
- [x] `duplicate()` with deep clone option
- [x] `update()` with immutable field protection (id, type, createdAt)
- [x] Query: `get()`, `has()`, `getChildren()`, `getDescendants()`, `getAncestors()`, `buildTree()`, `toArray()`
- [x] Events: add, remove, move, reorder, duplicate, update, change
- [x] Lifecycle hooks: before/after for all mutations
- [x] `TreeModelError` with typed error codes
- [x] JSON serialization round-trip

## EdgeModel

- [x] `add()` with registry validation (allowMultiple, undirected)
- [x] `remove()`, `update()` with immutable field protection
- [x] Query: `get()`, `has()`, `getAll()`, `getFrom()`, `getTo()`, `getBetween()`, `getConnected()`
- [x] Events: add, remove, update, change
- [x] Lifecycle hooks: before/after for all mutations
- [x] JSON serialization round-trip

## WeakRefEngine

- [x] Provider registration: `registerProvider()`, `unregisterProvider()`, `getProvider()`, `allProviders()`
- [x] Lifecycle: `start()`, `stop()`, `dispose()`
- [x] `recompute()` — diff-based edge materialization per object
- [x] `rebuildAll()` — full sync for project open / bulk import
- [x] `removeFor()` — clean up edges for deleted objects
- [x] Query: `getWeakRefChildren()`, `getWeakRefParents()`
- [x] `augmentTree()` — populate weakRefChildren on TreeNode[]
- [x] `isWeakRefEdge()` — identify engine-managed edges
- [x] Federation support: `scope: 'federated'` refs materialized even without local target
- [x] Events: recomputed, rebuilt, change

## NSID & Addressing

- [x] `NSID` branded type with validation regex (3+ reverse-DNS segments)
- [x] `isValidNSID()`, `parseNSID()`, `nsid()` constructor
- [x] `nsidAuthority()`, `nsidName()` extractors
- [x] `PrismAddress` branded type (`prism://did:web:node/objects/id`)
- [x] `isValidPrismAddress()`, `prismAddress()`, `parsePrismAddress()`
- [x] `NSIDRegistry` — bidirectional NSID↔local type mapping

## ObjectQuery

- [x] `ObjectQuery` — typed query descriptor (type, parentId, status, tags, date range, search, pinned, pagination, sorting, soft-delete)
- [x] `queryToParams()` / `paramsToQuery()` — URL serialization round-trip
- [x] `matchesQuery()` — in-memory filtering
- [x] `sortObjects()` — in-memory sorting

## Axed from Legacy

- [x] `interfaces.ts` — premature abstraction; concrete classes are the interface
- [x] `api-config.ts` — Prism uses Tauri IPC, not REST route generation
- [x] `command-palette.ts` — KBar already exists in Prism
- [x] `tree-clipboard.ts` + `cascade.ts` — premature; add when needed
- [x] `context-engine.ts` — deferred to later phase
- [x] `lua-bridge.ts` — Prism has its own Lua integration
- [x] `presets/` — domain-specific; Lenses define their own

## Tests

- [x] **Registry tests** (`registry.test.ts`) — 19 tests
  - [x] Entity registration and lookup
  - [x] Containment: category rules, extraChildTypes, extraParentTypes, childOnly, canBeRoot
  - [x] Edge type registration, canConnect, filtering
  - [x] Slot registration by type and category
  - [x] Effective tabs/fields merge with collision handling
  - [x] Tree building from flat objects
- [x] **Tree Model tests** (`tree-model.test.ts`) — 22 tests
  - [x] Add: root, child, containment validation, missing parent
  - [x] Remove: cascade descendants, position compaction
  - [x] Move: reparent, circular ref detection, containment validation
  - [x] Reorder: within parent
  - [x] Duplicate: single and deep
  - [x] Update: shell fields, immutable protection
  - [x] Query: getChildren sorted, getDescendants depth-first, getAncestors
  - [x] Events and unsubscribe
  - [x] JSON round-trip
- [x] **Edge Model tests** (`edge-model.test.ts`) — 13 tests
  - [x] Add/remove/update
  - [x] Query by source, target, between, connected
  - [x] Registry validation: allowMultiple, undirected
  - [x] Events on mutations
  - [x] JSON round-trip
- [x] **NSID tests** (`nsid.test.ts`) — 15 tests
  - [x] NSID validation, parsing, construction
  - [x] Authority/name extraction
  - [x] PrismAddress validation, construction, parsing
  - [x] NSIDRegistry register/resolve/clear
- [x] **Query tests** (`query.test.ts`) — 15 tests
  - [x] URL param round-trip (simple, arrays, null parentId)
  - [x] matchesQuery: type, status, tags, search, parentId, deleted, date range, pinned
  - [x] sortObjects: ascending, descending, default

## Verification

- [x] `pnpm typecheck` — zero errors
- [x] `pnpm test` — 156/156 pass (84 object-model + 72 existing)
- [x] `pnpm lint` — clean
- [x] Package export: `@prism/core/object-model`
- [x] Layer 1 barrel export updated
- [x] `CLAUDE.md` updated (prism-core)
- [x] `docs/dev/current-plan.md` updated
- [x] No Playwright E2E needed — pure Layer 1 logic with no UI surface

---

# Phase 4 Checklist: The Shell

Registry-driven workspace shell replacing hardcoded Studio UI.

## Layer 1 — Workspace Primitives

- [x] `LensId` / `TabId` branded types with `lensId()` / `tabId()` helpers
- [x] `LensManifest` — typed lens definition (id, name, icon, category, contributes)
- [x] `LensCategory` — editor, visual, data, debug, custom
- [x] `LensCommand` / `LensKeybinding` / `LensView` contribution types
- [x] `LensRegistry` — `createLensRegistry()` factory
  - [x] `register()` / `unregister()` with event emission
  - [x] `get()` / `has()` / `allLenses()` / `getByCategory()`
  - [x] `subscribe()` with `registered` / `unregistered` / `change` events
- [x] `WorkspaceStore` — `createWorkspaceStore()` Zustand vanilla
  - [x] `openTab()` with singleton dedup (focus existing unpinned tab)
  - [x] `closeTab()` with adjacent tab activation
  - [x] `pinTab()` / `unpinTab()` toggle
  - [x] `reorderTab()` / `setActiveTab()`
  - [x] `toggleSidebar()` / `toggleInspector()` panel layout
  - [x] `setSidebarWidth()` / `setInspectorWidth()` with clamping

## Layer 2 — Shell Components

- [x] `LensProvider` + `useLensContext()` + `useWorkspaceStore()` React context
- [x] `ActivityBar` — vertical icon bar from LensRegistry with active indicator
- [x] `TabBar` — horizontal tab bar with close/pin/middle-click-close
- [x] `WorkspaceShell` — top-level layout (ActivityBar | Sidebar? | TabBar + Content | Inspector?)
- [x] `data-testid` attributes on all interactive elements

## Prism Studio

- [x] 4 built-in lens manifests: Editor, Graph, Layout, CRDT
- [x] `createLensComponentMap()` — maps LensId to React components via closures
- [x] `registerBuiltinLenses()` — registers all 4 manifests into LensRegistry
- [x] `App.tsx` rewritten: LensProvider + WorkspaceShell replaces hardcoded tabs
- [x] KBar actions derived from LensRegistry (not hardcoded)

## Tests

- [x] **Lens Registry tests** (`lens-registry.test.ts`) — 12 tests
  - [x] Register/retrieve, has(), allLenses(), getByCategory()
  - [x] Unregister via method and via returned function
  - [x] Subscribe fires on register/unregister, unsubscribe works
  - [x] Re-register replaces, empty registry, no-op unregister
- [x] **Workspace Store tests** (`workspace-store.test.ts`) — 15 tests
  - [x] Empty initial state, default panel layout
  - [x] openTab creates and activates, singleton dedup, pinned bypass
  - [x] closeTab removes and activates adjacent, last tab → null
  - [x] pinTab/unpinTab, reorderTab, setActiveTab
  - [x] toggleSidebar, toggleInspector, width clamping
- [x] **Playwright E2E** (`e2e/tests/shell.spec.ts`) — 8 tests
  - [x] Activity bar with all lens icons
  - [x] Clicking activity bar opens tab
  - [x] Tab bar shows open tabs
  - [x] Close tab removes it
  - [x] Pin tab shows indicator
  - [x] Sidebar toggle works
  - [x] KBar navigation still works
  - [x] Multiple tabs switching
- [x] **Updated Playwright** — heartbeat (6) + graph (5) tests updated for new shell selectors

## Verification

- [x] `pnpm typecheck` — zero errors
- [x] `pnpm test` — 183/183 pass (27 workspace + 156 existing)
- [x] `npx playwright test` — 19/19 pass (8 shell + 6 heartbeat + 5 graph)
- [x] Package exports: `@prism/core/workspace`, `@prism/core/shell`
- [x] Layer 1 + Layer 2 barrel exports updated
- [x] `CLAUDE.md` updated (prism-core)
- [x] `docs/dev/current-plan.md` updated
