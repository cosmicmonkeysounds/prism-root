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

## Phase 9: Config System, Undo/Redo System (Complete)

Two infrastructure systems for settings management and interaction history. All pure TypeScript, zero React.

### Completed

- [x] **Config System** (`layer1/config/`) — layered settings with scope resolution
  - `ConfigRegistry` — instance-based catalog of SettingDefinitions + FeatureFlagDefinitions
  - `ConfigModel` — live runtime config with layered scope resolution (default → workspace → user)
  - `SettingDefinition` — typed settings with validation, scope restrictions, secret masking, tags
  - `ConfigStore` interface — synchronous persistence (Loro CRDT is sync)
  - `MemoryConfigStore` — in-process store with `simulateExternalChange()` for testing
  - `attachStore(scope, store)` — auto-load + subscribe to external changes
  - `watch(key, cb)` — observe specific key changes (immediate call + on change)
  - `on('change', cb)` — wildcard listener for all config mutations
  - `toJSON(scope)` — serialization with secret masking ('***')
  - `validateConfig()` — lightweight JSON Schema subset (string/number/boolean/array/object)
  - `coerceConfigValue()` — env var string → typed value coercion
  - `schemaToValidator()` — bridge from declarative schema to SettingDefinition.validate
  - `FeatureFlags` — boolean toggles with config key delegation and condition evaluation
  - Built-in settings: ui (theme, density, language, sidebar, activityBar), editor (fontSize, lineNumbers, spellCheck, indentSize, autosaveMs), sync (enabled, intervalSeconds), ai (enabled, provider, modelId, apiKey), notifications
  - Built-in flags: ai-features (→ ai.enabled), sync (→ sync.enabled)
- [x] **Undo/Redo System** (`layer1/undo/`) — snapshot-based undo stack
  - `UndoRedoManager` — framework-agnostic undo/redo with configurable max history
  - `ObjectSnapshot` — before/after diffs for GraphObject and ObjectEdge
  - `push(description, snapshots)` — record undoable entry, clears redo stack
  - `merge(snapshots)` — coalesce rapid edits into last entry
  - `undo()` / `redo()` — calls applier with snapshot direction
  - `canUndo` / `canRedo` / `undoLabel` / `redoLabel` — UI state queries
  - `subscribe(cb)` — observe stack changes for reactive UI updates
  - Synchronous applier (not async — Loro CRDT operations are sync)

### Axed from Legacy

- `SettingScope: 'app' | 'team'` — Prism is local-first; no server-level or team-level scopes
- `IConfigStore` (async) — simplified to synchronous `ConfigStore` (Loro CRDT is sync)
- `LocalStorageConfigStore` — browser-specific; Prism uses Tauri IPC
- `FeatureFlagCondition: 'user-role' | 'team-plan' | 'env'` — Prism has no team plans or server env vars
- Server/SaaS settings (session timeout, CORS, 2FA, allowed origins) — Relay concerns, not core
- `loadFromModule()` — deferred until schema loader exists

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (config-registry) | 13 | Pass |
| Vitest (config-model) | 28 | Pass |
| Vitest (config-schema) | 29 | Pass |
| Vitest (feature-flags) | 11 | Pass |
| Vitest (undo-manager) | 22 | Pass |
| **Phase 9 Total** | **103** | **All Pass** |

## Phase 10: Server Factory, Undo Bridge (Complete)

Two systems for REST API generation and undo/redo integration. All pure TypeScript, zero React.

### Completed

- [x] **Server Factory** (`layer1/server/`) — framework-agnostic REST route generation
  - `ApiOperation` + `ObjectTypeApiConfig` — types added to EntityDef for declarative API config
  - `generateRouteSpecs(registry)` — reads ObjectRegistry.allDefs() and generates RouteSpec[] for types with `api` config
  - Per-type CRUD routes: list (GET), get (GET /:id), create (POST), update (PUT /:id), delete (DELETE /:id), restore (POST /:id/restore), move (POST /:id/move), duplicate (POST /:id/duplicate)
  - Edge routes: GET/POST/PUT/DELETE /api/edges[/:id], GET /api/objects/:id/related
  - Global object search: GET /api/objects, GET /api/objects/:id
  - `RouteAdapter` interface + `registerRoutes()` — framework-agnostic handler registration
  - `groupByType()`, `printRouteTable()` — utilities for introspection
  - `buildOpenApiDocument()` — OpenAPI 3.1.0 document from RouteSpec[] + ObjectRegistry
  - Per-type component schemas from EntityFieldDef (enum, date, datetime, url, object_ref, bool, int, float, text)
  - GraphObject/ObjectEdge/ResolvedEdge base schemas in components
  - Proper operationIds (listTasks, getTask, createTask, etc.) and tags
  - `generateOpenApiJson()` — serialized OpenAPI document
  - String helpers: `pascal()`, `camel()`, `singular()` (in object-model/str.ts)
