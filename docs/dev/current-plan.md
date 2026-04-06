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

## Phase 16: Activity Log (Complete)

Audit trail of GraphObject mutations with actor/timestamp. All pure TypeScript, zero React.

### Completed

- [x] **Activity Store** (`layer1/activity/activity-log.ts`) — append-only per-object event log
  - `ActivityVerb` — 20 semantic verbs: created, updated, deleted, restored, moved, renamed, status-changed, commented, mentioned, assigned, unassigned, attached, detached, linked, unlinked, completed, reopened, blocked, unblocked, custom
  - `FieldChange` — before/after record for a single field mutation
  - `ActivityEvent` — immutable audit record with verb-specific fields (changes, fromParentId/toParentId, fromStatus/toStatus, meta)
  - `createActivityStore(options?)` — in-memory ring buffer per object (default 500 events)
  - `record()` — append event with auto-generated id and createdAt
  - `getEvents(objectId, opts?)` — newest-first retrieval with limit and before filters
  - `getLatest()` / `getEventCount()` — quick access queries
  - `hydrate(objectId, events)` — bulk load from persistence (sorts by createdAt)
  - `subscribe(objectId, listener)` — per-object change notifications
  - `toJSON()` / `clear()` — serialisation and reset
- [x] **Activity Tracker** (`layer1/activity/activity-tracker.ts`) — auto-derives events from GraphObject diffs
  - `TrackableStore` — duck-typed subscription interface (structurally compatible with CollectionStore)
  - `createActivityTracker(options)` — watches objects via per-object subscriptions
  - `track(objectId, store)` — begin watching; diffs snapshots on each emission
  - Verb inference: deleted/restored (deletedAt), moved (parentId), renamed (only name), status-changed (status field), updated (fallback)
  - Shell field diffing + data payload one-level-deep diffing
  - `ignoredFields` config (default: updatedAt) to filter noise
  - Handles object appeared (created if age < 5s) and disappeared (hard delete)
  - `untrackAll()` — stop all subscriptions, `trackedIds()` — introspection
- [x] **Activity Formatter** (`layer1/activity/activity-formatter.ts`) — human-readable event rendering
  - `formatFieldName(field)` — raw field path to display label (data. prefix strip, camelCase split, overrides)
  - `formatFieldValue(value)` — inline display formatting (null → "(none)", booleans, arrays, ISO dates, truncation)
  - `formatActivity(event, opts?)` — text + HTML description for all 20 verbs
  - `groupActivityByDate(events)` — Today/Yesterday/This week/Earlier buckets for timeline rendering

### Axed from Legacy

- `ActivityStore` class — replaced with factory function (Prism convention)
- `storageKey` option — Prism uses Loro CRDT for persistence, not localStorage
- `ITrackableStore` class-based — replaced with `TrackableStore` structural interface

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (activity-log) | 21 | Pass |
| Vitest (activity-tracker) | 21 | Pass |
| Vitest (activity-formatter) | 51 | Pass |
| **Phase 16 Total** | **93** | **All Pass** |

## Phase 17: Batch Operations, Clipboard, Templates (Complete)

Utility Layer 1 systems for bulk manipulation and reuse. All pure TypeScript, zero React.

### Completed

- [x] **Batch Operations** (`layer1/batch/`)
  - `BatchOp` — 7 operation kinds: create-object, update-object, delete-object, move-object, create-edge, update-edge, delete-edge
  - `createBatchTransaction(options)` — collect ops, validate, execute atomically
  - `validate()` — pre-flight checks (missing IDs, types, EdgeModel presence)
  - `execute(options?)` — apply all mutations, push single undo entry, rollback on failure
  - `BatchProgressCallback` — called before each op with current/total/op
  - Undo integration: entire batch = one UndoRedoManager.push() call
- [x] **Clipboard** (`layer1/clipboard/`)
  - `createTreeClipboard(options)` — cut/copy/paste for GraphObject subtrees
  - `copy(ids)` — deep-clone subtrees with descendants + internal edges
  - `cut(ids)` — copy + delete sources on paste (one-time)
  - `paste(options?)` — remap all IDs, reattach under target parent, recreate internal edges
  - `SerializedSubtree` — portable snapshot: root + descendants + internalEdges
  - `PasteResult` — created objects, created edges, oldId→newId map
  - Single undo entry for paste (includes cut deletions)
