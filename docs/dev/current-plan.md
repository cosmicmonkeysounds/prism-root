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

## Phase 4: The Shell (Complete)

Registry-driven workspace shell replacing hardcoded Studio UI.

### Completed

- [x] `LensManifest` — typed lens definition (id, name, icon, category, contributes)
- [x] `LensRegistry` — register/unregister/query/subscribe with events
- [x] `WorkspaceStore` — Zustand store for tabs, activeTab, panelLayout
  - [x] Singleton tab behavior (dedup by lensId, pinned tabs opt out)
  - [x] Tab CRUD: openTab, closeTab, pinTab, reorderTab, setActiveTab
  - [x] Panel layout: toggleSidebar, toggleInspector, width management
- [x] `LensProvider` + `useLensContext()` — React context for registries
- [x] `ActivityBar` — vertical icon bar from LensRegistry
- [x] `TabBar` — horizontal tab bar with close/pin controls
- [x] `WorkspaceShell` — top-level layout composing all shell components
- [x] 4 built-in lenses: Editor, Graph, Layout, CRDT (manifests + component map)
- [x] KBar actions derived from LensRegistry (not hardcoded)
- [x] `data-testid` attributes throughout for Playwright

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (lens-registry) | 12 | Pass |
| Vitest (workspace-store) | 15 | Pass |
| Playwright (shell) | 8 | Pass |
| Playwright (heartbeat, updated) | 6 | Pass |
| Playwright (graph, updated) | 5 | Pass |
| **Phase 4 Total** | **27 Vitest + 19 E2E** | **All Pass** |

## Phase 5: Input, Forms, Layout, Expression (Complete)

Four foundational Layer 1 systems ported from legacy Helm. All pure TypeScript, zero React.

### Completed

- [x] **Input System** (`layer1/input/`) — KeyboardModel, InputScope, InputRouter (LIFO scope stack)
  - Shortcut format: `cmd+k`, `cmd+shift+z`, `escape`. `cmd` = Ctrl OR Meta (cross-platform)
  - InputScope: named context with KeyboardModel + action handlers, fluent `.on()`, UndoHook
  - InputRouter: push/pop/replace scopes, handleKeyEvent walks stack top-down, async dispatch
- [x] **Forms & Validation** (`layer1/forms/`) — schema-driven forms + text parsing
  - FieldSchema: 17 field types (text, number, currency, rating, slider, boolean, date, select, etc.)
  - DocumentSchema: fields + sections (TextSection | FieldGroupSection)
  - FormSchema: extends DocumentSchema with validation rules + conditional visibility
  - FormState: immutable pure-function state management (create, set, validate, reset)
  - Wiki-link parser: `parseWikiLinks()`, `extractLinkedIds()`, `renderWikiLinks()`, `detectInlineLink()`
  - Markdown parser: `parseMarkdown()` → BlockToken[], `parseInline()` → InlineToken[]
- [x] **Layout System** (`layer1/layout/`) — multi-pane navigation with history
  - SelectionModel: select/toggle/selectRange/selectAll/clear with events
  - PageModel<TTarget>: viewMode, activeTab, selection, inputScopeId, persist/fromSerialized
  - PageRegistry<TTarget>: maps target.kind → defaults, createPage factory
  - WorkspaceSlot: inline back/forward history (no external NavigationController), LRU page cache
  - WorkspaceManager: multiple slots, active tracking, open/close/focus
- [x] **Expression Engine** (`layer1/expression/`) — formula evaluation
  - Scanner/Tokenizer: operand syntax `[type:id.subfield]`, numbers, strings, booleans, operators
  - Recursive descent parser: standard operator precedence, bare identifiers as field operands
  - Evaluator: arithmetic, comparison, boolean logic (short-circuit), string concat, builtins (abs, ceil, floor, round, sqrt, pow, min, max, clamp)
  - `evaluateExpression()` convenience: auto-wraps bare identifiers as `[field:name]`

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (keyboard-model) | 25 | Pass |
| Vitest (input-router + scope) | 23 | Pass |
| Vitest (form-state) | 20 | Pass |
| Vitest (wiki-link) | 17 | Pass |
| Vitest (markdown) | 27 | Pass |
| Vitest (selection-model) | 12 | Pass |
| Vitest (page-model) | 12 | Pass |
| Vitest (page-registry) | 8 | Pass |
| Vitest (workspace-slot) | 14 | Pass |
| Vitest (workspace-manager) | 14 | Pass |
| Vitest (scanner) | 21 | Pass |
| Vitest (expression) | 68 | Pass |
| **Phase 5 Total** | **261** | **All Pass** |

## Phase 6: Context Engine, Plugin System, Reactive Atoms (Complete)

