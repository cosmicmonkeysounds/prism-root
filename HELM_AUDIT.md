# Helm Legacy Audit for Prism

> **Helm = Prism** | **Yggdrasil = Lattice** | **Flux = Flux** (unchanged)
>
> This document catalogs everything in the `$legacy-inspiration-only/helm/` codebase
> that can inform, inspire, or be directly ported into the Prism rewrite.

---

## Codebase Overview

Helm is a **Lua-extensible, local-first app platform** with a 6-layer architecture:

```
Layer 1: @core/*           73 pure TypeScript packages (zero React)
Layer 2: @helm/*           28 domain modules (pure TS)
Layer 3: @helm/components  22 React subsystems
Layer 4: @helm/framework   Public SDK facade
Layer 5: create-helm-app   Interactive scaffolder
Layer 6: app/              Flux (productivity IDE) + Yggdrasil (game design)
```

Plus: a Python AI service (`ai/`), Rust audio engine (`canto-core/`), and Tauri platform plugin.

---

## 1. HIGH-VALUE: Port or Adapt Directly

### 1.1 Object Model (`@core/object-model`)

The universal data shape. Everything in Helm is a `GraphObject`.

**Key types:**
- `GraphObject` — `{id, type, parentId, data: JSONB, metadata}`
- `ObjectEdge` — `{sourceId, targetId, relation, behavior: 'weak'|'strong'|'dependency'|'membership'|'assignment'}`
- `EntityDef` — type registry entry (schema, tabs, fields, color, icon, category)
- `ObjectRegistry` — singleton where packages register types, edges, slots, containment rules

**Key functions:**
- `buildObjectTree()` — flat list to hierarchical tree
- `canBeChildOf(sourceType, targetType)` — introspection
- `WeakRefEngine` — auto-materializes content-derived edges (e.g. `[[elena]]` in a Loom conversation creates a virtual edge to the character entity)

**Why it matters for Prism:** This maps directly to Loro CRDT nodes in the Object-Graph. The registry pattern, containment rules, and weak-ref engine are all reusable concepts. Prism's type system should be at least as expressive.

---

### 1.2 Workspace Shell & Plugin System (`@core/workspace`)

Zero-props shell that derives everything from registries.

**Key types:**
- `HelmPlugin` — universal extension unit: `{id, name, contributes: {views, commands, contextMenus, keybindings, fileHandlers, settings, toolbar, statusBar, activityBar}}`
- `ContributionRegistry<T>` — generic typed registry (register, query, byPlugin)
- `TabModel` — tab list with pinning, dedup, eviction, reorder
- `KeyboardRouter` — global command dispatch with priority sorting
- `PanelLayoutModel` — sidebar/inspector/bottom panel visibility + sizing
- `ViewZone` — `'left'|'right'|'top'|'bottom'|'content'|'floating'|'toolbar'|'activity-bar'`

**Three authoring paths for views:**
1. TypeScript components — `useRegisterView(def, Component)`
2. Lua scripts — `Shell.registerView({ render = function(ui, ctx) ... end })`
3. Visual builder — UI that emits Lua

**Why it matters for Prism:** The plugin + registry pattern is exactly what Prism needs for its Lens system. Views are plugins, not hardcoded layouts. The `ContributionRegistry` is generic enough to handle any contribution type.

---

### 1.3 Canto Audio Engine (Rust)

Lock-free, real-time safe signal graph executor. **This is production-grade audio infrastructure.**

**Workspace:** 6 Rust crates (`canto-core` + 5 DSP crates)

**Architecture:**
```
Game Thread                    Audio Thread
CantoEngine.play(id)           GraphExecutor.process()
  -> CommandRing.push(Play)      -> pull nodes in topo order
  -> SoundHandle returned        -> mix to bus graph
                                 -> spatial panning
                                 -> master output
              (lock-free ring buffer)
```

**Real-time guarantees:**
- No heap allocation on audio thread (verified via `assert_no_alloc`)
- No locks (crossbeam ring buffers)
- No blocking I/O
- Topological sort cached, recomputed only on graph changes
- Pre-allocated audio block pools