- [x] **Template System** (`layer1/template/`)
  - `createTemplateRegistry(options)` — catalog of reusable ObjectTemplates
  - `ObjectTemplate` — blueprint: root TemplateNode tree + TemplateEdge[] + TemplateVariable[]
  - `register(template)` / `unregister(id)` / `list(filter?)` — CRUD with category/type/search filtering
  - `instantiate(templateId, options?)` — create live objects from template with variable interpolation
  - `createFromObject(objectId, meta)` — snapshot existing subtree as reusable template (round-trip capable)
  - Variable interpolation: `{{name}}`, `{{date}}` in name, description, status, data string values
  - Undo integration: single entry for entire instantiation

### Axed from Phase 18 Draft

Phase 18's draft content has been promoted to Phase 17 and completed. The remaining phases (19+) retain their numbering.

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (batch-transaction) | 24 | Pass |
| Vitest (tree-clipboard) | 21 | Pass |
| Vitest (template-registry) | 28 | Pass |
| **Phase 17 Total** | **73** | **All Pass** |

---

## Phase 18: Ephemeral Presence (Complete)

Real-time collaboration awareness. All pure TypeScript, zero React.

### Completed

- [x] **Ephemeral Presence** (`layer1/presence/`)
  - `PresenceState` — cursor position, selection ranges, active view, peer identity, arbitrary data
  - `PeerIdentity` — peerId, displayName, color, optional avatarUrl
  - `CursorPosition` — objectId + optional field + offset for text cursor tracking
  - `SelectionRange` — objectId + optional field + anchor/head for inline selection
  - `createPresenceManager(options)` — RAM-only state for connected peers (no CRDT persistence)
  - `setCursor()` / `setSelections()` / `setActiveView()` / `setData()` — local state updates
  - `updateLocal(partial)` — bulk update of local presence fields
  - `receiveRemote(state)` — ingest remote peer state (from awareness protocol)
  - `removePeer(peerId)` — explicit peer removal
  - `subscribe(listener)` — reactive updates for cursor/selection overlays (joined/updated/left)
  - TTL-based eviction: configurable `ttlMs` + automatic `sweepIntervalMs` sweep timer
  - `sweep()` — manual eviction trigger, returns evicted peer IDs
  - `dispose()` — stop sweep timer, remove all remote peers, clear listeners
  - Injectable `TimerProvider` for deterministic testing

### Note

Phase 18's original draft (Batch/Clipboard/Template) was promoted to Phase 17 and completed there. This phase number now covers Ephemeral Presence (previously Phase 19). Subsequent phases retain their original numbering but shift down by one.

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (presence-manager) | 38 | Pass |
| **Phase 18 Total** | **38** | **All Pass** |

---

## Phase 19: Identity & Encryption ✅

W3C DIDs and vault-level encryption. Foundation for federation and trust.

- [x] **DID Identity** (`layer1/identity/`)
  - [x] `PrismIdentity` — W3C DID document wrapper (did:web, did:key)
  - [x] `createIdentity()` — generate Ed25519 keypair + DID document
  - [x] `resolveIdentity(did)` — resolve DID to public key and metadata
  - [x] Threshold multi-sig support for shared vault ownership (createMultiSigConfig, createPartialSignature, assembleMultiSignature, verifyMultiSignature)
  - [x] `signPayload()` / `verifySignature()` — Ed25519 sign/verify for CRDT updates
  - [x] Base58btc + multicodec encoding for did:key format
- [x] **Vault Encryption** (`layer1/encryption/`)
  - [x] `VaultKeyManager` — HKDF-derived AES-GCM-256 vault key from identity keypair
  - [x] `encryptSnapshot()` / `decryptSnapshot()` — encrypt Loro snapshots at rest
  - [x] Per-collection encryption with key rotation support (deriveCollectionKey, rotateKey)
  - [x] Secure key storage integration (KeyStore interface for Tauri keychain / Secure Enclave bridge, createMemoryKeyStore for testing)
  - [x] AAD (Additional Authenticated Data) support for binding ciphertext to collection context
  - [x] Standalone encryptSnapshot/decryptSnapshot for one-off encryption without VaultKeyManager

