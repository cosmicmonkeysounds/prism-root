# @prism/core

Client-side execution environment. Organized into 8 domain categories under `src/`, each exposed as a subpath export. No `layer1` / `layer2` split — dependencies flow strictly downward through the DAG:

```
foundation → language/identity → kernel/network → interaction/domain → bindings
```

## Build
- `pnpm typecheck` / `pnpm test` / `pnpm test:watch`

## Import rules
- Inside prism-core, use `@prism/core/<subsystem>` for any import that crosses a category boundary. Only same-category siblings may use relative paths.
- Never `../../foundation/...`-style relative imports across categories — `tsconfig.json` exposes each subsystem under the `@prism/core/*` path alias.
- Loro CRDT is the hidden buffer. Editors project Loro state.
- Zustand stores subscribe to specific Loro node IDs (atomic).

## Categories

### `foundation/` — pure data primitives, no external concerns
- **`object-model/`** → `@prism/core/object-model` — GraphObject, ObjectRegistry, TreeModel, EdgeModel, WeakRefEngine, ContextEngine, NSID/PrismAddress, query/filter helpers, pascal/camel/singular string utilities
- **`persistence/`** → `@prism/core/persistence` — `createCollectionStore` (Loro-backed object/edge storage with ObjectFilter queries and CRDT sync), `createVaultManager` (manifest-driven lifecycle with lazy loading/dirty tracking), `PersistenceAdapter` interface, `createMemoryAdapter`
- **`vfs/`** → `@prism/core/vfs` — `createVfsManager` (content-addressed SHA-256 blob store, BinaryRef for GraphObject.data), `importFile`/`exportFile`, Binary Forking Protocol (`acquireLock`/`releaseLock`/`replaceLockedFile`), `createMemoryVfsAdapter` + `VfsAdapter` interface
- **`crdt-stores/`** → `@prism/core/stores` — Zustand store factories + `useCrdtStore` hook bridging Loro → React
- **`batch/`** → `@prism/core/batch` — `createBatchTransaction` (atomic multi-op execution, 7 op kinds, pre-flight validation, rollback, progress callbacks, single undo entry)
- **`clipboard/`** → `@prism/core/clipboard` — `createTreeClipboard` (cut/copy/paste for GraphObject subtrees, deep clone + ID remapping, internal edge preservation)
- **`template/`** → `@prism/core/template` — `createTemplateRegistry` (ObjectTemplate blueprints with TemplateNode tree / TemplateEdge / TemplateVariable, `{{variable}}` interpolation, `createFromObject` round-trip)
- **`undo/`** → `@prism/core/undo` — `UndoRedoManager` (snapshot-based undo/redo with merge/batch, configurable max history), `createUndoBridge` for auto-recording TreeModel/EdgeModel mutations
- **`loro-bridge.ts`** — shared Loro bridge helper (same-category only, no dedicated export)

