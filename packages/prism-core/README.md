# @prism/core

Client-side execution environment for Prism. Two layers:

- **Layer 1** — Pure TypeScript, zero React, zero DOM. Runs in any JS environment (browser, Node, Deno, Bun, workers).
- **Layer 2** — React renderers that project Layer 1 state into visual form.

## Layer 1 Systems

| System | Export | Purpose |
|--------|--------|---------|
| Object Model | `@prism/core/object-model` | GraphObject, ObjectRegistry, TreeModel, EdgeModel, WeakRefEngine, NSID, ObjectQuery |
| Lens System | `@prism/core/lens` | LensManifest, LensRegistry, ShellStore, tab/panel management |
| Plugin System | `@prism/core/plugin` | PrismPlugin, PluginRegistry, ContributionRegistry |
| Reactive Atoms | `@prism/core/atom` | PrismBus event bus, AtomStore, ObjectAtomStore, bus-to-atom bridges |
| State Machines | `@prism/core/automaton` | Flat FSM with guards, actions, lifecycle hooks |
| Input System | `@prism/core/input` | KeyboardModel, InputScope, InputRouter (LIFO scope stack) |
| Layout System | `@prism/core/layout` | SelectionModel, PageModel, PageRegistry, LensSlot, LensManager |
| Forms | `@prism/core/forms` | FieldSchema, DocumentSchema, FormState, wiki-links, markdown |
| Expression Engine | `@prism/core/expression` | Scanner, recursive descent parser, evaluator with builtins |
| Graph Analysis | `@prism/core/graph-analysis` | Dependency graph, topological sort, cycle detection, CPM planning |
| Automation | `@prism/core/automation` | Trigger/condition/action rules, cron scheduling |
| Manifest | `@prism/core/manifest` | PrismManifest (`.prism.json`), CollectionRef, validation |
| Config | `@prism/core/config` | ConfigRegistry, ConfigModel, FeatureFlags, layered scope resolution |
| Undo/Redo | `@prism/core/undo` | UndoRedoManager with snapshot-based undo, batch, merge |
| Persistence | `@prism/core/persistence` | CollectionStore (Loro-backed), VaultManager, PersistenceAdapter |
| Search | `@prism/core/search` | TF-IDF inverted index, cross-collection search with facets |
| Notifications | `@prism/core/notification` | NotificationStore (8 kinds), NotificationQueue with debounced batching |
| Derived Views | `@prism/core/view` | ViewRegistry (7 modes), ViewConfig pipeline, LiveView |
| Vault Discovery | `@prism/core/discovery` | VaultRoster, filesystem scanning for `.prism.json` |
| Activity Log | `@prism/core/activity` | Append-only audit trail, auto-diff tracking |
| Batch Ops | `@prism/core/batch` | Atomic multi-op with validation, rollback, single undo entry |
| Clipboard | `@prism/core/clipboard` | Cut/copy/paste for subtrees with deep clone, ID remap |
| Templates | `@prism/core/template` | Reusable ObjectTemplate blueprints with variable interpolation |
| Presence | `@prism/core/presence` | RAM-only peer cursors, selection, TTL eviction |
| Identity | `@prism/core/identity` | W3C DID (did:key/did:web), Ed25519 sign/verify, multi-sig |
| Encryption | `@prism/core/encryption` | HKDF-derived AES-GCM-256 vault keys, snapshot encryption |
| VFS | `@prism/core/vfs` | Content-addressed blob storage, SHA-256 dedup, binary forking |
| Relay | `@prism/core/relay` | Composable relay builder, 8 modules (mailbox, router, portals, etc.) |
| Actor System | `@prism/core/actor` | ProcessQueue, Lua/sidecar runtimes, AI providers, capability sandboxing |
| Communication | `@prism/core/session` | Sessions, transcript timeline, playback controller |
| Syntax Engine | `@prism/core/syntax` | LSP-like diagnostics/completions/hover for expressions |
| Trust & Safety | `@prism/core/trust` | Lua sandbox, schema validator, hashcash, peer trust, Shamir splitting |
| Timeline/NLE | `@prism/core/timeline` | Transport, tracks, clips, automation, tempo map (PPQN) |
| Server Factory | `@prism/core/server` | Auto-generate REST routes + OpenAPI 3.1 from ObjectRegistry |
| Flux Domain | `@prism/core/flux` | 11 entity types, 7 edge types, 8 automation presets, CSV/JSON import |

## Layer 2 Systems

| System | Export | Purpose |
|--------|--------|---------|
| CodeMirror | `@prism/core/codemirror` | LoroText bidirectional sync |
| Puck | `@prism/core/puck` | Loro-backed visual layout builder |
| KBar | `@prism/core/kbar` | Command palette with focus-depth routing |
| Spatial Graph | `@prism/core/graph` | @xyflow/react + elkjs auto-layout |
| Shell | `@prism/core/shell` | ShellLayout, ActivityBar, TabBar, LensProvider |
| 3D Viewport | `@prism/core/viewport3d` | Loro-backed scene graph, OpenCASCADE.js CAD import, TSL→GLSL shader compiler |
| OpenDAW Audio | `@prism/core/audio` | Bridges Prism timeline to OpenDAW audio engine |

## Key Principles

- **Loro CRDT is the hidden buffer.** Editors project Loro state — they don't own it.
- **Zustand stores subscribe to specific Loro node IDs** — atomic reactivity.
- **Layer 1 has zero runtime dependencies** on React, DOM, or any specific JS runtime.

## Build

```bash
pnpm typecheck    # TypeScript strict
pnpm test         # Vitest (2600+ tests across all Layer 1/2 systems)
pnpm test:watch   # Watch mode
```