**9 built-in node types:** Oscillator, Sampler, Envelope, Filter, Gain, Mix, Output, Delay, Noise

**5 DSP crates:** Common, Filters, Reverb, Spatial, Compressor

**Dependencies:** `dasp` (DSP fundamentals), `cpal` (native audio), `symphonia` (decoding), `rubato` (resampling), `fundsp` (DSP node framework)

**Why it matters for Prism:** This is `prism-daemon`'s audio subsystem. The Tauri IPC bridge (`audio.rs`) already demonstrates the pattern: `State<Mutex<AudioState>>` wrapping the engine, exposed via `#[tauri::command]`. Port the Rust crates directly; adapt the TypeScript bindings to Prism's Tauri IPC layer.

---

### 1.4 Lua Runtime (`@core/lua`)

Unified Lua VM orchestration across three contexts.

**Key types:**
- `LuaRuntime` — loads/unloads modules, manages lifecycle hooks, hot-reload
- `LuaModule` — loadable script with `on_load()`, `on_reload()`, `on_unload()` hooks
- `LuaGlobalRegistry` — tracks registered globals for EmmyDoc stub generation
- `EmmyDocGenerator` — emits `.d.lua` stubs for LuaLS IDE autocomplete
- `WasmoonLuaBackend` — Lua 5.4 WASM (production)
- `MockLuaBackend` — deterministic testing

**Three contexts:**

| Context | Scope | Globals |
|---------|-------|---------|
| `workspace` | IDE shell extensions | `Shell.*`, `ObjectModel.*`, `ui.*` |
| `runtime` | Game/simulator session | Domain-specific (`Loom.*`, `Audio.*`) |
| `build` | Build-time codegen | `Expr.eval()` (sandboxed) |

**Module loading patterns:**
- Immediate load (fires `on_load`)
- Lazy registration (`on-demand`, `on-command`, `on-event` triggers)
- Hot-reload (fires `on_reload` hooks)

**Why it matters for Prism:** Prism's Actor system can use the same Lua runtime for user-authored automations. The EmmyDoc generator gives IDE support for free. The three-context model maps to Prism's Lens (workspace), Actor (runtime), and build pipeline contexts.

---

### 1.5 Expression Engine (`@core/syntax`)

Generic expression evaluator with pluggable Lua backend and codegen DSL.

**Key capabilities:**
- TypeScript AST evaluation
- Lua evaluation contract (pluggable VMs)
- Variable/operand resolution
- Conditional logic, math, function calls
- `Scanner`/`TokenStream` for custom parsers

**SymbolDef codegen pipeline** (describe once, emit many targets):
```
new CodegenPipeline()
  .register(new SymbolTypeScriptEmitter())
  .register(new SymbolCSharpEmitter())
  .register(new SymbolGDScriptEmitter())
  .register(new SymbolEmmyDocEmitter())
  .run(symbols, config)
```

**Why it matters for Prism:** Any Lens that needs formulas, conditions, or computed fields can use this. Lattice needs it for stat evaluation (`[stat:trust] >= 60`). The SymbolDef pipeline is essential for Lattice's multi-engine export.

---

### 1.6 Input System (`@core/input`)

Keyboard event routing via a named scope stack.

**Key types:**
- `InputScope` — named context with `KeyboardModel` + action handlers
- `InputRouter` — LIFO scope stack; routes events top-to-bottom
- `UndoHook` — `{undo(), redo()}` without importing `@core/store`

**Pattern:** Active scope (top of stack) gets first chance at events. Each scope has its own action handlers. Push/pop scopes as focus moves between panels.

**Why it matters for Prism:** This decouples keyboard handling from React components. Prism Studio's multi-panel layout needs exactly this pattern — each Lens panel pushes its own scope.

---

### 1.7 Layout System (`@core/layout`)

Multi-pane navigation layer.

**Key types:**
- `PageModel<T>` — live context for a navigated location (viewMode, activeTab, selection, inputScopeId)
- `PageRegistry<T>` — maps target kinds to default view modes
- `WorkspaceSlot` — single navigable pane with back/forward history
- `WorkspaceManager` — manages multiple slots