Three systems that complete the registry-driven architecture. All pure TypeScript, zero React.

### Completed

- [x] **Context Engine** (`layer1/object-model/context-engine.ts`) — context-aware suggestion engine
  - `getEdgeOptions(sourceType, targetType?)` — valid edge types between objects
  - `getChildOptions(parentType)` — object types that can be children
  - `getAutocompleteSuggestions(sourceType)` — inline [[...]] link types + defaultRelation
  - `getContextMenu(objectType, targetType?)` — structured right-click menu (create/connect/object sections)
  - `getInlineLinkTypes(sourceType)` / `getInlineEdgeTypes()` — suggestInline edge types
  - All answers derived from ObjectRegistry — nothing hardcoded
- [x] **Plugin System** (`layer1/plugin/`) — universal extension unit
  - `ContributionRegistry<T>` — generic typed registry (register/unregister/query/byPlugin)
  - `PrismPlugin` — universal plugin interface with `contributes` (views, commands, keybindings, contextMenus, activityBar, settings, toolbar, statusBar, weakRefProviders)
  - `PluginRegistry` — manages plugins, auto-registers contributions into typed registries, events on register/unregister
  - Contribution types: ViewContributionDef, CommandContributionDef, KeybindingContributionDef, ContextMenuContributionDef, ActivityBarContributionDef, SettingsContributionDef, ToolbarContributionDef, StatusBarContributionDef
- [x] **Reactive Atoms** (`layer1/atom/`) — Zustand-based reactive state layer
  - `PrismBus` — lightweight typed event bus (on/once/emit/off, createPrismBus factory)
  - `PrismEvents` — well-known event type constants (objects/edges/navigation/selection/search)
  - `AtomStore` — UI state atoms (selectedId, selectionIds, editingObjectId, activePanel, searchQuery, navigationTarget)
  - `ObjectAtomStore` — in-memory object/edge cache with selectors (selectObject, selectQuery, selectChildren, selectEdgesFrom, selectEdgesTo)
  - `connectBusToAtoms(bus, atomStore)` — wire navigation/selection/search events to UI atoms
  - `connectBusToObjectAtoms(bus, objectStore)` — wire object/edge CRUD events to cache

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (context-engine) | 17 | Pass |
| Vitest (contribution-registry) | 15 | Pass |
| Vitest (plugin-registry) | 17 | Pass |
| Vitest (event-bus) | 13 | Pass |
| Vitest (atoms) | 15 | Pass |
| Vitest (object-atoms) | 19 | Pass |
| Vitest (connect) | 15 | Pass |
| **Phase 6 Total** | **111** | **All Pass** |

## Phase 7: State Machines, Graph Analysis, Planning Engine (Complete)

Three systems for workflow orchestration and dependency analysis. All pure TypeScript, zero React.

### Completed

- [x] **State Machines** (`layer1/automaton/`) — flat FSM ported from legacy `@core/automaton`
  - `Machine<TState, TEvent>` — context-free FSM with guards, actions, lifecycle hooks (onEnter/onExit)
  - `createMachine(def)` factory with `.start()` and `.restore()` (no onEnter on restore)
  - Wildcard `from: '*'` matches any state, array `from` for multi-source transitions
  - Terminal states block all outgoing transitions
  - Observable via `.on()` listener, serializable via `.toJSON()`
- [x] **Dependency Graph** (`layer1/graph-analysis/dependency-graph.ts`) — ported from legacy `@core/tasks`
  - `buildDependencyGraph(objects)` — forward "blocks" graph from `data.dependsOn`/`data.blockedBy`
  - `buildPredecessorGraph(objects)` — inverse "blocked-by" graph
  - `topologicalSort(objects)` — Kahn's algorithm, cyclic nodes appended at end
  - `detectCycles(objects)` — DFS cycle detection, returns cycle paths
  - `findBlockingChain(objectId, objects)` — transitive upstream blockers (BFS)
  - `findImpactedObjects(objectId, objects)` — transitive downstream dependants (BFS)
  - `computeSlipImpact(objectId, slipDays, objects)` — BFS wave propagation of slip
- [x] **Planning Engine** (`layer1/graph-analysis/planning-engine.ts`) — ported from legacy `@core/planning`
  - `computePlan(objects)` — generic CPM on any GraphObject with `data.dependsOn`
  - Forward pass: earlyStart, earlyFinish
  - Backward pass: lateStart, lateFinish, totalFloat
  - Critical path extraction (zero-float nodes)
  - Duration priority: `data.durationDays` > `data.estimateMs` > date span > default 1 day

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (machine) | 19 | Pass |
| Vitest (dependency-graph) | 20 | Pass |
| Vitest (planning-engine) | 10 | Pass |
| **Phase 7 Total** | **49** | **All Pass** |

