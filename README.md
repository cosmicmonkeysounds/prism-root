# Prism

**Distributed Visual Operating System** built on a **Federated Object-Graph** using CRDTs.

Local-first, file-over-app. Applications are **Lenses** — disposable views over sovereign data. If the entire Prism ecosystem disappeared tomorrow, users retain 100% functional local files.

Every Prism app is an IDE. There is no wall between "using" and "building." The difference between a consumer app and a developer tool is a single toggle — the **"Glass Flip."**

> Full specification: [`SPEC.md`](SPEC.md) | Decision log: [`docs/adr/`](docs/adr/) | Current task: [`docs/dev/current-plan.md`](docs/dev/current-plan.md)

## Data Topology

Prism replaces files and folders with a semantic mesh of nodes connected by weak references.

| Concept | What it is |
|---------|-----------|
| **Vault** | Encrypted local directory. The physical security boundary — files are unreadable blobs unless the Prism Daemon is unlocked. Contains Collections and Manifests. |
| **Collection** | Typed CRDT array (e.g. `Contacts`, `Tasks`, `Audio_Busses`). This is where data lives. |
| **Manifest** | A `.prism.json` file containing weak references to Collections. A "workspace" is just a manifest — it points to data, it doesn't hold it. Multiple manifests can reference the same collection with different filters. |
| **Shell** | The IDE chrome that renders whatever a manifest references. No fixed layout; content is derived from registries. |
| **Lens** | An application view (editor, graph, layout builder). Lenses project Loro CRDT state into visual form. |

## Quick Start

### Prerequisites

- **Node.js** >= 20, **pnpm** >= 9
- **Rust** >= 1.75 (for prism-daemon)
- **Tauri 2.0 CLI** (`cargo install tauri-cli --version "^2"`)

### Setup

```bash
pnpm install          # Install all dependencies
pnpm typecheck        # TypeScript strict check
pnpm test             # Vitest (all packages)
pnpm test:e2e         # Playwright end-to-end

# Rust daemon
cd packages/prism-daemon && cargo test && cargo clippy

# Dev server (Vite SPA)
cd packages/prism-studio && pnpm dev

# Tauri desktop app (includes daemon)
cd packages/prism-studio && pnpm tauri dev
```

## Architecture

```
prism/
├── packages/
│   ├── shared/           # TypeScript types and IPC contracts
│   ├── prism-core/       # Layer 1 (pure TS) + Layer 2 (React renderers)
│   ├── prism-daemon/     # Rust: Loro CRDT, mlua Lua 5.4, VFS, hardware
│   └── prism-studio/     # Vite SPA + Tauri 2.0 shell (Universal Host)
├── e2e/                  # Playwright end-to-end tests
├── docs/                 # ADRs, dev plans
└── SPEC.md               # Full technical specification
```

### The 5-Pillar Model

| Pillar | Role |
|--------|------|
| **Prism Core** | Client-side glass + logic. Layer 1 (domain-agnostic pure TS) + Layer 2 (React renderers). |
| **Prism Daemon** | Rust background engine on sovereign hardware. CRDT merging, VFS, Actors, hardware protocols. |
| **Prism Relay** | Open-source zero-knowledge routing infrastructure. E2EE store-and-forward, Sovereign Portals via Next.js SSR. |
| **Prism Studio** | The Universal Host app. Vite SPA in Tauri (desktop) / Capacitor (mobile). Every app is a Studio instance. |
| **Prism Nexus** | Commercial SaaS wrapper: managed Relays + cloud Studio + App Repo. Fully ejectable to local. |

### The Stack

| Layer | Technology | Purpose |
|-------|-----------|---------|
| CRDT | Loro | Single source of truth. The hidden buffer beneath all editors. |
| State | Zustand | Atomic stores subscribed to specific Loro node IDs. |
| Scripting | Lua 5.4 | Same scripts run in browser (wasmoon) and daemon (mlua). |
| Desktop | Tauri 2.0 | Native shell, Rust backend, IPC bridge. |
| Frontend | Vite + React | SPA for all client apps. Next.js strictly on Relays. |
| Editor | CodeMirror 6 | Sole text/code editor. No Monaco anywhere. |

### Data Flow

```
React UI  →  Zustand Store  →  Loro Bridge  →  LoroDoc (browser)
                                                     │
                                                 Tauri IPC
                                                     │
                                               Rust Daemon  →  LoroDoc (native)
                                                     │
                                               mlua (Lua 5.4)
```

### Layer 1 Systems (Pure TypeScript)

Layer 1 is the domain-agnostic core. Zero React, zero DOM, zero runtime assumptions. Everything here works in any JavaScript environment.