**Pattern:** Each `PageModel` is independent — scoped selection, viewMode, activeTab. Slot caches PageModels so navigating back restores exact state. Serializable for workspace persistence.

**Why it matters for Prism:** Prism Studio's layout compositor can use the same slot/page model. Each Lens instance gets a `PageModel` with scoped state.

---

## 2. MEDIUM-VALUE: Study and Adapt Patterns

### 2.1 Reactive Atoms (`@core/atom`)

Nanostores-based reactive state bridge.

**Key atoms:** `$selectedId`, `$selectionIds`, `$editingObjectId`, `$activePanel`, `$searchQuery`, `$timer`, `$objects`, `$edges`

**Key factories:**
- `atomQuery(predicate, sort?)` — filtered/sorted derived atoms
- `atomChildren(parentId)` — direct children of a parent
- `atomFamily()` — per-entity atom map
- `connectBusToAtoms()` — wire event bus to atoms
- `connectStoreToAtoms()` — seed cache from ObjectStore
- `atomFromMachine()` — convert FSM into atom
- `connectMachineBidirectional()` — safe two-way wiring with cycle prevention

**Pattern note:** Prism uses Zustand slices + Preact Signals instead of nanostores. The *pattern* (derived atoms, bus-to-atom bridges, machine-to-atom) transfers directly; the library doesn't.

---

### 2.2 Forms & Validation (`@core/forms`, `@core/text-model`)

Schema-driven forms with pure functional state management.

**@core/text-model** — AST definitions:
- `FieldSchema` — 16 field types (text, textarea, rich-text, number, currency, duration, rating, slider, boolean, date, datetime, url, email, phone, color, select, multi-select, tags)
- `DocumentSchema` — fields + sections
- `parseMarkdown()` — block parser (h1-h3, p, blockquote, li, task, code, hr)
- `parseWikiLinks()` — extract `[[id|display]]` syntax
- `extractLinkedIds()` — all linked object IDs from text

**@core/forms** — pure functional state:
- `FormState` — `{values, errors, touched, dirty, isSubmitting, isValid}`
- `createFormState()`, `setFieldValue()`, `validateField()`, `validateForm()`
- `ConditionalRule` — show/hide fields based on other field values
- All mutations return new objects (consumers manage atom/ref)

**Why it matters for Prism:** Any Lens that renders structured data (Flux's object detail view, Lattice's entity inspector) needs schema-driven forms. The wiki-link parser is essential for the Object-Graph's cross-reference system.

---

### 2.3 Task & Planning Engines (`@core/tasks`, `@core/planning`)

**@core/tasks:**
- `ResolvedTask` — typed view over `GraphObject`
- `TaskStateMachine` — 5 states: todo, active, paused, done, cancelled
- `buildDependencyGraph()`, `topologicalSort()`, `detectCycles()`, `findBlockingChain()`, `computeSlipImpact()`

**@core/planning (generalized CPM):**
- Works on **any** `GraphObject` using `data.dependsOn`/`data.blockedBy`
- `buildDepGraph()` -> `computePlan()` (early/late start/finish, total float, critical path)
- Returns `{nodes, critical, totalDuration}`

**Why it matters for Prism:** Flux needs task dependencies. Lattice needs quest/dialogue dependency graphs. The planning engine is domain-agnostic by design.

---

### 2.4 Loom Lang (Yggdrasil)

Domain-specific language for narrative scripting. Not a copy of Ink — own syntax:

- Sigils: `?` conditions, `~` actions, `*` choices, `->` diverts, `<>` inline
- S-expressions for complex conditions
- Screenplay-style dialogue formatting
- Round-trip serialization: `serializeLoomLang()` for visual editor integration
- Lua evaluation at runtime: `[var:session.met_elena] and [stat:trust] >= 60` is real code

**Three-part package structure:**
1. `src/editor/` — UI views registered into workspace shell
2. `src/build/` — compilers, preprocessors, codegen (headless builds possible)
3. `src/runtime/` — gameplay logic that consumes compiled artifacts

**Why it matters for Prism:** Lattice's narrative tools should inherit Loom Lang's design. The three-part structure (authoring/build/runtime) is the right pattern for any Lattice package.

---

### 2.5 Server Factory (`@core/server`)