### Implementation Notes

- All crypto via Web Crypto API (SubtleCrypto) — works in Node.js 20+, browsers, Tauri WebView
- Ed25519 for signing (64-byte signatures), AES-GCM-256 for encryption, HKDF-SHA-256 for key derivation
- DID:key uses multibase z-prefix + base58btc + Ed25519 multicodec (0xed01) per W3C spec
- DID:web builds proper `did:web:domain:path` URIs; resolution requires network resolver (interface ready, not yet wired)
- Multi-sig uses threshold scheme: collect N-of-M partial Ed25519 signatures, verify each individually
- Key rotation derives new key from existing material + version-tagged salt — old ciphertext needs old key version

### Files

- `identity-types.ts` — DID, DIDDocument, PrismIdentity, MultiSigConfig, KeyHandle types
- `identity.ts` — createIdentity, resolveIdentity, signPayload, verifySignature, multi-sig functions, base58btc codec
- `encryption-types.ts` — VaultKeyInfo, EncryptedSnapshot, KeyStore, VaultKeyManager types
- `encryption.ts` — createVaultKeyManager, createMemoryKeyStore, standalone encrypt/decrypt

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (identity) | 29 | Pass |
| Vitest (encryption) | 22 | Pass |
| **Phase 19 Total** | **51** | **All Pass** |

---

## Phase 21: Virtual File System ✅

Decouples the object-graph (text/CRDTs) from heavy binary assets.

- [x] **VFS Layer** (`layer1/vfs/`)
  - [x] `VfsAdapter` interface — abstract file I/O (read, write, stat, list, delete, has, count, totalSize)
  - [x] `createMemoryVfsAdapter()` — in-memory adapter for testing
  - [x] `createLocalVfsAdapter()` — VfsAdapter interface ready; Tauri impl deferred to daemon phase
  - [x] `BinaryRef` — content-addressed reference (SHA-256 hash) stored in GraphObject.data
  - [x] Binary Forking Protocol: acquireLock/releaseLock/replaceLockedFile for non-mergeable files
  - [x] `importFile()` / `exportFile()` — move binaries in/out of vault storage via VfsManager
  - [x] Deduplication via content addressing (same SHA-256 hash = one blob)
  - [x] `computeBinaryHash()` — standalone SHA-256 hash utility

### Implementation Notes

- SHA-256 content addressing via Web Crypto API, hex-encoded (64 chars)
- VfsManager wraps VfsAdapter with lock management + import/export convenience
- Binary Forking Protocol: lock → edit → replaceLockedFile (new blob, moved lock, old preserved) → release
- dispose() clears locks only; blobs persist for history/undo

### Files

- `vfs-types.ts` — BinaryRef, FileStat, BinaryLock, VfsAdapter, VfsManager types
- `vfs.ts` — createMemoryVfsAdapter, createVfsManager, computeBinaryHash

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (vfs) | 34 | Pass |
| **Phase 21 Total** | **34** | **All Pass** |

---

## Next: Phase 22: Federation & Sync

Cross-node object addressing, CRDT sync, and ghost nodes.

- [ ] **Federated Addressing** (`layer1/federation/`)
  - [ ] `PrismAddress` resolution — `prism://did:web:node/objects/id` → local or remote fetch
  - [ ] `GhostNode` — locked placeholder for objects in unshared collections
  - [ ] `FederatedEdge` — edge where source and target live on different nodes
  - [ ] `resolveRemoteObject()` — fetch object snapshot from peer via Relay or direct
- [ ] **Sync Engine** (`layer1/sync/`)
  - [ ] `SyncSession` — bidirectional Loro CRDT sync between two peers
  - [ ] `SyncTransport` interface — pluggable transport (WebSocket, Tauri IPC, Relay)
  - [ ] `createDirectSyncTransport()` — peer-to-peer WebSocket transport
  - [ ] `createRelaySyncTransport()` — store-and-forward via Prism Relay
  - [ ] Conflict quarantine: detect divergent non-CRDT state, surface for manual resolution

