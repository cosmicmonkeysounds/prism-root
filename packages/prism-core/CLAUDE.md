# @prism/core

Client-side execution environment — Layer 1 (agnostic TS) + Layer 2 (React renderers).

## Build
- `pnpm typecheck` / `pnpm test` / `pnpm test:watch`

## Architecture
- **Layer 1**: Object Model (GraphObject, ObjectRegistry, TreeModel, EdgeModel, WeakRefEngine, ContextEngine, NSID), Lens System (LensRegistry, ShellStore), Plugin System (PrismPlugin, PluginRegistry, ContributionRegistry), Reactive Atoms (PrismBus, AtomStore, ObjectAtomStore, bus-to-atom bridges), State Machines (Machine, createMachine — flat FSM with guards/actions/lifecycle), Graph Analysis (dependency graph, topological sort, cycle detection, blocking chains, slip impact), Planning Engine (CPM — critical path, early/late start/finish, float), Input System (KeyboardModel, InputScope, InputRouter), Layout System (SelectionModel, PageModel, PageRegistry, WorkspaceSlot, WorkspaceManager), Forms & Validation (FieldSchema, DocumentSchema, FormState, wiki-links, markdown), Expression Engine (Scanner, Parser, Evaluator), Automation Engine (AutomationEngine, triggers/conditions/actions, condition evaluator, template interpolation), Manifest (PrismManifest — weak references to Collections inside a Vault, StorageConfig, SchemaConfig, SyncConfig, CollectionRef), Config System (ConfigRegistry, ConfigModel, FeatureFlags — layered scope resolution: default→workspace→user), Undo/Redo (UndoRedoManager — snapshot-based undo with merge/batch, createUndoBridge for auto-recording TreeModel/EdgeModel mutations), Server Factory (generateRouteSpecs, buildOpenApiDocument — framework-agnostic REST route generation + OpenAPI 3.1.0 from ObjectRegistry), CRDT Persistence (CollectionStore — Loro-backed object/edge storage with filtering and sync, VaultManager — manifest-driven collection lifecycle with PersistenceAdapter), Search Engine (SearchIndex — TF-IDF inverted index with field-weighted scoring, SearchEngine — cross-collection search orchestrator with structured filters, facets, pagination, live subscriptions), Vault Discovery (VaultRoster — persistent registry of known vaults with sort/pin/search/persistence, VaultDiscovery — filesystem scanning for .prism.json manifests with roster merge and discovery events), Derived Views (ViewRegistry — 7 view modes with capability descriptors, ViewConfig — filter/sort/group pipeline with 12 filter operators, LiveView — auto-updating materialized projection from CollectionStore with facets/groups/subscriptions), Notification System (NotificationStore — in-memory notification registry with 8 kinds, read/dismiss/pin, filtering, eviction policy, subscriptions; NotificationQueue — debounced batching with dedup by objectId+kind), Zustand stores, Loro bridge, Lua runtime (wasmoon), XState machines
- **Layer 2**: CodeMirror 6 (LoroText sync), Puck (Loro layout), KBar (focus-depth), Spatial Graph (@xyflow/react + elkjs), Shell (ShellLayout, ActivityBar, TabBar, LensProvider)
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
- `@prism/core/object-model` — Object Model (GraphObject, registry, tree, edges, weak refs, NSID, query)
- `@prism/core/workspace` — Lens system + shell state (LensManifest, LensRegistry, ShellStore, tab/panel management). NOT the spec's "Workspace" (Manifest → Collections); this is IDE shell infrastructure.
- `@prism/core/shell` — Shell React components (ShellLayout, ActivityBar, TabBar, LensProvider)
- `@prism/core/input` — Input System (KeyboardModel, InputScope, InputRouter)
- `@prism/core/forms` — Forms & Validation (FieldSchema, DocumentSchema, FormState, wiki-links, markdown)
- `@prism/core/layout` — Layout System (SelectionModel, PageModel, PageRegistry, WorkspaceSlot, WorkspaceManager)
- `@prism/core/expression` — Expression Engine (Scanner, Parser, Evaluator with builtins)
- `@prism/core/plugin` — Plugin System (PrismPlugin, PluginRegistry, ContributionRegistry for views/commands/keybindings/menus)
- `@prism/core/atom` — Reactive Atoms (PrismBus event bus, AtomStore UI state, ObjectAtomStore object/edge cache, bus-to-atom bridges)
- `@prism/core/automaton` — State Machines (Machine flat FSM with guards/actions/lifecycle hooks, createMachine factory with start/restore)
- `@prism/core/graph-analysis` — Dependency Graph (topological sort, cycle detection, blocking chains, slip impact) + Planning Engine (CPM critical path method)
- `@prism/core/automation` — Automation Engine (triggers, conditions, actions, condition evaluator, template interpolation, cron scheduling)
- `@prism/core/manifest` — Prism Manifest (weak references to Collections in a Vault; StorageConfig, SchemaConfig, SyncConfig, CollectionRef, parse/serialise/validate). See manifest-types.ts for Vault/Collection/Manifest/Shell glossary.
- `@prism/core/config` — Config System (ConfigRegistry, ConfigModel, FeatureFlags, validateConfig, coerceConfigValue, schemaToValidator, MemoryConfigStore). Layered scope resolution: default → workspace → user. Built-in settings for ui/editor/sync/ai/notifications.
- `@prism/core/undo` — Undo/Redo (UndoRedoManager — snapshot-based undo/redo with merge for rapid edits, batch support, configurable max history, subscribe for UI reactivity, createUndoBridge for auto-recording TreeModel/EdgeModel mutations)
- `@prism/core/server` — Server Factory (generateRouteSpecs — framework-agnostic RouteSpec[] from ObjectRegistry, registerRoutes + RouteAdapter, buildOpenApiDocument/generateOpenApiJson — OpenAPI 3.1.0 document generation, groupByType, printRouteTable)
- `@prism/core/persistence` — CRDT Persistence (createCollectionStore — Loro-backed object/edge storage with ObjectFilter queries and CRDT sync, createVaultManager — manifest-driven collection lifecycle with lazy loading/dirty tracking/save, PersistenceAdapter interface, createMemoryAdapter)
- `@prism/core/search` — Search Engine (createSearchIndex — TF-IDF inverted index with field-weighted scoring and tokenizer, createSearchEngine — cross-collection search orchestrator with structured filters/facets/pagination/live subscriptions/auto-reindex via CollectionStore change events)
- `@prism/core/notification` — Notification System (createNotificationStore — 8 notification kinds, add/markRead/dismiss/pin/getAll/getUnreadCount with eviction policy and subscriptions, createNotificationQueue — debounced batching with dedup by objectId+kind within configurable window)
- `@prism/core/view` — Derived Views (createViewRegistry — 7 view modes with capability queries, applyFilters/applySorts/applyGroups/applyViewConfig — pure transform pipeline with 12 filter operators, createLiveView — auto-updating materialized projection from CollectionStore with facets/groups/dispose)
- `@prism/core/discovery` — Vault Discovery (createVaultRoster — persistent registry of known vaults with CRUD/sort/pin/search/dedup/persistence via RosterStore, createVaultDiscovery — filesystem scanning for .prism.json manifests with DiscoveryAdapter/roster merge/discovery events, createMemoryRosterStore/createMemoryDiscoveryAdapter for testing)