### E2E Tests (All Phases)

All 19 Playwright tests pass covering Phases 1-4 rendered UI:

| Suite | Tests | Status |
|-------|-------|--------|
| Playwright (heartbeat) | 6 | Pass |
| Playwright (graph) | 5 | Pass |
| Playwright (shell) | 8 | Pass |
| **E2E Total** | **19** | **All Pass** |

Note: Phases 0, 5, 6, 7, 8 are pure Layer 1 TypeScript with no React UI. E2E tests apply to rendered UI features (Phases 1-4). All Layer 1 systems are tested via Vitest unit tests.

## Phase 8: Automation Engine, Prism Manifest (Complete)

Two systems for workflow automation and manifest-driven workspace definition. All pure TypeScript, zero React.

### Terminology (from SPEC.md)

| Term | Definition |
|------|-----------|
| **Vault** | Encrypted local directory — the physical security boundary. Contains Collections and Manifests. |
| **Collection** | Typed CRDT array (e.g. `Contacts`, `Tasks`). Holds the actual data. |
| **Manifest** | JSON file with weak references to Collections. A "workspace" is just a Manifest pointing to data nodes. Multiple manifests can reference the same collection with different filters. |
| **Shell** | The IDE chrome that renders whatever a Manifest references. No fixed layout. |

### Completed

- [x] **Automation Engine** (`layer1/automation/`) — trigger/condition/action rule engine
  - `AutomationTrigger` — ObjectTrigger (created/updated/deleted with type/tag/field filters), CronTrigger, ManualTrigger
  - `AutomationCondition` — FieldCondition (10 comparison operators), TypeCondition, TagCondition, And/Or/Not combinators
  - `AutomationAction` — CreateObject, UpdateObject, DeleteObject, Notification, Delay, RunAutomation
  - `evaluateCondition()` — recursive condition tree evaluator with dot-path field access
  - `interpolate()` — `{{path}}` template replacement from AutomationContext
  - `matchesObjectTrigger()` — object event filtering by type/tag/field match
  - `AutomationEngine` — orchestrator with start/stop lifecycle, cron scheduling, handleObjectEvent(), run()
  - `AutomationStore` interface — synchronous list/get/save/saveRun for pluggable persistence
  - Action dispatch via `ActionHandlerMap` — app layer provides handlers, engine orchestrates
  - Execution tracking: AutomationRun with per-action results, status (success/failed/skipped/partial)
- [x] **Prism Manifest** (`layer1/manifest/`) — workspace definition file
  - `PrismManifest` — on-disk `.prism.json` containing weak references to Collections in a Vault
  - `CollectionRef` — a manifest's pointer to a typed CRDT collection, optionally with type/tag/sort filters
  - `StorageConfig` — Loro CRDT (default), memory, fs backends (adapted from legacy sqlite/http/postgres)
  - `SchemaConfig` — ordered schema module references (`@prism/core`, relative paths)
  - `SyncConfig` — off/manual/auto modes with peer addresses for CRDT sync
  - `defaultManifest()`, `parseManifest()`, `serialiseManifest()`, `validateManifest()`
  - Collection ref CRUD: `addCollection()`, `removeCollection()`, `updateCollection()`, `getCollection()`
  - Full glossary (Vault/Collection/Manifest/Shell) in `manifest-types.ts` doc comment

### Axed from Legacy

- `WebhookTrigger` / `IntegrationEventTrigger` — Prism uses Tauri IPC, not HTTP endpoints
- `WebhookAction` / `IntegrationAction` — no raw HTTP in Prism architecture
- `FeatureFlagCondition` / `ConfigCondition` — deferred until config system exists
- `IAutomationStore` (async) — simplified to synchronous `AutomationStore` (Loro CRDT is sync)
- `SqliteStorageConfig` / `HttpStorageConfig` / `PostgresStorageConfig` / `IndexedDBStorageConfig` — Prism uses Loro CRDT, not SQL
- `SyncProviderKind` (http/git/dropbox/onedrive/s3) — simplified to peer-based CRDT sync
- `WorkspaceRoster` — deferred; vault discovery is a daemon concern

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (condition-evaluator) | 27 | Pass |
| Vitest (automation-engine) | 17 | Pass |
| Vitest (manifest) | 27 | Pass |
| **Phase 8 Total** | **71** | **All Pass** |

## Next: Phase 9

Candidates:
- Vault Discovery (recent vaults/manifests, daemon-side roster)
- Server Factory (Hono routes from ObjectRegistry — for Relay nodes)
- Config System (ConfigModel, setting layers, plugin settings)
- Undo/Redo System (command-based undo stack integrated with Loro CRDT)