## Phase 23: Prism Relay ✅

Modular, composable relay runtime. The Relay is the bridge between Core/Daemon and the outside world — NOT just a server. Users mix and match Web 1/2/3 features via builder pattern. Next.js is optional (for Sovereign Portals).

- [x] **Relay Builder** (`layer1/relay/`)
  - [x] `createRelayBuilder()` — composable builder with `.use()` chaining and `.configure()` overrides
  - [x] `RelayModule` interface — pluggable modules with name/description/dependencies/install/start/stop lifecycle
  - [x] `RelayContext` — shared capability registry for inter-module communication
  - [x] `RelayInstance` — built relay with start/stop lifecycle, capability access, module listing
  - [x] Dependency validation at build time (missing deps, duplicate modules)
  - [x] `RELAY_CAPABILITIES` — well-known capability names for standard modules
- [x] **Blind Mailbox** (module: `blind-mailbox`)
  - [x] E2EE store-and-forward message queue — deposit/collect/pendingCount/evict
  - [x] TTL-based expiry eviction for stale envelopes
  - [x] `RelayEnvelope` — encrypted payload with from/to DID, TTL, optional proof-of-work
- [x] **Relay Router** (module: `relay-router`, depends on blind-mailbox)
  - [x] Zero-knowledge routing: delivers to online peers or queues to mailbox
  - [x] `registerPeer()` — flushes queued envelopes when peer comes online
  - [x] Rejects oversized envelopes (configurable max size)
- [x] **Relay Timestamping** (module: `relay-timestamp`)
  - [x] `stamp()` — cryptographic Ed25519-signed timestamp receipts for data hashes
  - [x] `verify()` — validate receipt signatures
- [x] **Blind Pings** (module: `blind-pings`)
  - [x] Content-free push notifications with pluggable `PingTransport` (APNs, FCM, etc.)
  - [x] `createMemoryPingTransport()` for testing
- [x] **Capability Tokens** (module: `capability-tokens`)
  - [x] Scoped access tokens with Ed25519 signatures — issue/verify/revoke
  - [x] TTL-based expiry, wildcard subjects, tamper detection
- [x] **Webhooks** (module: `webhooks`)
  - [x] Register/unregister/list webhooks with event filtering and wildcard support
  - [x] Pluggable `WebhookHttpClient` for outgoing HTTP; dry-run mode without client
  - [x] HMAC-SHA256 payload signatures, delivery logging
- [x] **Sovereign Portals** (module: `sovereign-portals`)
  - [x] `PortalRegistry` — register/unregister/list/resolve portals
  - [x] Portal levels 1-4 (read-only → complex webapp)
  - [x] Domain + path resolution for routing requests to portals
  - [x] SSR/AutoREST/Next.js integration deferred to `packages/prism-relay/` runtime package

### Implementation Notes

- Relay is a Layer 1 module (agnostic TS) — the actual server runtime (`packages/prism-relay/`) will import these primitives
- Builder pattern central: `createRelayBuilder({ relayDid }).use(mod1()).use(mod2()).build()`
- "Choose your own adventure": Web 1.0 (just portals), Web 2.0 (portals + webhooks), full (all 7 modules), or custom
- Custom modules implement `RelayModule` interface and register capabilities via `RelayContext`
- All crypto uses existing identity module (Ed25519 signing for timestamps/tokens)
- Zero-knowledge: router sees `RelayEnvelope` with encrypted ciphertext, never plaintext

### Files

- `relay-types.ts` — RelayEnvelope, BlindMailbox, RelayRouter, RelayTimestamper, BlindPinger, CapabilityToken/Manager, WebhookEmitter, PortalRegistry, RelayModule, RelayContext, RelayBuilder, RelayInstance types
- `relay.ts` — createRelayBuilder, 7 module factories, createMemoryPingTransport

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Vitest (relay) | 44 | Pass |
| **Phase 23 Total** | **44** | **All Pass** |

---

## Phase 24: Actor System (0B/0C) (Complete)