- [x] **Undo Bridge** (`layer1/undo/undo-bridge.ts`) — auto-recording TreeModel/EdgeModel mutations
  - `createUndoBridge(manager)` — returns TreeModelHooks + EdgeModelHooks
  - afterAdd: records create snapshot (before=null, after=object)
  - afterRemove: records delete snapshots for object + all descendants
  - afterMove: records move snapshot
  - afterDuplicate: records create snapshots for all copies
  - afterUpdate: records before/after snapshot
  - Edge hooks: afterAdd, afterRemove, afterUpdate — same pattern for ObjectEdge
  - All snapshots are deep copies via structuredClone (mutation-safe)

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (route-gen) | 19 | Pass |
| Vitest (openapi) | 16 | Pass |
| Vitest (undo-bridge) | 11 | Pass |
| **Phase 10 Total** | **46** | **All Pass** |

## Phase 11: CRDT Persistence Layer (Complete)

Two systems for durable CRDT-backed storage and vault lifecycle management. All pure TypeScript, zero React.

### Completed

- [x] **Collection Store** (`layer1/persistence/collection-store.ts`) — Loro CRDT-backed object/edge storage
  - `createCollectionStore(options?)` — wraps a LoroDoc with "objects" + "edges" top-level maps
  - Object CRUD: `putObject()`, `getObject()`, `removeObject()`, `listObjects()`, `objectCount()`
  - Edge CRUD: `putEdge()`, `getEdge()`, `removeEdge()`, `listEdges()`, `edgeCount()`
  - `ObjectFilter` — query by types, tags, statuses, parentId, excludeDeleted
  - Edge filtering by sourceId, targetId, relation
  - Snapshot: `exportSnapshot()`, `exportUpdate(since?)`, `import(data)` — full Loro CRDT sync
  - `onChange(handler)` — subscribe to object/edge mutations via `CollectionChange` events
  - `allObjects()`, `allEdges()`, `toJSON()` — bulk access and debugging
  - Multi-peer sync via peerId option and Loro merge semantics
- [x] **Vault Persistence** (`layer1/persistence/vault-persistence.ts`) — manifest-driven collection lifecycle
  - `PersistenceAdapter` interface — pluggable I/O: `load()`, `save()`, `delete()`, `exists()`, `list()`
  - `createMemoryAdapter()` — in-memory adapter for testing and ephemeral workspaces
  - `createVaultManager(manifest, adapter, options?)` — orchestrates collection stores against persistence
  - Lazy loading: `openCollection(id)` creates + hydrates from disk on first access
  - Dirty tracking: mutations auto-mark collections dirty via `onChange` subscription
  - `saveCollection(id)` / `saveAll()` — persist dirty collections as Loro snapshots
  - `closeCollection(id)` — save + evict from cache
  - `isDirty(id)`, `openCollections()` — introspection
  - Collection paths: `data/collections/{collectionId}.loro`

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (collection-store) | 31 | Pass |
| Vitest (vault-persistence) | 25 | Pass |
| **Phase 11 Total** | **56** | **All Pass** |

## Phase 12: Search Engine (Complete)

Full-text search with TF-IDF scoring and cross-collection structured queries. All pure TypeScript, zero React.

### Completed

