# @prism/studio

The Universal Host — every Prism app is a Studio instance. Vite SPA + Tauri 2.0 desktop shell + Capacitor 7 mobile wrapper.

## Quick Start

```bash
pnpm dev                          # Vite dev server on http://localhost:1420
pnpm tauri dev                    # Tauri desktop with hot reload
pnpm build                        # Production build
pnpm typecheck                    # TypeScript strict
pnpm test:e2e                     # Playwright E2E (requires dev server on :1420)
pnpm exec cap add ios|android     # One-time native scaffold (not checked in)
pnpm exec cap sync ios|android    # Inject built dist/ into native shells
```

## Architecture

Studio is a **local-first Vite SPA** (not Next.js). The **same Vite output** is wrapped by Tauri for desktop and by Capacitor for iOS/Android — no forks, one codebase. All daemon communication goes through `src/ipc-bridge.ts` using Tauri `invoke()` — never raw HTTP.

### Kernel (`src/kernel/`)

The kernel wires all Layer 1 systems together at app startup:

- **`studio-kernel.ts`** — singleton that creates and connects ObjectRegistry, CollectionStore, PrismBus, AtomStore, UndoRedoManager, NotificationStore, SearchEngine, ActivityStore, RelayManager, AutomationEngine, PluginRegistry, InputRouter, Identity, VfsManager, Trust & Safety, Facet System, and more.
- **`relay-manager.ts`** — manages WebSocket connections to Prism Relay servers. Studio connects as a client only (no server code).
- **`entities.ts`** — page-builder entity types: folder, page, section, heading, text-block, image, button, card.
- **`kernel-context.tsx`** — React context providing `useKernel`, `useSelection`, `useObjects`, `useUndo`, `useRelay`, `useIdentity`, `useVfs`, etc.

### Components (`src/components/`)

- **`studio-shell.tsx`** — custom shell layout with sidebar/inspector
- **`object-explorer.tsx`** — tree view with drag-drop reorder/reparent, search, templates, activity feed
- **`component-palette.tsx`** — available block types from registry, click-to-add, draggable
- **`inspector-panel.tsx`** — schema-driven property editor from EntityDef fields
- **`notification-toast.tsx`** — floating toasts from NotificationStore
- **`presence-indicator.tsx`** — colored avatar dots for connected peers

### Panels (`src/panels/`)

22 lens panels covering the full IDE surface:

| Panel | Key | Purpose |
|-------|-----|---------|
| Canvas | `v` | WYSIWYG page preview with block toolbar + quick-create |
| Editor | `e` | CodeMirror 6 editing selected object content |
| Graph | `g` | Live-reactive @xyflow/react node graph |
| Layout | `l` | Puck visual builder wired to kernel |
| CRDT | `c` | State inspector (objects/edges/JSON) |
| Relay | `r` | Relay connection manager + portal publishing |
| Settings | `,` | Category-grouped settings from ConfigRegistry |
| Automation | `a` | Create/edit/toggle automation rules |
| Analysis | `n` | Critical path, cycle detection, impact analysis |
| Plugins | `p` | Plugin registry browser |
| Shortcuts | `k` | Keyboard binding manager |
| Vaults | `w` | Vault roster browser |
| Identity | `i` | DID generation, sign/verify, import/export |
| Assets | `f` | VFS blob browser with import/lock/unlock |
| Trust | `t` | Peers, validation, content flags, escrow |
| Form | `d` | Schema-driven form renderer |
| Table | `b` | Data grid with sort/filter/inline edit |
| Sequencer | `q` | Visual automation/script builder |
| Report | `o` | Grouped/summarized data with aggregates |
| Luau Facet | `u` | Luau `ui.*` call parser → React renderer |
| Facet Designer | `x` | Visual FacetDefinition layout builder |
| Record Browser | `z` | Unified data browser (form/list/table/card modes) |

### Data Flow

```
User action → kernel.createObject/updateObject/deleteObject
  → CollectionStore.putObject (Loro CRDT)
  → PrismBus.emit (event)
  → ObjectAtomStore (Zustand cache)
  → React re-render
  → UndoRedoManager.push (snapshot)
  → SearchEngine auto-reindex
```

## Self-Replicating Builds (App Builder)

Studio is not only the universal host — it is also the **factory that produces focused apps** (Flux, Lattice, Cadence, Grip, Relay) from the same codebase. The pipeline lives entirely in the kernel and is exposed via the **App Builder lens** (`Shift+B`).

```
AppProfile + BuildTarget
  → BuilderManager.planBuild() — composes a deterministic BuildPlan
  → BuilderManager.runPlan()   — walks plan.steps one at a time
  → executor.executeStep()     — tauri OR dry-run
  → invoke("run_build_step", { step, workingDir, env })
  → prism_daemon::commands::build::run_build_step
  → structured { stdout?, stderr? } back to the kernel
```

- **`src/kernel/builder-manager.ts`** — profile registry (6 built-ins: `studio`, `flux`, `lattice`, `cadence`, `grip`, `relay`), active-profile pinning, plan composition via `@prism/core/builder`, step-by-step execution through an injectable `BuildExecutor`, run history.
- **`BuildExecutor` modes** — `tauri` (real daemon IPC) and `dry-run` (browser/tests; `emit-file` steps succeed with contents buffered into `stdout`, `run-command`/`invoke-ipc` are skipped).
- **Build targets** — `web` (Vite SPA), `tauri` (desktop binary via `pnpm tauri build`), `capacitor-ios` / `capacitor-android` (mobile via `cap sync` + `cap build`), `relay-node` / `relay-docker` (relay deployments).
- **Capacitor scaffolding** — `capacitor.config.ts` is checked in; the generated `ios/` and `android/` directories are not. Run `pnpm cap add ios|android` once per checkout to create them.
- **Tests** — `builder-manager.test.ts` (30 tests, dry-run + mocked Tauri executor) and `builder-manager-e2e.test.ts` (5 tests, Node-backed invoke fn that faithfully mirrors the daemon's `run_build_step` contract: real `fs.writeFile`, real `spawnSync` with env propagation, real failure surfacing). The full TS↔Rust loop is covered without spawning actual `vite`/`tauri`/`cap` processes in unit tests.

See SPEC.md → *Studio as a Self-Replicating Meta-Builder* for the architectural rationale and `packages/prism-daemon/README.md` for the Rust-side `run_build_step` contract.

## Key Design Decisions

- Kernel is a **singleton** created at app startup, not per-component.
- All mutations go through **kernel CRUD methods** — never raw CollectionStore.
- Studio has **no server code**. Relay servers are managed via CLI; Studio connects as a client.
- Canvas resolves the selected page (walking up parentId) and renders live.
- Editor creates per-object LoroText buffers, debounce-syncs back to kernel.
- The build pipeline dispatches **one step at a time** to the daemon (not a whole plan) so Studio can surface per-step progress and halt on the first failure without leaking a half-executed plan back to the kernel.