Process queue, language runtimes, and local AI integration.

- [x] **Process Queue** (`layer1/actor/`)
  - [x] `createProcessQueue()` — priority-ordered execution with concurrency control, auto-processing, cancel/prune/dispose
  - [x] `ActorRuntime` interface — pluggable language execution with capability-scoped sandboxing
  - [x] `createLuaActorRuntime()` — wraps wasmoon via function injection (no hard WASM dependency)
  - [x] `createSidecarRuntime()` — TypeScript/Python via `SidecarExecutor` interface (Daemon provides Tauri shell)
  - [x] `createTestRuntime()` — synchronous in-memory runtime for testing
  - [x] `CapabilityScope` — zero-trust by default, explicit permission grants per task (network, fs, crdt, spawn, endpoints, duration/memory limits)
  - [x] Queue events: enqueued/started/completed/failed/cancelled with subscribe/unsubscribe
- [x] **Intelligence Layer** (0C)
  - [x] `AiProvider` interface — pluggable providers with name/target/defaultModel/complete/completeInline/listModels/isAvailable
  - [x] `createAiProviderRegistry()` — register, switch active, delegate complete/completeInline
  - [x] `createOllamaProvider()` — local Ollama inference via injected `AiHttpClient` (chat + inline fill-in-the-middle)
  - [x] `createExternalProvider()` — OpenAI-compatible API bridge for Claude, OpenAI, etc. with Bearer auth
  - [x] `createContextBuilder()` — object-aware context from graph neighbors (ancestors/children/edges/collection) with configurable limits, `toSystemMessage()` for AI prompts
  - [x] `createTestAiProvider()` — canned response provider for testing
  - [x] `AiHttpClient` interface — abstracts HTTP calls to avoid fetch dependency in Layer 1

### Implementation Notes
- Three execution targets: Sovereign Local, Federated Delegate, External Provider
- Actor types in `actor-types.ts`, AI types in `ai-types.ts` (separate concerns)
- All HTTP calls abstracted behind interfaces (AiHttpClient, SidecarExecutor) — Layer 1 has no runtime dependencies
- ProcessQueue supports priority ordering (lower = higher), concurrency control, and fire-and-forget auto-processing

### Test Summary (49 tests)
| Suite | Tests | Status |
|-------|-------|--------|
| ProcessQueue basics | 14 | Pass |
| Auto-processing | 2 | Pass |
| LuaActorRuntime | 4 | Pass |
| SidecarRuntime | 3 | Pass |
| AiProviderRegistry | 7 | Pass |
| OllamaProvider | 7 | Pass |
| ExternalProvider | 4 | Pass |
| ContextBuilder | 4 | Pass |
| TestAiProvider | 3 | Pass |
| **Phase 24 Total** | **49** | **All Pass** |

---

## Phase 25: Prism Syntax Engine (Complete)

LSP-like intelligence for the expression and scripting layers.

- [x] **Syntax Engine** (`layer1/syntax/`)
  - [x] `createSyntaxEngine()` — orchestrates SyntaxProviders for diagnostics, completions, hover
  - [x] `createExpressionProvider()` — built-in provider for the Prism expression language
  - [x] Diagnostics: parse errors with source positions, unknown fields/functions, wrong arity, type mismatches
  - [x] Completions: fields (from SchemaContext), functions (9 builtins), keywords, operators with prefix filtering
  - [x] Hover: field type/description/enum values/computed expressions, function signatures, literals, keyword operators
  - [x] `inferNodeType()` — AST type inference mapping EntityFieldType → ExprType via FIELD_TYPE_MAP
  - [x] `validateTypes()` — schema-aware type checking (arithmetic on strings, unknown fields, wrong arity)
  - [x] `generateLuaTypeDef()` — .d.lua generation from ObjectRegistry schemas with @class/@field annotations, enum unions, optional markers, standard GraphObject fields, builtin function stubs
  - [x] `SyntaxProvider` interface for custom language providers (pluggable beyond expression)
  - [x] CodeMirror integration ready: TextRange positions compatible with CM offsets (Layer 2 wiring deferred)