| System | Module | What it does |
|--------|--------|-------------|
| Object Model | `@prism/core/object-model` | GraphObject, ObjectRegistry, TreeModel, EdgeModel, WeakRefEngine, ContextEngine, NSID |
| Input | `@prism/core/input` | KeyboardModel, InputScope, InputRouter (LIFO scope stack) |
| Forms | `@prism/core/forms` | FieldSchema, DocumentSchema, FormState, wiki-link parser, markdown parser |
| Layout | `@prism/core/layout` | SelectionModel, PageModel, PageRegistry, WorkspaceSlot, WorkspaceManager |
| Expression | `@prism/core/expression` | Scanner, recursive descent parser, evaluator with builtins |
| Plugin | `@prism/core/plugin` | PrismPlugin, PluginRegistry, ContributionRegistry (views, commands, keybindings, menus) |
| Reactive Atoms | `@prism/core/atom` | PrismBus event bus, AtomStore, ObjectAtomStore, bus-to-atom bridges |
| State Machines | `@prism/core/automaton` | Flat FSM with guards, actions, lifecycle hooks (onEnter/onExit) |
| Graph Analysis | `@prism/core/graph-analysis` | Dependency graph, topological sort, cycle detection, CPM planning engine |
| Automation | `@prism/core/automation` | Trigger/condition/action rules, condition evaluator, template interpolation, cron scheduling |
| Manifest | `@prism/core/manifest` | PrismManifest (`.prism.json`), CollectionRef, StorageConfig, SyncConfig, validation |
| Workspace Shell | `@prism/core/workspace` | LensManifest, LensRegistry, ShellStore (IDE chrome infrastructure) |

### Layer 2 Systems (React Renderers)

Layer 2 projects Layer 1 state into visual form.

| System | Module | What it does |
|--------|--------|-------------|
| CodeMirror | `@prism/core/codemirror` | LoroText bidirectional sync |
| Puck | `@prism/core/puck` | Loro-backed visual layout builder |
| KBar | `@prism/core/kbar` | Command palette with focus-depth routing |
| Spatial Graph | `@prism/core/graph` | @xyflow/react + elkjs auto-layout |
| Shell | `@prism/core/shell` | ShellLayout, ActivityBar, TabBar, LensProvider |

## Build Phases

| Phase | Name | Systems | Status |
|-------|------|---------|--------|
| 1 | The Heartbeat | Loro CRDT round-trip via Tauri IPC | Complete |
| 2 | The Eyes | CodeMirror, Puck, KBar, multi-panel Studio | Complete |
| 0 | The Object Model | GraphObject, Registry, Tree, Edges, WeakRef, NSID, Query | Complete |
| 4 | The Shell | LensRegistry, WorkspaceStore, ActivityBar, TabBar | Complete |
| 5 | Input, Forms, Layout, Expression | Four Layer 1 systems ported from legacy | Complete |
| 6 | Context Engine, Plugin, Atoms | ContextEngine, PluginRegistry, PrismBus, AtomStore | Complete |
| 7 | Machines, Graph Analysis, Planning | FSM, dependency graph, CPM critical path | Complete |
| 8 | Automation, Manifest | AutomationEngine, PrismManifest | Complete |

**Test status**: 676 Vitest + 19 Playwright E2E — all passing.

## Ecosystem Apps

Four apps share the same Object-Graph and Prism Core. Each is a set of Lenses and Manifests.

| App | Purpose |
|-----|---------|
| **Flux** | Operational hub: productivity, finance, CRM, goals, inventory. The primary entry point. |
| **Lattice** | Game middleware suite: narrative (Loom), audio (Canto), entity authoring (Simulacra), event orchestration (Cue). |
| **Cadence** | Music education platform: interactive lessons, DID-based enrollment, CRDT-synced assignments. |
| **Grip** | Live production management: stage plots, cue sheets, lighting/audio/video control via hardware protocols. |

## Philosophy

- **Local-First**: The user's hard drive is the source of truth.
- **Every App Is an IDE**: Consumer and developer share the same app (Glass Flip).
- **Ownership Over Convenience**: MIT-only deps, no vendor lock-in.
- **Blockchain-Free**: W3C DIDs + E2EE CRDTs, not ledgers or tokens.
- **Loro Is the Hidden Constant**: All state lives in Loro. Editors are projections.

## Development

```bash
pnpm dev          # Start all packages
pnpm build        # Build in dependency order
pnpm test         # Vitest (all packages)
pnpm test:e2e     # Playwright end-to-end
pnpm lint         # ESLint
pnpm typecheck    # TypeScript strict check
pnpm format       # Prettier

# Rust
cd packages/prism-daemon
cargo test        # Unit + integration tests
cargo clippy      # Linting
```

### Conventions

- ES Modules, TypeScript strict, no `any`, kebab-case files
- Conventional Commits: `type(scope): description`
- `@prism/*` path aliases — never relative paths across packages
- Never deprecate. Rename, move, break, fix. `tsc --noEmit` is your safety net.

## License

MIT