### `language/` — languages, parsers, emitters
- **`document/`** → `@prism/core/document` — `PrismFile` + `FileBody` (discriminated union over text/graph/binary). The single file/document abstraction (ADR-002 §A1). `DocumentSurface` is wired through it.
- **`registry/`** → `@prism/core/language-registry` — `LanguageContribution` + `LanguageSurface` + `LanguageCodegen` (ADR-002 §A2) and the unified `LanguageRegistry` (`register`, `resolve({ id?, filename? })`, `resolveByPath`, `getByExtension`). Generic over renderer/editor-extension types so this stays React/CodeMirror-free. Markdown via `createMarkdownContribution()` and Luau via `createLuauContribution()` are the canonical consumers.
- **`markdown/`** → `@prism/core/markdown` — `createMarkdownContribution()`, reuses `parseMarkdown` from `@prism/core/forms` so there is exactly one markdown tokenizer.
- **`expression/`** → `@prism/core/expression` — Expression Engine (Scanner, Parser, Evaluator with builtins; `tokenize`, `isDigit`, `isIdentStart`, `isIdentChar`)
- **`forms/`** → `@prism/core/forms` — FieldSchema, DocumentSchema, FormState, wiki-link parser, markdown parser (block + inline tokens)
- **`syntax/`** → `@prism/core/syntax` — `createSyntaxEngine` (LSP-like diagnostics/completions/hover), schema-aware type checking via SchemaContext, `inferNodeType`, `generateLuauTypeDef` (.d.luau from ObjectRegistry), `createExpressionProvider` + `SyntaxProvider` interface, `BUILTIN_FUNCTIONS`/`FIELD_TYPE_MAP`
- **`luau/`** → `@prism/core/luau` — browser Luau runtime (luau-web), `createLuauEngine`/`executeLuau`, `createLuauDebugger` with `TraceFrame`/`DebugRunResult`
- **`facet/`** → `@prism/core/facet` — Facet System: `FacetParser`, `SpellChecker` + filters, `ProseCodec` (markdown ↔ ProseNode), `Sequencer` (condition + script builders → Luau), language Emitters (TS/JS/C#/Luau/JSON/YAML/TOML writers), `createFacetDefinition`/`FacetDefinitionBuilder`, Value Lists (static + dynamic + registry), Print Config, conditional format evaluation, visual scripts (STEP_KINDS, emitStepsLuau, validateSteps), FacetStore

### `kernel/` — runtime, execution, orchestration
- **`actor/`** → `@prism/core/actor` — Actor System (`createProcessQueue` with priority/concurrency/cancel/prune/events, `createLuauActorRuntime`/`createSidecarRuntime`/`createTestRuntime` pluggable runtimes, `CapabilityScope` zero-trust sandboxing per task) + Intelligence Layer (`createAiProviderRegistry` multi-provider AI, `createOllamaProvider`/`createExternalProvider`, `createContextBuilder` object-aware graph context, `createTestAiProvider`, `AiHttpClient` interface abstraction)
- **`automation/`** → `@prism/core/automation` — AutomationEngine (triggers, conditions, actions, condition evaluator, template interpolation, cron scheduling)
- **`builder/`** → `@prism/core/builder` — Self-Replicating App Builder (AppProfile, BuildTarget, BuildPlan, BuilderManager + build step types for emit-file/run-command/invoke-ipc, used by Studio to compose focused apps)
- **`initializer/`** → `@prism/core/initializer` — generic `KernelInitializer<TKernel>` post-boot hook pattern (`{ id, name, install({ kernel }) => uninstall }`) + `installInitializers()`. Symmetric with `PluginBundle` / `LensBundle` but scoped to side-effects that run AFTER a kernel's construction. Each app specialises `TKernel` with its own kernel type (Studio → `StudioKernel`, etc).
- **`config/`** → `@prism/core/config` — ConfigRegistry, ConfigModel, FeatureFlags, validateConfig, coerceConfigValue, schemaToValidator, MemoryConfigStore. Layered scope resolution: default → workspace → user.
- **`plugin/`** → `@prism/core/plugin` — PluginRegistry, ContributionRegistry (views/commands/keybindings/menus/settings/toolbars/status bar/weak-ref providers), `PrismPlugin` interface
- **`plugin-bundles/`** → `@prism/core/plugin-bundles` — canonical built-in plugin bundles (`createBuiltinBundles`, `installPluginBundles`) used by Studio to register life/platform/finance/work/assets/crm modules
- **`state-machine/`** → `@prism/core/automaton` (also `@prism/core/machines`, `@prism/core/state-machine`) — flat FSM primitives (`Machine`, `createMachine` with guards/actions/lifecycle hooks/start/restore)

### `interaction/` — UI-facing state (React-agnostic)
- **`atom/`** → `@prism/core/atom` — Reactive Atoms (PrismBus event bus, AtomStore UI state, ObjectAtomStore object/edge cache, `connectBusToAtoms`/`connectBusToObjectAtoms` bridges, selectors)
- **`layout/`** → `@prism/core/layout` — SelectionModel, PageModel, PageRegistry, LensSlot, LensManager
- **`lens/`** → `@prism/core/lens` — Lens system + shell state. Includes `LensBundle<TComponent>` / `installLensBundles` / `defineLensBundle` — self-registering bundles pairing a manifest with its component. Generic over component type so this stays React-free; Studio specializes it to `ComponentType` at its own layer.
- **`input/`** → `@prism/core/input` — KeyboardModel, InputScope, InputRouter (`parseShortcut`, `normaliseKeyEvent`, `keyToShortcut`)
- **`activity/`** → `@prism/core/activity` — ActivityStore (append-only per-object audit trail, 20 verbs, ring buffer eviction), ActivityTracker (auto-derives events from GraphObject diffs via TrackableStore), formatActivity/formatFieldName/formatFieldValue/groupActivityByDate
- **`notification/`** → `@prism/core/notification` — NotificationStore (8 kinds, add/markRead/dismiss/pin, filter, eviction policy, subscriptions), NotificationQueue (debounced batching with dedup by objectId+kind)
- **`search/`** → `@prism/core/search` — SearchIndex (TF-IDF inverted index with field-weighted scoring), SearchEngine (cross-collection orchestrator with structured filters/facets/pagination/live subscriptions, auto-reindex via CollectionStore change events)
- **`view/`** → `@prism/core/view` — Derived Views: `createViewRegistry` (7 view modes with capability queries), `applyFilters`/`applySorts`/`applyGroups`/`applyViewConfig` (pure transform pipeline, 12 filter operators), `createLiveView` (auto-updating materialized projection), `createSavedView`/`createSavedViewRegistry` (persistable named view configs aka "Found Sets")
- **`design-tokens/`** → `@prism/core/design-tokens` — `DesignTokenRegistry` (colors/spacing/fonts buckets), `tokensToCss`, `lookupToken`, `mergeTokens`, `DEFAULT_TOKENS`. Framework-agnostic — emits plain CSS strings. Extracted from Studio in ADR-002 Phase 3.
- **`page-builder/`** → `@prism/core/page-builder` — `BlockStyleData` + `STYLE_FIELD_DEFS` (spread into entity defs for per-block styling), `computeBlockStyle`/`extractBlockStyle`/`mergeCss`/`resolveShadow`, responsive overrides (`mediaRule`, `computeMobileOverride`, `computeTabletOverride`, `BREAKPOINTS`), and `exportPageToJson`/`exportPageToHtml`/`renderNodeHtml`/`toExportedNode` (deterministic `prism-page/v1` snapshot + dependency-free HTML). Framework-agnostic — style bags are `Record<string, string | number>` (structurally compatible with React's `CSSProperties`). Extracted from Studio in ADR-002 Phase 3.

### `identity/` — DID, keys, trust, manifest
- **`did/`** → `@prism/core/identity` — `createIdentity` (W3C DID identity, Ed25519 keypair for did:key/did:web), `resolveIdentity`, `signPayload`/`verifySignature`, multi-sig (`createMultiSigConfig`/`createPartialSignature`/`assembleMultiSignature`/`verifyMultiSignature`), base58btc/multicodec encoding utilities
- **`encryption/`** → `@prism/core/encryption` — `createVaultKeyManager` (HKDF-derived AES-GCM-256 vault/collection keys with rotation), `encryptSnapshot`/`decryptSnapshot` (Loro snapshot encryption at rest with AAD), `createMemoryKeyStore` + `KeyStore` interface for Tauri keychain integration
- **`trust/`** → `@prism/core/trust` — `createLuauSandbox` (capability-based API restriction with glob URL/path filtering and violation recording), `createSchemaValidator` (built-in import-safety rules), `createHashcashMinter`/`createHashcashVerifier` (SHA-256 proof-of-work spam protection), `createPeerTrustGraph` (peer reputation with trust/distrust/ban + content hash flagging), `createShamirSplitter` (GF(256) Shamir secret sharing for vault recovery), `createEscrowManager` (deposit/claim/evict lifecycle with TTL), `createPasswordAuthManager`
- **`manifest/`** → `@prism/core/manifest` — PrismManifest (weak references to Collections in a Vault; StorageConfig/SchemaConfig/SyncConfig/CollectionRef, parse/serialise/validate) + Access Control (`createPrivilegeSet` with collection/field/layout/script/row-level permissions, RoleAssignment DID→PrivilegeSet mapping, `createPrivilegeEnforcer`). See `manifest-types.ts` for Vault/Collection/Manifest/Shell glossary.

### `network/` — everything that crosses the wire
- **`relay/`** → `@prism/core/relay` — `createRelayBuilder` (composable relay via builder pattern with `.use()` chaining). 8 built-in modules: `blindMailboxModule` (E2EE store-and-forward), `relayRouterModule` (zero-knowledge routing), `relayTimestampModule` (cryptographic timestamps), `blindPingModule` (push notifications), `capabilityTokenModule` (scoped access tokens), `webhookModule` (outgoing HTTP on CRDT changes), `sovereignPortalModule` (portal levels 1–4), `webrtcSignalingModule` (P2P/SFU connection negotiation). `RelayModule` interface for custom modules, `RelayContext` shared capability registry, `RELAY_CAPABILITIES` registry, `createMemoryPingTransport` for testing.
- **`presence/`** → `@prism/core/presence` — `createPresenceManager` (RAM-only peer state tracking: CursorPosition/SelectionRange/activeView, PeerIdentity with color/displayName, receiveRemote for awareness protocol, TTL-based sweep eviction, joined/updated/left events, injectable TimerProvider for testing)
- **`session/`** → `@prism/core/session` — Communication Fabric: `createSessionManager` (session lifecycle, participants with roles/muting, media tracks), `createTranscriptTimeline` (sorted searchable time-indexed segments with finalization), `createPlaybackController` (transcript-synced media seek), `TranscriptionProvider`/`SessionTransport` interfaces for Whisper.cpp/LiveKit/WebRTC, Listener Fallback delegation to capable peers, `createTestTransport`/`createTestTranscriptionProvider`
- **`discovery/`** → `@prism/core/discovery` — Vault Discovery: `createVaultRoster` (persistent registry of known vaults with CRUD/sort/pin/search/dedup via RosterStore), `createVaultDiscovery` (filesystem scanning for `.prism.json` manifests with DiscoveryAdapter, roster merge, discovery events), `createMemoryRosterStore`/`createMemoryDiscoveryAdapter`
- **`server/`** → `@prism/core/server` — Server Factory: `generateRouteSpecs` (framework-agnostic RouteSpec[] from ObjectRegistry), `registerRoutes` + `RouteAdapter`, `buildOpenApiDocument`/`generateOpenApiJson` (OpenAPI 3.1.0 from ObjectRegistry), `groupByType`, `printRouteTable`

### `domain/` — higher-level domain models
- **`flux/`** → `@prism/core/flux` — Flux Domain: `createFluxRegistry` with 11 entity types across 4 categories (productivity: Task/Project/Goal/Milestone; people: Contact/Organization; finance: Transaction/Account/Invoice; inventory: Item/Location), 7 edge types, 8 automation presets, CSV/JSON import/export with field selection
- **`graph-analysis/`** → `@prism/core/graph-analysis` — Dependency Graph (build, topological sort, cycle detection, blocking chain, impact, slip impact) + Planning Engine (CPM critical path, early/late start/finish, float)
- **`timeline/`** → `@prism/core/timeline` — NLE / Timeline System: `createTimelineEngine` (transport/tracks/clips/automation/markers with pluggable clock), `createTempoMap` (PPQN dual-time with tempo automation and time signatures), `createManualClock` for testing

### `bindings/` — external library adapters (React/DOM/WebGL)
- **`codemirror/`** → `@prism/core/codemirror` — CodeMirror 6 + LoroText sync extensions
- **`puck/`** → `@prism/core/puck` — Puck layout bridge backed by Loro
- **`kbar/`** → `@prism/core/kbar` — KBar command palette with focus-depth routing
- **`xyflow/`** → `@prism/core/graph` (alias `@prism/core/xyflow`) — Spatial node graph (`PrismGraph`, custom nodes/edges, elkjs auto-layout)
- **`react-shell/`** → `@prism/core/shell` — React shell components (ShellLayout, ActivityBar, TabBar, LensProvider), table/report/csv surfaces
- **`viewport3d/`** → `@prism/core/viewport3d` — 3D Viewport / Builder 3: `createSceneState` (Loro-backed scene graph with nodes/materials/hierarchy/transforms/CRDT sync), `createCadGeometryManager` (OpenCASCADE.js STEP/IGES/BREP import with tessellation via `CadWorkerAdapter`), `compileTslGraph` (TSL node-wire graph → GLSL, topological sort/type checking/cycle detection, WebGL/WebGPU targets), `createGizmoController` (translate/rotate/scale with snapping and undo via `GizmoUndoAdapter`), `createTestCadAdapter`
- **`audio/`** → `@prism/core/audio` — OpenDAW Audio Bridge: `createOpenDawBridge` (Prism timeline ↔ OpenDAW audio engine, bidirectional transport sync via AnimationFrame, track loading with AudioFileBox/AudioRegionBox/PPQN conversion, 10 audio effects, exportMix/exportStems to WAV), `installOpenDawWorkers`, React hooks `useOpenDawBridge`/`usePlaybackPosition`/`useTransportControls`/`useTrackEffects`

## Dependency DAG (enforced by imports, not tooling)

```
                 foundation
                /          \
           language      identity
              |             |
           kernel  ←---→  network
              \           /
               interaction
                    |
                 domain
                    |
                 bindings
```

- Anything in a higher row may import from a lower row, never the other way.
- `kernel` and `network` may import from each other for now (e.g. `actor` depends on `relay` capability types) but must not reach down into `interaction`/`bindings`.
- `bindings` is the only layer allowed to import React / DOM / WebGL.