- [x] **Search Index** (`layer1/search/search-index.ts`) — in-memory inverted index with TF-IDF scoring
  - `tokenize()` — lowercase tokenizer splitting on whitespace/punctuation, configurable min length
  - `createSearchIndex(options?)` — inverted index mapping tokens to document references
  - Field-weighted scoring: name (3x), type (2x), tags (2x), status (1x), description (1x), data (0.5x)
  - IDF with smoothing: `log(1 + N/df)` — avoids zero scores with single documents
  - Multi-field extraction: indexes name, description, type, tags, status, and string data payload values
  - Add/remove/update/clear per document, removeCollection for bulk eviction
  - Case-insensitive matching, multi-token query support
- [x] **Search Engine** (`layer1/search/search-engine.ts`) — cross-collection search orchestrator
  - `createSearchEngine(options?)` — composes SearchIndex with structured filters
  - Full-text query with TF-IDF relevance scoring across all indexed collections
  - Structured filters: types, tags (AND), statuses, collectionIds, dateAfter/dateBefore, includeDeleted
  - Faceted results: counts by type, collection, and tag (computed from full result set, not just page)
  - Pagination: configurable limit/offset with default page size (50)
  - Sort by relevance (default when query present), name (default otherwise), date, createdAt, updatedAt
  - Auto-indexing: `indexCollection()` subscribes to CollectionStore.onChange for live index updates
  - Live subscriptions: `subscribe(options, handler)` re-runs search on every index change
  - Reindex/remove collection lifecycle management

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (search-index) | 30 | Pass |
| Vitest (search-engine) | 34 | Pass |
| **Phase 12 Total** | **64** | **All Pass** |

## Phase 13: Vault Discovery (Complete)

Vault/manifest registry and filesystem discovery for workspace management. All pure TypeScript, zero React.

### Completed

- [x] **Vault Roster** (`layer1/discovery/vault-roster.ts`) — persistent registry of known vaults
  - `createVaultRoster(store?)` — in-memory registry with optional backing store
  - CRUD: `add()`, `remove()`, `get()`, `getByPath()`, `update()`
  - `touch(id)` — bump `lastOpenedAt` to now on workspace open
  - `pin(id, pinned)` — pin/unpin entries for quick access
  - `list(options?)` — sort by lastOpenedAt/name/addedAt, filter by pinned/tags/search text, limit
  - Pinned entries always float to top within any sort order
  - Path-based deduplication (same vault path → single entry)
  - Change events: `onChange(handler)` with add/remove/update types
  - `RosterStore` interface for pluggable persistence + `createMemoryRosterStore()`
  - `save()` / `reload()` for explicit persistence lifecycle
  - Hydrates from store on creation
- [x] **Vault Discovery** (`layer1/discovery/vault-discovery.ts`) — filesystem scanning for manifests
  - `createVaultDiscovery(adapter, roster?)` — scan + merge orchestrator
  - `DiscoveryAdapter` interface: `listDirectories()`, `readFile()`, `exists()`, `joinPath()`
  - `createMemoryDiscoveryAdapter()` — in-memory adapter for testing
  - `scan(options)` — scan search paths for `.prism.json` files, parse manifests
  - Configurable `maxDepth` (default 1), `mergeToRoster` toggle
  - Also checks search path itself for a manifest (not just children)
  - Automatic roster merge: adds new vaults, updates existing entries on rescan
  - Discovery events: `scan-start`, `scan-complete`, `vault-found`, `scan-error`
  - Scan state tracking: `scanning`, `lastScanAt`, `lastScanCount`

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (vault-roster) | 32 | Pass |
| Vitest (vault-discovery) | 22 | Pass |
| **Phase 13 Total** | **54** | **All Pass** |

## Phase 14: Derived Views (Complete)

View mode definitions, configurable filter/sort/group pipeline, and live materialized projections from CollectionStores. All pure TypeScript, zero React.

### Completed

- [x] **View Definitions** (`layer1/view/view-def.ts`) — view mode registry with capability descriptors
  - `ViewMode` — 7 standard modes: list, kanban, grid, table, timeline, calendar, graph
  - `ViewDef` — per-mode capabilities: supportsSort, supportsFilter, supportsGrouping, supportsColumns, supportsInlineEdit, supportsBulkSelect, supportsHierarchy, requiresDate, requiresStatus
  - `createViewRegistry()` — pre-loaded with 7 built-in defs, extensible via `register()`
  - `supports(mode, capability)` — single capability query
  - `modesWithCapability(capability)` — find all modes with a feature