### Implementation Notes
- Types in `syntax-types.ts`, implementation in `syntax.ts` (separation of concerns)
- FIELD_TYPE_MAP maps all 11 EntityFieldTypes to ExprType (number/boolean/string)
- BUILTIN_FUNCTIONS defines 9 functions with param types for completion detail and hover
- SchemaContext provides the bridge from ObjectRegistry to the syntax engine
- SyntaxProvider interface allows adding Lua/TypeScript language providers in future phases

### Test Summary (68 tests)
| Suite | Tests | Status |
|-------|-------|--------|
| Expression diagnostics | 9 | Pass |
| Expression completions | 9 | Pass |
| Expression hover | 10 | Pass |
| Type inference | 15 | Pass |
| SyntaxEngine | 9 | Pass |
| Lua typedef generation | 7 | Pass |
| FIELD_TYPE_MAP | 4 | Pass |
| Edge cases | 5 | Pass |
| **Phase 25 Total** | **68** | **All Pass** |

## Phase 26: Communication Fabric (Complete)

Real-time sessions, transcription, and A/V transport.

- [x] **Session Nodes** (`layer1/session/`)
  - [x] `createSessionManager()` — session lifecycle (create/join/end/pause/resume) with participant/track/delegation management
  - [x] `createTranscriptTimeline()` — ordered, searchable, time-indexed transcript segments with binary-insert sort, finalization, range queries, text search, plain text export
  - [x] `createPlaybackController()` — transcript-synced media seek with speed control (0.25x–4x), seekToSegment, position listeners
  - [x] Self-Dictation: `TranscriptionProvider` interface for Whisper.cpp sidecar integration (Tauri provides the executor)
  - [x] Hypermedia Playback: `seekToSegment()` jumps playback to transcript segment start time
  - [x] Listener Fallback: `requestDelegation()`/`respondToDelegation()` for compute delegation to capable peers
- [x] **A/V Transport**
  - [x] `SessionTransport` interface — abstract transport for LiveKit (SFU), WebRTC (P2P), or custom
  - [x] `createTestTransport()` — in-memory transport for testing (connect/disconnect/publish/unpublish/events)
  - [x] `createTestTranscriptionProvider()` — test provider with `feedSegment()` for simulating transcription
  - [x] `MediaTrack` management — add/remove/mute tracks with participant activeMedia sync
  - [x] Transport events: connected/disconnected/participant-joined/left/track-published/unpublished/muted/unmuted/data-received

### Implementation Notes
- Types in `session-types.ts`, implementation in `session.ts` (separation of concerns)
- All external dependencies abstracted behind interfaces: SessionTransport (LiveKit/WebRTC), TranscriptionProvider (Whisper.cpp)
- SessionManager is a pure state machine — no network I/O, no timers (transport layer handles that)
- TranscriptTimeline uses binary insert for O(log n) sorted insertion by startMs
- Non-final segments can be updated in place (streaming transcription refinement)
- Participant roles: host, speaker, listener, observer
- Delegation targets participants with `canDelegate: true`

### Test Summary (64 tests)
| Suite | Tests | Status |
|-------|-------|--------|
| TranscriptTimeline | 15 | Pass |
| PlaybackController | 9 | Pass |
| TestTransport | 5 | Pass |
| TestTranscriptionProvider | 4 | Pass |
| SessionManager lifecycle | 7 | Pass |
| SessionManager participants | 5 | Pass |
| SessionManager media tracks | 5 | Pass |
| SessionManager transcript | 3 | Pass |
| SessionManager delegation | 5 | Pass |
| SessionManager events | 6 | Pass |
| **Phase 26 Total** | **64** | **All Pass** |

## Phase 27: Trust & Safety (Complete)

The Sovereign Immune System — sandbox, spam protection, content trust.