One-call Hono server from an `ObjectRegistry`.

**Auto-generated routes from registry:**
```
GET    /api/objects/:type          (query, filter, paginate)
GET    /api/objects/:type/:id
POST   /api/objects/:type          (create)
PATCH  /api/objects/:type/:id      (update)
DELETE /api/objects/:type/:id
GET    /api/objects/:type/:id/edges/:rel
```

**Middleware stack:** request logging, CORS, rate limiting, auth (optional), OpenAPI spec generation.

**Why it matters for Prism:** Prism Relay needs REST endpoints. Auto-generating routes from the Object-Graph schema (via Loro types) follows the same pattern. The Hono adapter + middleware stack is directly reusable.

---

### 2.6 Auth & Federation (`@core/auth`, `@core/federation`)

**Auth (Better Auth):**
- Sessions, email+password, magic links, OAuth (GitHub, Google)
- Organizations, members, roles, invitations
- Open `CredentialRegistry` with pluggable providers: WebAuthn, LocalProvider (device ECDSA), DIDProvider (did:key/did:web/did:plc), OAuthProvider
- Tiered config: App tier -> Workspace tier -> Project tier -> Portal guests -> Service credentials

**Federation:**
- Helm Identity: portable DID (did:plc preferred)
- Helm Node: instance identity (did:web)
- Remote Ref Cache: snapshots of remote objects when offline
- Checkpoint Sync: CouchDB-style sequence-based replication
- UCAN delegation: self-verifiable capability tokens
- WebFinger discovery: DNS-based Node discovery
- Object addressing: `helm://did:plc:xxx/objects/uuid`
- Type NSIDs: `io.yggdrasil.simulacra.character` (reverse-DNS)

**Why it matters for Prism:** Prism's W3C DID + E2EE architecture is an evolution of this. The credential registry, UCAN delegation, and WebFinger patterns transfer directly. Swap "Helm" for "Prism" in the addressing scheme.

---

### 2.7 Automation Engine (`@core/automation`)

Trigger/condition/action engine with 30+ built-in action types.

**Four tiers:**
- T1: Simple (field updates, notifications)
- T2a: Data processing (transforms, aggregations)
- T2b: AI-powered (LLM calls, embeddings)
- T4: Complex orchestration (multi-step workflows)

**Plugin API for custom actions.**

**Why it matters for Prism:** This is the Actor system's automation layer. The trigger/condition/action model + tier system provides a good framework for Actor capability levels.

---

### 2.8 AI Service (`ai/`)

Standalone Python FastAPI wrapping local Ollama.

**Key subsystems:**
- RAG pipeline: chunker, embedder, vector store (in-memory with cosine similarity), keyword fallback
- Agent sandbox: ReAct loop with tool registry + audit trail
- Prompt security: injection detection, content sanitization
- Rate limiting: per-IP token bucket + anomaly detection
- 19 endpoints, 76 tests

**Built-in agent tools:** `search_knowledge`, `summarize_text`, `extract_data`, `finish`

**Why it matters for Prism:** Prism Daemon executes local Actors (Whisper, Python workers, local LLMs) as sidecars. The RAG pipeline and agent sandbox patterns are directly applicable. Rewrite in Rust for the daemon, or keep Python as a sidecar.

---

## 3. LOWER-VALUE: Patterns to Note, Not Port

### 3.1 React Components (`@helm/components`)

22 subsystems covering every UI pattern. **Comprehensive but React-specific — study for API design, not code reuse.**

Notable subsystems:
- **Shell:** AppShell, ActivityBar, BoundaryPanel, ContentArea, Breadcrumbs, TabBar
- **Editor:** DocumentSurface (6 modes: code/preview/richtext/form/spreadsheet/pdf)
- **Explorer:** ObjectTree with drag-drop, keyboard nav, fuzzy search
- **Dashboard:** DashboardController, WidgetRegistry, 13 default widgets
- **Graph:** ObjectGraphView (ReactFlow + ELK), node/edge types, filter panel
- **Views:** 20+ (kanban, calendar, list, table, gantt, reports, workflow-board, timeline, planner, database-dashboard, media-library, inbox, analytics, template-gallery, spreadsheet, file-manager, scorecard, habit-tracker, wellness-dashboard, time-blocking, invoice-builder, journal-editor, wiki-editor, contract-editor, scan-viewer)
- **Wizard:** ConditionWizard, ScriptWizard (visual condition/action builders that emit Lua)
- **Email:** 3-pane email client (`@helm/components/email`)
- **Chat:** Discord-style overlay (`@helm/components/chat`)