- [x] **View Config** (`layer1/view/view-config.ts`) — filter/sort/group pure transform pipeline
  - `FilterConfig` — 12 operators: eq, neq, contains, starts, gt, gte, lt, lte, in, nin, empty, notempty
  - `SortConfig` — field + direction, multi-level sort support
  - `GroupConfig` — field-based grouping with collapse state
  - `getFieldValue()` — resolves shell fields first, then data payload
  - `applyFilters()` — AND-combined filter evaluation
  - `applySorts()` — multi-level sort (immutable, returns new array)
  - `applyGroups()` — single-level grouping with insertion-order preservation, __none__ for null/undefined
  - `applyViewConfig()` — full pipeline: excludeDeleted → filters → sorts → limit
- [x] **Live View** (`layer1/view/live-view.ts`) — auto-updating materialized projection
  - `createLiveView(store, options?)` — wraps CollectionStore + ViewConfig
  - `snapshot` — materialized objects, grouped results, total count, type/tag facets
  - Config mutations: `setFilters()`, `setSorts()`, `setGroups()`, `setColumns()`, `setLimit()`, `setMode()`, `setConfig()`
  - `toggleGroupCollapsed(key)` — per-group collapse state management
  - `includes(objectId)` — fast membership check via internal ID set
  - Auto-updates on CollectionStore changes (add/update/remove)
  - `subscribe(listener)` — immediate callback + reactive updates
  - `refresh()` — force re-materialization
  - `dispose()` — detach from store, stop auto-updates

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (view-def) | 17 | Pass |
| Vitest (view-config) | 38 | Pass |
| Vitest (live-view) | 34 | Pass |
| **Phase 14 Total** | **89** | **All Pass** |

## Phase 15: Notification System (Complete)

In-app notification registry with debounced batching and deduplication. All pure TypeScript, zero React.

### Completed

- [x] **Notification Store** (`layer1/notification/notification-store.ts`) — in-memory notification registry
  - `NotificationKind` — 8 kinds: system, mention, activity, reminder, info, success, warning, error
  - `Notification` — id, kind, title, body?, objectId?, objectType?, actorId?, read, pinned, createdAt, readAt?, dismissedAt?, expiresAt?, data?
  - `createNotificationStore(options?)` — full CRUD with eviction policy
  - `add()` — create notification with auto-generated ID and timestamp
  - `markRead()` / `markAllRead(filter?)` — read state management
  - `dismiss()` / `dismissAll(filter?)` — soft-delete with timestamp
  - `pin()` / `unpin()` — pin important notifications
  - `getAll(filter?)` — newest-first, excludes dismissed/expired, filters by kind/read/objectId/since
  - `getUnreadCount(filter?)` — unread count excluding dismissed
  - `subscribe(handler)` — change events (add/update/dismiss)
  - `hydrate(items)` — bulk load from persistence
  - `clear()` — remove dismissed unpinned items, preserve pinned
  - Eviction policy: dismissed unpinned (oldest) → read unpinned (oldest); pinned never evicted
- [x] **Notification Queue** (`layer1/notification/notification-queue.ts`) — debounced batching with dedup
  - `createNotificationQueue(store, options?)` — enqueue → debounce → flush to store
  - `enqueue(input)` — add to pending queue with dedup by (objectId, kind)
  - Debounce: configurable window (default 300ms), timer resets on subsequent enqueue
  - Dedup within queue: same (objectId, kind) → last-write-wins
  - Dedup across flush: within dedupWindowMs (default 5000ms), recently flushed items are skipped
  - `flush()` — manually deliver all pending to store
  - `pending()` — queued count
  - `dispose()` — clear pending, cancel timers
  - Pluggable `TimerProvider` for testing (setTimeout, clearTimeout, now)

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (notification-store) | 36 | Pass |
| Vitest (notification-queue) | 12 | Pass |
| **Phase 15 Total** | **48** | **All Pass** |

## Next: Phase 16

Candidates:
- Federation (cross-Node object addressing, federated edges)
- Batch Operations (bulk create/update/delete with undo support)
- Clipboard / Drag-and-Drop (tree clipboard with cut/copy/paste, drag reorder)
- Template System (object templates, instantiation, template registry)
- Activity Log (audit trail of object mutations with actor/timestamp)