- [x] **Lua Sandbox** — `createLuaSandbox()` capability-based API restriction per plugin with glob URL/path filtering, violation recording
- [x] **Schema Validation** — `createSchemaValidator()` 5 built-in rules (max-depth, max-string-length, max-array-length, max-total-keys, disallowed-keys for __proto__/constructor/prototype)
- [x] **Relay Spam Protection** — `createHashcashMinter()`/`createHashcashVerifier()` SHA-256 proof-of-work via Web Crypto with configurable difficulty bits
- [x] **Web of Trust** — `createPeerTrustGraph()` peer reputation scoring with configurable thresholds, trust/distrust/ban, content hash flagging, event listeners
- [x] **Secure Recovery** — `createShamirSplitter()` GF(256) Shamir secret sharing with configurable threshold/total shares
- [x] **Relay Encrypted Escrow** — `createEscrowManager()` deposit/claim/evict lifecycle with TTL expiry

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Lua Sandbox | 10 | Pass |
| Schema Validator | 10 | Pass |
| Hashcash | 8 | Pass |
| Peer Trust Graph | 10 | Pass |
| Shamir Secret Sharing | 8 | Pass |
| Escrow Manager | 7 | Pass |
| **Phase 27 Total** | **53** | **All Pass** |

## Phase 28: Builder 3 (3D Viewport) (Complete — Layer 2)

R3F-based 3D editor for spatial content.

- [x] **3D Viewport** (`layer2/viewport3d/`)
  - [x] R3F + @react-three/drei scene graph (types, SceneNode/SceneGraph model)
  - [x] OpenCASCADE.js for CAD geometry (STEP/IGES import, tessellation, bounding box, mesh merge)
  - [x] TSL shader compilation (Three.js Shading Language → WebGPU/WebGL, node graph → GLSL)
  - [x] Loro-backed scene state: object transforms, materials, hierarchy in CRDT
  - [x] Gizmo controls: translate, rotate, scale with undo integration

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Scene State | 19 | Pass |
| CAD Geometry | 15 | Pass |
| TSL Compiler | 19 | Pass |
| Gizmo Controls | 18 | Pass |
| **Phase 28 Total** | **71** | **All Pass** |

## Phase 29: NLE / Timeline System (Grip) (Complete — Layer 1)

Non-linear editing and show control for live production.

- [x] **Timeline Engine** (`layer1/timeline/`)
  - [x] Pluggable `TimelineClock` abstraction (Layer 2 provides tone.js/rAF, tests use `ManualClock`)
  - [x] Track model: 5 kinds (audio, video, lighting, automation, midi) with mute/solo/lock/gain
  - [x] Clip model: time-range regions with sourceRef, sourceOffset, trim, move between tracks, lock/mute/gain
  - [x] Transport controls: play, pause, stop, seek, scrub, setSpeed, loop regions
  - [x] Automation lanes: step/linear/bezier interpolation, per-parameter breakpoint curves
  - [x] Timeline markers: sorted by time, custom colors
  - [x] Tempo map (PPQN): dual time model (seconds ↔ bar/beat/tick), tempo automation, time signature changes
  - [x] Event system: 14 event kinds with subscribe/unsubscribe
  - [x] Reference: OpenDAW SDK (naomiaro/opendaw-test) for Layer 2 audio integration
- [x] **Audio Pipeline** (`layer2/audio/`) — OpenDAW SDK bridge
  - [x] `createOpenDawBridge()` — bidirectional sync between Prism timeline and OpenDAW engine
  - [x] Track loading: AudioFileBox, AudioRegionBox, PPQN conversion, sample provider
  - [x] Transport sync: AnimationFrame position → Prism scrub, timeline events → OpenDAW play/stop
  - [x] 10 audio effects via EffectFactories (Reverb, Compressor, Delay, Crusher, EQ, etc.)
  - [x] Volume/pan/mute/solo per-track control
  - [x] Export: full mix and individual stems to WAV via AudioOfflineRenderer
  - [x] React hooks: useOpenDawBridge, usePlaybackPosition, useTransportControls, useTrackEffects
  - [x] Reference fork: cosmicmonkeysounds/opendaw-prism
  - [ ] peaks.js / waveform-playlist for waveform rendering (future)
  - [ ] WAM (Web Audio Modules) standard for VST-like plugins (future)
- [ ] **Video Pipeline** (Layer 2 — future)
  - [ ] WebCodecs API for frame-accurate seeking
  - [ ] Proxy workflow: low-res edit → full-res export