**What to learn:** The DocumentSurface's 6-mode pattern (one component, multiple rendering modes) is exactly what Prism Lenses need. The ConditionWizard/ScriptWizard pattern (visual UI -> Lua) enables non-programmers to create Actors.

---

### 3.2 Domain Modules (`@helm/*`)

28 modules across 4 groups. Each is a `HelmPlugin` that registers entity types, Lua bindings, views, and commands.

**Work:** crm, projects, freelancer, finance, loans-grants, time-tracking, focus-planner
**Life:** habits, fitness, sleep, goals, meals, journal, cycle-tracking, sexual-health
**Assets:** content, media, document-scanning, collections, inventory
**Platform:** ai, messaging, automation-module, calendar-module, reminders, integration, news

**Cross-module integration pattern:** Modules never import each other. Instead:
- Module A fires an event -> automation rule -> Module B reacts
- Bridge files (`inbox-bridge.ts`, `focus-bridge.ts`) provide pure functions
- App layer wires everything together

**What to learn:** The bridge pattern and event-driven decoupling. Don't port the modules themselves — Prism's Lenses will define their own domain types. But the *pattern* of self-registering, loosely-coupled modules is foundational.

---

### 3.3 Messaging Providers (`@core/messaging/*`)

10 provider packages: Gmail, Outlook, Discord, Slack, Telegram, SMS, Messenger, Instagram, iMessage, plus a unified Inbox model.

**What to learn:** The unified `InboxProvider` interface pattern. Each provider implements the same contract, enabling the UI to render any message source identically.

---

### 3.4 Storage & Sync (`@core/db`, `@core/store`, `@core/sync`)

**Three-tier sovereignty model:**
1. Build-time (`HELM_CAPABILITY=local-only`) — strips sync/auth/server code from binary
2. Deploy-time (`HELM_MAX_SYNC=off|manual|auto`) — operator ceiling
3. Runtime — per-workspace user preference (within build/deploy constraints)

**@core/db:** Drizzle ORM schema for PostgreSQL + SQLite (users, sessions, orgs, federation grants, portal sessions, service credentials, published views).

**@core/store:** `ObjectStore` with `getAll()`, `subscribe()`, `save()`, `createEdge()`, `undo/redo`.

**@core/sync:** Three modes: off (local SQLite), manual (user-triggered push/pull), auto (Electric SQL real-time).

**What to learn:** Prism replaces this with Loro CRDT + VFS, but the sovereignty model (build/deploy/runtime capability tiers) is a great pattern for Prism's sync configuration.

---

### 3.5 State Machines (`@core/automaton`)

- Flat FSMs: guards, actions, entry/exit hooks
- Hierarchical statecharts: nested states, parallel regions
- Used for: `TaskStateMachine`, `DocumentStatusMachine`, `Stopwatch`, protocol states

**What to learn:** XState is Prism's chosen FSM library, but the *patterns* (task lifecycle, document status, protocol handshake states) transfer.

---

### 3.6 Yggdrasil Packages (Incomplete)

Only Loom, Meridian, Simulacra, Cue, Canto, and Wyrd are implemented. Topology, Kami, Palette, and Boon are "planned" (specs exist, minimal code).

**What to learn:** The package specs for Topology (scene graph), Kami (AI behavior trees), Palette (inventory/loot), and Boon (abilities/skills) are useful design references for Lattice even though the code is incomplete.

---

## 4. Yggdrasil (Lattice) Specific Salvage

These components are uniquely relevant to Lattice:

