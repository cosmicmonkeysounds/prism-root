# @prism/core

Client-side execution environment for Prism. Organized into **8 domain categories** under `src/`, forming a strict downward dependency DAG:

```
foundation → language/identity → kernel/network → interaction/domain → bindings
```

Only `bindings/` is allowed to import React / DOM / WebGL; everything above it runs in any JS environment (browser, Node, Deno, Bun, workers). Loro CRDT is the hidden buffer — editors project Loro state.

## Subpath exports

### `foundation/` — pure data primitives
| Export | Purpose |
|--------|---------|
| `@prism/core/object-model` | GraphObject, ObjectRegistry, TreeModel, EdgeModel, WeakRefEngine, NSID, ObjectQuery |
| `@prism/core/persistence` | CollectionStore (Loro-backed), VaultManager, PersistenceAdapter |
| `@prism/core/vfs` | Content-addressed blob storage, SHA-256 dedup, binary forking |
| `@prism/core/stores` | Zustand CRDT store factories + `useCrdtStore` hook |
| `@prism/core/batch` | Atomic multi-op with validation, rollback, single undo entry |
| `@prism/core/clipboard` | Cut/copy/paste for subtrees with deep clone + ID remap |
| `@prism/core/template` | Reusable ObjectTemplate blueprints with variable interpolation |
| `@prism/core/undo` | UndoRedoManager with snapshot-based undo, batch, merge |

### `language/` — languages, parsers, emitters
| Export | Purpose |
|--------|---------|
| `@prism/core/expression` | Scanner, recursive descent parser, evaluator with builtins |
| `@prism/core/forms` | FieldSchema, DocumentSchema, FormState, wiki-links, markdown |
| `@prism/core/syntax` | LSP-like diagnostics/completions/hover, `.d.luau` generation |
| `@prism/core/luau` | Browser Luau runtime (`luau-web`) + Luau debugger |
| `@prism/core/facet` | FacetParser, SpellChecker, ProseCodec, Sequencer, language Emitters, value lists, visual scripts |

### `kernel/` — runtime, execution, orchestration
| Export | Purpose |
|--------|---------|
| `@prism/core/actor` | ProcessQueue, Luau/sidecar runtimes, AI providers, capability sandboxing |
| `@prism/core/automation` | Trigger/condition/action rules with cron scheduling |
| `@prism/core/builder` | Self-replicating app builder (AppProfile → BuildPlan → steps) |
| `@prism/core/config` | ConfigRegistry, ConfigModel, FeatureFlags, layered scope resolution |
| `@prism/core/plugin` | PrismPlugin, PluginRegistry, ContributionRegistry |
| `@prism/core/plugin-bundles` | Canonical built-in plugin bundles used by hosts |
| `@prism/core/automaton` / `machines` / `state-machine` | Flat FSM primitives with guards/actions/lifecycle |

### `interaction/` — UI-facing state (React-agnostic)
| Export | Purpose |
|--------|---------|
| `@prism/core/atom` | PrismBus event bus, AtomStore, ObjectAtomStore, bus→atom bridges |
| `@prism/core/layout` | SelectionModel, PageModel, PageRegistry, LensSlot, LensManager |
| `@prism/core/lens` | LensManifest, LensRegistry, ShellStore, LensBundle + self-register |
| `@prism/core/input` | KeyboardModel, InputScope, InputRouter (LIFO scope stack) |
| `@prism/core/activity` | Append-only audit trail, auto-diff tracking |
| `@prism/core/notification` | NotificationStore (8 kinds), NotificationQueue with debounced batching |
| `@prism/core/search` | TF-IDF inverted index, cross-collection search with facets |
| `@prism/core/view` | ViewRegistry (7 modes), ViewConfig pipeline, LiveView, Saved Views |

### `identity/` — DIDs, keys, trust, manifest
| Export | Purpose |
|--------|---------|
| `@prism/core/identity` | W3C DID (did:key/did:web), Ed25519 sign/verify, multi-sig |
| `@prism/core/encryption` | HKDF-derived AES-GCM-256 vault keys, snapshot encryption |
| `@prism/core/trust` | Luau sandbox, schema validator, hashcash, peer trust, Shamir splitting, escrow, password auth |
| `@prism/core/manifest` | PrismManifest (`.prism.json`), CollectionRef, privilege sets, validation |

### `network/` — everything that crosses the wire
| Export | Purpose |
|--------|---------|
| `@prism/core/relay` | Composable relay builder + 8 modules (mailbox, router, portals, webhooks, WebRTC, …) |
| `@prism/core/presence` | RAM-only peer cursors, selection, TTL eviction |
| `@prism/core/session` | Sessions, transcript timeline, playback controller |
| `@prism/core/discovery` | VaultRoster, filesystem scanning for `.prism.json` |
| `@prism/core/server` | Auto-generate REST routes + OpenAPI 3.1 from ObjectRegistry |

### `domain/` — higher-level domain models
| Export | Purpose |
|--------|---------|
| `@prism/core/flux` | 11 entity types, 7 edge types, 8 automation presets, CSV/JSON import |
| `@prism/core/graph-analysis` | Dependency graph, topological sort, cycle detection, CPM planning |
| `@prism/core/timeline` | Transport, tracks, clips, automation, tempo map (PPQN) |

### `bindings/` — external library adapters (React / DOM / WebGL live here)
| Export | Purpose |
|--------|---------|
| `@prism/core/codemirror` | LoroText bidirectional sync |
| `@prism/core/puck` | Loro-backed visual layout builder |
| `@prism/core/kbar` | Command palette with focus-depth routing |
| `@prism/core/graph` / `xyflow` | @xyflow/react + elkjs auto-layout |
| `@prism/core/shell` | ShellLayout, ActivityBar, TabBar, LensProvider |
| `@prism/core/viewport3d` | Loro-backed scene graph, OpenCASCADE.js CAD import, TSL→GLSL shader compiler |
| `@prism/core/audio` | Bridges Prism timeline to OpenDAW audio engine |

## Key Principles

- **Loro CRDT is the hidden buffer.** Editors project Loro state — they don't own it.
- **Zustand stores subscribe to specific Loro node IDs** — atomic reactivity.
- **`bindings/` is the framework boundary.** Everything above it is framework-agnostic and runs anywhere JS runs.
- **Cross-category imports use `@prism/core/<subsystem>` path aliases**, never deep relative paths.

## Build

```bash
pnpm typecheck    # TypeScript strict
pnpm test         # Vitest
pnpm test:watch   # Watch mode
```