- [ ] **Hardware Bridges** (Rust daemon — future)
  - [ ] Art-Net (DMX lighting control)
  - [ ] VISCA over IP (PTZ camera control)
  - [ ] OSC (Open Sound Control)
  - [ ] MIDI (instrument/controller I/O)

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Transport | 10 | Pass |
| Tracks | 8 | Pass |
| Clips | 10 | Pass |
| Automation | 8 | Pass |
| Markers | 6 | Pass |
| Queries | 4 | Pass |
| Events | 1 | Pass |
| Lifecycle | 1 | Pass |
| ManualClock | 6 | Pass |
| TempoMap | 10 | Pass |
| **Phase 29 Total** | **67** | **All Pass** |

## Phase 30: Ecosystem Apps — Flux (Complete — Layer 1 Domain Schemas)

Operational hub: productivity, finance, CRM, goals, inventory.

- [x] **Flux Domain Schemas** (`layer1/flux/`)
  - [x] 11 EntityDef schemas: Task, Project, Goal, Milestone, Contact, Organization, Transaction, Account, Invoice, Item, Location
  - [x] 4 categories: productivity, people, finance, inventory
  - [x] 7 edge types: assigned-to, depends-on, blocks, belongs-to, related-to, invoiced-to, stored-at
  - [x] 8 automation presets: task completion timestamps, recurring task reset, overdue notifications, invoice overdue, low/out-of-stock alerts, goal progress tracking, project completion
  - [x] Computed fields: invoice tax/total, item stock value, goal progress formulas
  - [x] CRM fields on Contact: deal value, deal stage pipeline (prospect→closed)
  - [x] Import/export: CSV and JSON with field selection
  - [x] NSIDs for all entity and edge types (io.prismapp.flux.*)
- [ ] **Flux App** (`packages/prism-flux/` — future)
  - [ ] Lens plugins: Tasks, Contacts, Projects, Goals, Finance, Inventory
  - [ ] Dashboard views: kanban, calendar, timeline per entity type

### Test Summary

| Suite | Tests | Status |
|-------|-------|--------|
| Entity Definitions | 12 | Pass |
| Edge Definitions | 7 | Pass |
| Automation Presets | 5 | Pass |
| CSV Export/Import | 6 | Pass |
| JSON Export/Import | 4 | Pass |
| Edge Cases | 2 | Pass |
| **Phase 30 Total** | **38** | **All Pass** |

## Phase 31: Ecosystem Apps — Lattice

Game middleware suite: narrative, audio, entity authoring, world topology.

- [ ] **Lattice App** (`packages/prism-lattice/`)
  - [ ] **Loom** — Narrative engine: unified entry resolution, Fact Store (Ledger/Var), `.loom` format
  - [ ] **Canto** — Audio middleware: Sound Objects, signal graph, spatial audio, acoustic scenes
  - [ ] **Simulacra** — Entity authoring: game object system, `.sim` format, component slots, codegen
  - [ ] **Topology** — World navigation: scenes, regions, portals, state transitions
  - [ ] **Kami** — AI middleware: behavior trees, HSM, GOAP planners
  - [ ] **Cue** — Event orchestration: timeline editor, sync animations/dialogue/audio
  - [ ] **Meridian** — Stats: axes, pools, skill trees, conditions
  - [ ] **Palette** — Inventory: items, loot tables, equipment slots
  - [ ] **Boon** — Abilities: skills, cooldowns, activation rules

## Phase 32: Ecosystem Apps — Cadence & Grip

Music education and live production.

- [ ] **Cadence App** (`packages/prism-cadence/`)
  - [ ] LMS: courses, lessons, assignments, student progress
  - [ ] Interactive music lessons with CRDT-synced notation
  - [ ] Practice tracking and repertoire management
- [ ] **Grip App** (`packages/prism-grip/`)
  - [ ] 3D stage plot editor (Builder 3 + venue templates)
  - [ ] NLE timeline for show programming (Phase 28 integration)
  - [ ] Hardware control dashboard: DMX, PTZ, audio mixer
  - [ ] Show runtime: cue-to-cue execution with hardware sync
