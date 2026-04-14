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
pnpm install                    # Install all dependencies
```

### Dev Servers

```bash
pnpm dev                        # Start all packages (Turborepo)
pnpm --filter @prism/studio dev  # Studio only — Vite SPA on http://localhost:1420
pnpm --filter @prism/relay dev   # Relay only on http://localhost:3000
```

### Testing

```bash
pnpm test                       # Vitest — all packages (2652 tests)
pnpm test:e2e                   # Playwright E2E (263 Studio + 135 Relay tests)
pnpm typecheck                  # TypeScript strict check
pnpm lint                       # ESLint
pnpm format                     # Prettier

# Rust daemon
cd packages/prism-daemon && cargo test && cargo clippy
```

### Desktop App (Tauri)

```bash
cd packages/prism-studio && pnpm tauri dev   # Tauri desktop with hot reload
```

## Architecture

```
prism/
├── packages/
│   ├── shared/           # TypeScript types and IPC contracts
│   ├── prism-core/       # 8 domain categories (foundation → … → bindings)
│   ├── prism-daemon/     # Rust: Loro CRDT, mlua Luau, VFS, hardware
│   ├── prism-relay/      # Modular relay server: Hono HTTP + WebSocket, CLI
│   ├── prism-studio/     # Vite SPA + Tauri 2.0 shell (Universal Host)
│   └── prism-puck-playground/  # Standalone single-file harness for the Puck builder
├── docs/
│   ├── adr/              # Architecture Decision Records
│   └── dev/              # Current plan, studio checklist
├── scripts/hooks/        # Claude Code development hooks
└── SPEC.md               # Full technical specification
```

### The 5-Pillar Model

| Pillar | Role |
|--------|------|
| **Prism Core** | Client-side glass + logic. 8 domain categories with a strict downward dependency DAG; React / DOM / WebGL live only in `bindings/`. |
| **Prism Daemon** | Rust background engine on sovereign hardware. CRDT merging, VFS, Actors, hardware protocols. |
| **Prism Relay** | Open-source zero-knowledge routing infrastructure. E2EE store-and-forward, Sovereign Portals via SSR, AutoREST API gateway, federation mesh. 15 composable modules. |
| **Prism Studio** | The Universal Host app **and** the factory that builds every other app. Vite SPA in Tauri (desktop) / Capacitor (mobile). The App Builder lens composes focused apps (Flux/Lattice/Cadence/Grip/Relay) from the same codebase via `BuilderManager` → `run_build_step` daemon IPC. |
| **Prism Nexus** | Commercial SaaS wrapper: managed Relays + cloud Studio + App Repo. Fully ejectable to local. |

### The Stack

| Layer | Technology | Purpose |
|-------|-----------|---------|
| CRDT | Loro | Single source of truth. The hidden buffer beneath all editors. |
| State | Zustand | Atomic stores subscribed to specific Loro node IDs. |
| Scripting | Luau | Same scripts run in browser (luau-web) and daemon (mlua). |
| Desktop | Tauri 2.0 | Native shell, Rust backend, IPC bridge. |
| Mobile | Capacitor 7 | iOS/Android native wrapper around the same Vite SPA. |
| Frontend | Vite + React | SPA for all client apps. SSR strictly on Relays. |
| Editor | CodeMirror 6 | Sole text/code editor. No Monaco anywhere. |

### Data Flow

```
React UI  →  Zustand Store  →  Loro Bridge  →  LoroDoc (browser)
                                                     │
                                                 Tauri IPC
                                                     │
                                               Rust Daemon  →  LoroDoc (native)
                                                     │
                                               mlua (Luau)
