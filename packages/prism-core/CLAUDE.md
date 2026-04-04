# @prism/core

Client-side execution environment — Layer 1 (agnostic TS) + Layer 2 (React renderers).

## Build
- `pnpm typecheck` / `pnpm test` / `pnpm test:watch`

## Architecture
- **Layer 1**: Object Model (GraphObject, ObjectRegistry, TreeModel, EdgeModel, WeakRefEngine, ContextEngine, NSID), Lens System (LensRegistry, ShellStore), Plugin System (PrismPlugin, PluginRegistry, ContributionRegistry), Reactive Atoms (PrismBus, AtomStore, ObjectAtomStore, bus-to-atom bridges), State Machines (Machine, createMachine — flat FSM with guards/actions/lifecycle), Graph Analysis (dependency graph, topological sort, cycle detection, blocking chains, slip impact), Planning Engine (CPM — critical path, early/late start/finish, float), Input System (KeyboardModel, InputScope, InputRouter), Layout System (SelectionModel, PageModel, PageRegistry, WorkspaceSlot, WorkspaceManager), Forms & Validation (FieldSchema, DocumentSchema, FormState, wiki-links, markdown), Expression Engine (Scanner, Parser, Evaluator), Zustand stores, Loro bridge, Lua runtime (wasmoon), XState machines
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