| Component | What It Does | Salvage Value |
|-----------|-------------|---------------|
| **Loom** | Narrative scripting DSL | Port the language spec; rewrite parser for Prism |
| **Meridian** | Stats system (axes, pools, attributes, condition eval) | Port the data model; adapt to Loro |
| **Simulacra** | Entity/object system for game worlds | Merge with Prism's Object-Graph |
| **Cue** | Typed event bus for game events | Adapt for Prism's Actor event system |
| **Canto** | Audio engine (Rust, lock-free) | Port directly into `prism-daemon` |
| **Wyrd** | Subsystems (topo sort, scope registry) | Port utility functions |
| **Chronicle** | Offline-first game analytics/event store | Port for Lattice's playtesting tools |
| **SymbolDef pipeline** | Describe once, emit TS/C#/GDScript/Lua | Port for Lattice's multi-engine export |
| **Bank export** | Content-addressed artifact manifests (SHA-256) | Port for incremental hot-reload to engines |
| **Workspace Hub** | Click-to-enable package management with dep resolution | Port UX pattern for Lattice's module selector |

---

## 5. Developer Tooling Worth Preserving

### .claude/commands/

| Command | Purpose | Port? |
|---------|---------|-------|
| `layer-check.md` | Validate code is in correct architectural layer | Yes — adapt for Prism's 5-pillar taxonomy |
| `port.md` | Step-by-step guide to port legacy code | Reference only |
| `new-pkg.md` | Scaffold new `@core` package | Yes — adapt for `@prism/*` packages |
| `new-module.md` | Scaffold new `@helm` module | Yes — adapt for Prism Lens packages |
| `test.md` | Run unified test suite | Superseded by Prism's turbo pipeline |

### Codegen Utilities

| Utility | Purpose | Port? |
|---------|---------|-------|
| `luaBrowserView()` | Emit Lua for filterable list views | Yes |
| `luaCollectionRule()` | Emit Lua for collection validation | Yes |
| `luaStatsCommand()` | Emit Lua for stat counting | Yes |
| `luaMenuItem()` | Emit Lua for menu items | Yes |
| `luaCommand()` | Emit Lua for commands | Yes |
| `luaWeakRefProvider()` | Emit Lua for cross-object ref extraction | Yes |
| `emitConditionLua()` | Pure function: visual condition -> Lua | Yes |
| `emitScriptLua()` | Pure function: visual script -> Lua | Yes |

---

## 6. Architectural Patterns to Carry Forward

### Registry-Driven Everything
Don't hardcode type lists, view lists, or command lists. Let packages self-register into open registries. The `ObjectRegistry`, `ViewRegistry`, `ContributionRegistry`, `CredentialRegistry`, and `LuaGlobalRegistry` patterns are all worth adopting.

### Generalize Domain Logic
`@core/planning` works on **any** `GraphObject`, not just tasks. `DocumentSurface` renders **any** structured data in 6 modes. Make Prism's core systems domain-agnostic; let Lenses specialize.

### Bridge Pattern for Cross-Module Integration
Modules never import each other. Bridge files translate domain logic into shapes others can consume. Events decouple producers from consumers. The app layer is the only place that wires modules together.

### Three Authoring Paths
TypeScript for developers, Lua for power users, visual builders for everyone else. All three produce the same output and appear identically in the shell.

### Separate Shape from Content
Workspace folder (YAML + assets, Git-committable) holds structure. Database (SQLite/Loro) holds live content. Structure and content version independently.

### Hot-Reload as a First-Class Feature
Lua modules can be edited and reloaded without restart. This enables live workspace customization — essential for the "every app is an IDE" philosophy.

---

## 7. What NOT to Port

- **NullObjectStore** — placeholder no-op store in Flux. Build real local persistence from day one.
- **Electric SQL sync** — Prism uses Loro CRDT sync instead.
- **TipTap rich text editor** — Prism uses CodeMirror 6 exclusively.
- **npm workspaces** — Prism uses pnpm + Turborepo.
- **Nanostores** — Prism uses Zustand + Preact Signals.
- **Better Auth directly** — evaluate against Prism's DID-first auth model.
- **Hard-coded dashboard layouts** — Flux's `HomeDashboardPanel` bypasses its own `DashboardController`. Don't repeat this.
- **Domain module implementations** — The 28 `@helm/*` modules are Helm-specific. Port the patterns, not the code.