```

### Subsystems (by category)

`@prism/core` is split into 8 domain categories forming a strict downward dependency DAG (`foundation → language/identity → kernel/network → interaction/domain → bindings`). Only `bindings/` is allowed to import React / DOM / WebGL; everything above it runs in any JavaScript environment. See `packages/prism-core/README.md` for the full export table.

**`foundation/`** — pure data primitives: `@prism/core/object-model`, `@prism/core/persistence`, `@prism/core/vfs`, `@prism/core/stores`, `@prism/core/batch`, `@prism/core/clipboard`, `@prism/core/template`, `@prism/core/undo`

**`language/`** — languages, parsers, emitters: `@prism/core/expression`, `@prism/core/forms`, `@prism/core/syntax`, `@prism/core/luau`, `@prism/core/facet`

**`kernel/`** — runtime/execution: `@prism/core/actor`, `@prism/core/automation`, `@prism/core/builder`, `@prism/core/config`, `@prism/core/plugin`, `@prism/core/plugin-bundles`, `@prism/core/automaton`

**`interaction/`** — UI-facing state (React-free): `@prism/core/atom`, `@prism/core/layout`, `@prism/core/lens`, `@prism/core/input`, `@prism/core/activity`, `@prism/core/notification`, `@prism/core/search`, `@prism/core/view`

**`identity/`** — DIDs, keys, trust, manifest: `@prism/core/identity`, `@prism/core/encryption`, `@prism/core/trust`, `@prism/core/manifest`

**`network/`** — wire-crossing systems: `@prism/core/relay`, `@prism/core/presence`, `@prism/core/session`, `@prism/core/discovery`, `@prism/core/server`

**`domain/`** — higher-level domain models: `@prism/core/flux`, `@prism/core/graph-analysis`, `@prism/core/timeline`

**`bindings/`** — React / DOM / WebGL adapters: `@prism/core/codemirror`, `@prism/core/puck`, `@prism/core/kbar`, `@prism/core/graph` (xyflow), `@prism/core/shell`, `@prism/core/viewport3d`, `@prism/core/audio`

## Admin Dashboards

Every Prism runtime ships a live admin dashboard powered by `@prism/admin-kit`:

| Runtime | Access | Technology |
|---------|--------|------------|
| **Studio** | Admin lens (Shift+A) | React + Puck widgets via `<AdminProvider>` |
| **Relay** | `GET /admin` on the running relay | Self-contained HTML page, auto-refreshes via `/admin/api/snapshot` |
| **Daemon** | `GET /admin` on the HTTP transport | Self-contained HTML page, auto-refreshes via `/admin/api/snapshot` |

All dashboards share the same `AdminSnapshot` data model (health, uptime, metrics, services, activity). The admin-kit provides:
- **React widgets** — `HealthBadge`, `MetricCard`, `MetricChart`, `ServiceList`, `ActivityTail`, `UptimeCard`, `SourceHeader`
- **HTML renderer** — `renderAdminHtml()` generates a standalone page for server embedding
- **Data sources** — `createKernelDataSource()`, `createRelayDataSource()`, `createDaemonDataSource()`

## Build Phases

| Phase | Name | Systems | Status |
|-------|------|---------|--------|
| 1 | The Heartbeat | Loro CRDT round-trip via Tauri IPC | Complete |
| 2 | The Eyes | CodeMirror, Puck, KBar, multi-panel Studio | Complete |
| 0 | The Object Model | GraphObject, Registry, Tree, Edges, WeakRef, NSID, Query | Complete |
| 4 | The Shell | LensRegistry, ShellStore, ActivityBar, TabBar | Complete |
| 5 | Input, Forms, Layout, Expression | Four core subsystems ported from legacy | Complete |
| 6 | Context Engine, Plugin, Atoms | ContextEngine, PluginRegistry, PrismBus, AtomStore | Complete |
| 7 | Machines, Graph Analysis, Planning | FSM, dependency graph, CPM critical path | Complete |
| 8 | Automation, Manifest | AutomationEngine, PrismManifest | Complete |
| 28-29 | NLE Timeline, Luau | NLE core, Timeline, Luau runtime, Tool FSM | Complete |
| 30a-d | Studio Kernel (Tier 0) | StudioKernel, Entities, Persistence, Undo, Notifications, Search, Clipboard, Templates, Activity | Complete |
| 30e | Studio UI (Tier 1) | Clipboard UI, Template Gallery, Activity Feed, Object Reorder + 18 E2E | Complete |

**Test status**: 2652 Vitest (120 test files) + 263 Studio Playwright E2E + 135 Relay Playwright E2E — all passing.

## Ecosystem Apps

Four apps share the same Object-Graph and Prism Core. Each is a set of Lenses and Manifests.

| App | Purpose |
|-----|---------|
| **Flux** | Operational hub: productivity, finance, CRM, goals, inventory. The primary entry point. |
| **Lattice** | Game middleware suite: narrative (Loom), audio (Canto), entity authoring (Simulacra), event orchestration (Cue). |
| **Cadence** | Music production + education: interactive lessons, DID-based enrollment, CRDT-synced assignments. Backed by OpenDAW audio engine. |
| **Grip** | Live production management: stage plots, cue sheets, lighting/audio/video control via hardware protocols (MIDI, DMX, OSC). |

## Philosophy

- **Local-First**: The user's hard drive is the source of truth.
- **Every App Is an IDE**: Consumer and developer share the same app (Glass Flip).
- **Ownership Over Convenience**: MIT-only deps, no vendor lock-in.
- **Blockchain-Free**: W3C DIDs + E2EE CRDTs, not ledgers or tokens.
- **Loro Is the Hidden Constant**: All state lives in Loro. Editors are projections.
- **Layers Flow Bottom-Up**: Plugins, lenses, and initializers self-register via `PluginBundle` / `LensBundle` / `StudioInitializer`. The host composes — it never reaches into its children. See `SPEC.md → Kernel Composition & Self-Registering Bundles`.

## Development

All commands run from the repo root unless noted. Turborepo handles dependency ordering.

```bash
# Dev
pnpm dev                        # All packages
pnpm --filter @prism/studio dev  # Studio only (:1420)
pnpm --filter @prism/relay dev   # Relay only (:3000)

# Quality
pnpm build        # Production build (dependency order)
pnpm test         # Vitest (all packages, ~2652 tests)
pnpm test:e2e     # Playwright E2E (263 Studio + 135 Relay)
pnpm typecheck    # TypeScript strict
pnpm lint         # ESLint
pnpm format       # Prettier

# Rust daemon
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
