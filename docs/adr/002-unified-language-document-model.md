# ADR-002: Unified language/document model and core/studio restructuring

**Status**: Proposed
**Date**: 2026-04-11

## Context

A survey of `prism-core` and `prism-studio` revealed four related problems.

### 1. Four overlapping systems that never meet

Parsing, code generation, document rendering, and persistence live in disjoint subsystems with no shared abstractions:

- **Syntax** — `LanguageRegistry` + `LanguageDefinition` (`packages/prism-core/src/layer1/syntax/language-registry.ts:52-118`), plus `SyntaxProvider` (`syntax-types.ts:125-137`) for expression-level diagnostics/completions/hover. Only Luau and the expression language are registered. `LanguageDefinition.serialize?` is declared but never implemented.
- **Codegen** — Two parallel hierarchies. `Emitter<SymbolDef[]>` + `CodegenPipeline` in `syntax/codegen/` (symbol-oriented, emits TS/C#/GDScript) and a second set of schema writers in `facet/emitters.ts` (TypeScriptWriter/JavaScriptWriter/CSharpWriter/LuauWriter/JsonWriter/YamlWriter/TomlWriter) that consume `SchemaModel` instead of `SymbolDef[]`. Neither is pluggable into the other. Additionally, `facet/facet-builders.ts` emits Luau strings ad-hoc without going through any `Emitter` at all.
- **Document Surfaces** — A **third registry**, `DocumentSurfaceRegistry` (`document-types.ts:111-176`), registers Markdown/YAML/JSON/HTML/CSV/SVG/plaintext with a `SurfaceMode` (code/preview/form/spreadsheet/report). Markdown has a surface but no `LanguageDefinition`. Luau has a `LanguageDefinition` but no surface. The two registries are disjoint; they are linked only by optional `codemirrorExtensionIds?: string[]` — a fragile string-array escape hatch.
- **Documents / Files** — There is no `Document` type. A "file" is any of: raw `string`, `LoroText`, `GraphObject` (`object-model/`), `BinaryRef` (`vfs/vfs-types.ts:16-27`), or `DocumentSchema` (`forms/document-schema.ts`). Persistence is via `CollectionStore` → `VaultManager` → `PersistenceAdapter`. `DocumentSurface` is a pure `(value, onChange)` renderer with no awareness of any of it; callers reinvent persistence every time. The VFS (binary) and Loro (text/records) never meet.

### 2. Luau lives in two directories

- `packages/prism-core/src/layer1/syntax/luau/` — parsing via full-moon WASM AST (added 2026-04-10)
- `packages/prism-core/src/layer1/luau/` — `luau-runtime.ts` (execution via luau-web) and `luau-debugger.ts` (trace instrumentation)

`luau-debugger.ts` depends on `syntax/luau/luau-ast` but lives outside it. Imports are confusing and there is no coherent "Luau" home.

### 3. `layer1/` + `layer2/` has outlived its usefulness

The `layer1` (framework-agnostic domain) / `layer2` (React UI) split emerged informally when Layer 1 held ~10 subsystems. It now holds **~40 subsystems across ~330 files**, and the binary split has become a junk drawer:

- `layer1/` is not a category, it's a negation ("things that aren't React"). Navigation collapses to scanning an alphabetical list of 40 folders.
- Related subsystems are adjacent only by accident. `automaton/`, `automation/`, and `machines/` all live next to each other because they start with "a" or "m", not because of a considered grouping.
- Cross-cutting concerns — persistence, language, identity, collaboration, kernel orchestration — have no home at all; they bleed across sibling folders.
- `layer2/` (React) is coherent but its name commits to a two-layer ontology that no longer describes reality. There is no `layer3`. It's just "the framework-coupled stuff."

The layer split also produces perverse outcomes: `studio-kernel.ts` is 95% framework-agnostic and belongs in Core, but the only place to put it is `layer1/kernel/` — a folder whose name tells you nothing about what a kernel is or where its peers live.

### 4. `prism-core` naming and size drift; `prism-studio` kernel is Core-ready

**Core** organizational smells:

- **Naming inconsistencies**: `plugin` (registry) vs `plugins` (bundles), `automaton` (flat FSM, 3 files) vs `automation` (trigger/condition/action engine) vs `machines` (XState FSM, 3 files). Plural/singular is inconsistent across the tree.
- **Grab-bag subsystems**: `facet/` is 33 files and ~10K LOC mixing parser, runtime, codegen, spell engine, prose codec, spatial layout, script steps, and emitters.
- **AI/actor coupling**: `layer1/actor/` holds both the `ProcessQueue` executor **and** the `AiProviderRegistry`/`ContextBuilder` intelligence layer — two distinct concerns sharing a folder because neither had anywhere else to go.
- **No "kernel" in Core**. ProcessQueue is an executor; there is no event loop, command dispatcher, or centralized wiring — that logic all lives in Studio.

**Studio** has 121 TS/TSX files (~38,700 LOC). The standout finding: `studio-kernel.ts` (2,051 LOC) is a singleton wiring layer that owns RelayManager, BuilderManager, DesignTokenRegistry, page-export, block-style, and the initializer pattern. It imports from 37 `@prism/core/*` subsystems. **Its only React dependencies are two type-only imports** (`ComponentType`, `CSSProperties`) that can be removed. The kernel is Core-ready; it is only sitting in Studio by accident.

## Decision

Adopt a unified model built around two new abstractions — **`PrismFile`** and **`LanguageContribution`** — restructure `prism-core` around **eight domain categories**, replacing the `layer1/`/`layer2/` split entirely, and extract Studio's kernel to Core.

This ADR describes the target architecture. A staged migration plan follows in §Consequences.

### Part A — Unified language + document model

#### A1. `PrismFile`: the missing abstraction

Introduce `PrismFile` as the single file/document type that bridges persistence, syntax, and rendering:

```ts
// language/document/prism-file.ts
interface PrismFile {
  path: string;                 // NSID or VFS path
  languageId?: string;          // resolves a LanguageContribution
  surfaceId?: string;           // explicit surface override; defaults via languageId
  body: FileBody;               // text | graph | binary (discriminated union)
  schema?: DocumentSchema;      // optional form/field schema
  metadata?: Record<string, unknown>;
}

type FileBody =
  | { kind: "text"; ref: LoroText | string }
  | { kind: "graph"; ref: GraphObject }
  | { kind: "binary"; ref: BinaryRef };
```

`PrismFile` is the contract that Surfaces, Syntax, Codegen, and Persistence agree on. It is the answer to "what is a file in Prism?"

#### A2. `LanguageContribution`: unify `LanguageDefinition` + `DocumentContribution`

Collapse the two registries into one. A `LanguageContribution` owns everything about a format — parsing, codegen, editor UI, and optional form schema:

```ts
// language/registry/language-contribution.ts
interface LanguageContribution {
  id: string;                          // "prism:luau", "prism:markdown"
  extensions: string[];
  displayName: string;

  // Syntax (optional — binary formats may omit)
  parse?(text: string): RootNode;
  serialize?(ast: RootNode): string;   // round-trip; actually implemented now
  syntaxProvider?(): SyntaxProvider;   // diagnostics, completion, hover
  codemirrorExtensions?: () => Promise<Extension[]>;

  // Surface (editor UI)
  surface: {
    defaultMode: SurfaceMode;
    availableModes: SurfaceMode[];
    inlineTokens?: InlineToken[];
    renderers: Partial<Record<SurfaceMode, SurfaceRenderer>>;
  };

  // Codegen (optional)
  codegen?: {
    emitters: Emitter<unknown>[];      // keyed by output kind
  };
}
```

A single `LanguageRegistry.register(contribution)` call replaces the current split between `LanguageRegistry` and `DocumentSurfaceRegistry`. Resolution: `registry.resolveByPath(path)` → `LanguageContribution`, then `contribution.surface.renderers[mode]`.

Markdown gains a real `parse()` (unified with the existing prose codec). Luau gains a `surface` with code + debug modes. `serialize()` becomes non-optional at the type level for any language that claims structured editing.

#### A3. Unified codegen input

Collapse `Emitter<SymbolDef[]>` and the schema writers into a single pipeline keyed by input kind:

```ts
interface Emitter<In, Out = string> {
  id: string;
  inputKind: "symbols" | "schema" | "ast" | "facet";
  emit(input: In): CodegenResult<Out>;
}
```

The `CodegenPipeline` accepts heterogeneous emitters and routes by `inputKind`. `facet/emitters.ts` (Writers) and `syntax/codegen/symbol-emitter.ts` both conform. Ad-hoc Luau string builders in `facet-builders.ts` migrate onto a `LuauEmitter` behind the pipeline.

Additionally, wire `CodegenPipeline` into `Processor.compile` (`language-registry.ts:139-200`) so the compile phase actually produces code instead of being a dead hook.

#### A4. Folded Luau

Luau's parser, runtime, debugger, provider, and codegen all live in one directory:

```
language/luau/
  parse.ts          (was syntax/luau/luau-ast.ts)
  provider.ts       (was syntax/luau/luau-provider.ts)
  contribution.ts   (new: LanguageContribution for Luau)
  runtime.ts        (was luau/luau-runtime.ts)
  debugger.ts       (was luau/luau-debugger.ts)
  codegen.ts        (new: LuauEmitter, absorbs facet-builders Luau output)
```

One directory, one story.

### Part B — `prism-core` restructuring: domain categories, not layers

Drop `layer1/` and `layer2/`. Organize `packages/prism-core/src/` into **eight domain categories**, each with a clear purpose and a natural dependency direction. The category is the navigation unit; subsystems live inside their category.

```
packages/prism-core/src/
  foundation/           — the data truth
    object-model/       — GraphObject, ObjectRegistry, TreeModel, EdgeModel, WeakRefEngine, NSID
    persistence/        — CollectionStore, VaultManager, PersistenceAdapter
    vfs/                — BinaryRef, content-addressed blobs
    crdt-stores/        — Zustand wrappers (was stores/)
    batch/              — BatchTransaction
    clipboard/          — TreeClipboard
    template/           — TemplateRegistry
    undo/               — UndoRedoManager, undo bridge
    loro-bridge.ts

  language/             — files, syntax, codegen, forms
    document/           — PrismFile, FileBody, persistence bridge (NEW)
    registry/           — LanguageRegistry, LanguageContribution (unified)
    luau/               — parse, runtime, debugger, provider, codegen, contribution
    markdown/           — parse, prose-codec, contribution
    expression/         — scanner, parser, evaluator, provider
    codegen/            — unified Emitter<InputKind>, CodegenPipeline (moved up from syntax/)
    forms/              — FieldSchema, DocumentSchema, FormState
    facet/              — FileMaker-inspired visual-builder sub-language
      parser/           — facet-parser, facet-schema, value-list
      runtime/          — spatial-layout, spell-engine, prose-codec, facet-runtime
      codegen/          — script-steps, sequencer (emitter impls register into language/codegen/)
    syntax/             — shared SyntaxProvider interface, AST types

  kernel/               — orchestration, runtime, wiring
    prism-kernel.ts     — PrismKernel factory (was studio-kernel.ts)
    kernel-context.ts   — framework-agnostic kernel handle interface
    initializer/        — KernelInitializer pattern
    actor/              — ProcessQueue, ActorRuntime (executors only)
    intelligence/       — AiProviderRegistry, ContextBuilder, providers (split from actor/)
    automation/         — trigger/condition/action engine
    state-machine/      — merges automaton/ + machines/ (flat-fsm.ts + xstate-fsm.ts)
    config/             — ConfigRegistry, ConfigModel, FeatureFlags
    plugin/             — PluginRegistry, ContributionRegistry (framework)
    plugin-bundles/     — work/finance/crm/life/assets/platform (was plugins/)
    builder/            — BuilderManager, BuildPlan, BuildExecutor interface

  interaction/          — the running editor experience (mechanism, not content)
    atom/               — PrismBus, AtomStore, ObjectAtomStore
    layout/             — PageModel, LensSlot, LensManager
    lens/               — LensRegistry, ShellStore, LensBundle
    input/              — KeyboardModel, InputScope, InputRouter
    activity/           — ActivityStore, ActivityTracker
    notification/       — NotificationStore, NotificationQueue
    search/             — SearchIndex, SearchEngine
    design-tokens/      — DesignTokenRegistry, tokensToCss
    page-builder/       — block-style, page-export (extracted from Studio)

  identity/             — who you are and what you can do
    did/                — DID key/web, sign/verify, multi-sig (was identity/)
    encryption/         — VaultKeyManager, snapshot encryption
    trust/              — LuauSandbox, Hashcash, PeerTrustGraph, Escrow, ShamirSplitter
    manifest/           — PrismManifest, PrivilegeSet, PrivilegeEnforcer

  network/              — talking to other peers and servers
    relay/              — modular relay (21 files)
    relay-manager/      — connection manager (extracted from Studio)
    presence/           — PresenceManager
    session/            — SessionManager, TranscriptTimeline, PlaybackController
    discovery/          — VaultRoster, VaultDiscovery
    server/             — route specs, OpenAPI generation

  domain/               — app-level content and domain entities
    flux/               — Task, Project, Goal, Milestone, Contact, … (11 entity types)
    timeline/           — TimelineEngine, TempoMap
    graph-analysis/     — topo sort, CPM, blocking chains

  bindings/             — framework-coupled adapters (was layer2/)
    react-shell/        — ShellLayout, DocumentSurface host, LensProvider, print-renderer
    codemirror/         — CM6 + Loro sync, spell-check, inline-tokens
    puck/               — Puck + Loro layout bridge
    kbar/               — command palette
    xyflow/             — spatial node graph + elkjs layout (was graph/)
    viewport3d/         — 3D scene, CAD, TSL shaders
    audio/              — OpenDAW bridge
```

**Dependency direction** (a category may import from categories below it, not above):

```
                bindings/
                   ↑
         interaction/    domain/
                ↑           ↑
              kernel/      network/
                ↑           ↑
         language/      identity/
                ↑           ↑
             foundation/
```

- `foundation/` imports nothing outside itself — it's the data truth
- `language/` and `identity/` build on foundation
- `kernel/` and `network/` build on language + identity + foundation
- `interaction/` and `domain/` build on kernel
- `bindings/` is the only place React/DOM/CodeMirror/Puck/Tauri may appear

This replaces the binary `layer1`/`layer2` rule with a richer DAG that matches how the code actually flows.

**Rename rules applied**:

- Consistent naming: singular by default, plural only for true collections (`plugin-bundles`, `crdt-stores`).
- `automaton` + `machines` → merged as `kernel/state-machine/`.
- `plugins/` → `kernel/plugin-bundles/`.
- `stores/` → `foundation/crdt-stores/`.
- `identity/` → `identity/did/` (the folder holds only DID logic; encryption/trust/manifest are siblings under `identity/`).
- `graph/` (xyflow) → `bindings/xyflow/` (frees the name for potential future graph logic in `domain/`).
- `facet/` split internally into `parser`/`runtime`/`codegen` under `language/facet/`.

### Part C — `PrismKernel`: extract Studio's kernel to Core

Create `kernel/prism-kernel.ts` as the canonical wiring layer. It owns instances of ObjectRegistry, CollectionStore, PrismBus, AtomStore, UndoRedoManager, NotificationStore, SearchEngine, ActivityStore, ConfigModel, PresenceManager, AutomationEngine, PluginRegistry, InputRouter, VaultRoster, Identity, VfsManager, Trust subsystem, Facet system, LensRegistry, DesignTokenRegistry — **all of the framework-agnostic orchestration currently in `studio-kernel.ts`**.

`PrismKernel` is a configurable factory:

```ts
interface PrismKernelConfig {
  lensBundles?: LensBundle[];
  initializers?: KernelInitializer[];
  persistenceAdapter?: PersistenceAdapter;
  buildExecutor?: BuildExecutor;
  aiProviders?: AiProvider[];
}
function createPrismKernel(config: PrismKernelConfig): PrismKernel;
```

Studio, Flux, Lattice, and Musica all instantiate `PrismKernel` with their own lens bundles and initializers. No more per-app kernel reinvention.

Extract-to-Core subsystems (all confirmed pure by the audit):

| From Studio | To Core | LOC | Notes |
|---|---|---|---|
| `kernel/studio-kernel.ts` (core wiring) | `kernel/prism-kernel.ts` | ~1,800 of 2,051 | Remove `ComponentType` type import; use generic `TComponent` param |
| `kernel/relay-manager.ts` | `network/relay-manager/` | 613 | Lives next to `network/relay/` |
| `kernel/builder-manager.ts` | `kernel/builder/` | 397 | `BuildExecutor` stays injectable |
| `kernel/design-tokens.ts` | `interaction/design-tokens/` | 109 | New folder |
| `kernel/page-export.ts` | `interaction/page-builder/page-export.ts` | 251 | Pure serialization |
| `kernel/block-style.ts` | `interaction/page-builder/block-style.ts` | 310 | Replace `CSSProperties` return type with `Record<string, string \| number>` |
| `kernel/initializer.ts` + pattern | `kernel/initializer/` | 48 | Pattern only; Studio keeps its own `builtin-initializers.ts` |

After extraction, Studio's `kernel/` holds only: `studio-kernel.ts` (thin wrapper that calls `createPrismKernel` with Studio lens bundles + initializers), `kernel-context.tsx` (React Context + 45+ hooks, stays), `builtin-initializers.ts` (Studio templates/demo content), and `entities.ts` (Studio's page-builder domain model, 1,103 LOC — stays).

### Part D — `prism-studio` restructuring

Post-extraction Studio shape:

```
packages/prism-studio/src/
  main.tsx
  App.tsx
  ipc-bridge.ts
  wasm-bootstrap.ts
  kernel/
    studio-kernel.ts         — thin: wraps createPrismKernel() with Studio config
    kernel-context.tsx       — React Context + hooks
    builtin-initializers.ts  — Studio templates, demo workspace
    entities/                — page-builder entity registry, split by category
      page.ts                — page, section entities
      block.ts               — heading, text, button, card, image
      luau.ts                — luau-block entity
      facet.ts               — facet-view entity
      spatial.ts             — spatial-canvas entity
      data-portal.ts         — data-portal entity
  panels/                    — 41 lens components
  lenses/                    — lens bundle aggregator
  components/                — 29 shared React UI primitives
```

The 1,103-LOC `entities.ts` file is split by category — the only non-move Studio change. The 41-panel flat directory is acceptable given the lens bundle pattern, but a README grouping them by feature (data-view, automation, design, etc.) would help navigation.

## Rationale

### Why domain categories beat `layer1`/`layer2`

The layer binary encodes **one** fact: "does this touch React?" That question was useful when the answer meaningfully partitioned the codebase. With 40 subsystems in Layer 1, the partition is now one tiny half (`layer2/`, 7 folders) and one sprawling half (`layer1/`, 40). The useful question has become "what does this subsystem *do*?" — and the layer names answer "nothing specific."

Domain categories answer that question at the folder level. `kernel/` tells you it's orchestration. `language/` tells you it's about files. `network/` tells you it's about peers. The dependency DAG enforces the same invariant the layer split did (React only in `bindings/`) while also enforcing richer rules: `language/` can't import from `kernel/`, `foundation/` can't import from anything, etc. One weaker rule replaced with eight stronger ones.

The DAG also gives `PrismKernel` a natural home that the layer split couldn't. `kernel/` is the name of its role. `layer1/kernel/` was the name of what it wasn't.

### Why `interaction/` instead of `workspace/`

"Workspace" is already a loaded term in Prism's domain model — per the in-repo convention, a workspace is the *identity envelope* (manifest + vault + shell), not the editing experience. Using `workspace/` as a directory name would collide. `interaction/` captures the actual concern: the subsystems that mediate between user input and graph state (input, layout, lens, atom bus, activity, notification, search, design tokens, page-builder).

### Why `PrismFile` and `LanguageContribution` together

The four systems are fragmented because they have no shared vocabulary. `PrismFile` gives persistence and rendering a common type. `LanguageContribution` gives syntax, codegen, and surfaces a common registry. Together, resolving a path becomes: `registry.resolveByPath(file.path)` → everything needed to parse, render, and emit from it. Today that takes three registry lookups and a manual bridge.

### Why unify codegen now

Two emitter hierarchies that cannot compose is technical debt that grows with every new target language. Luau codegen is currently ad-hoc strings; without unification, adding (say) GDScript or Swift would duplicate the drift. Routing by `inputKind` keeps the change additive — existing emitters conform with a one-line discriminator.

### Why split `facet/`

10K LOC in a single folder is too large to reason about, and its contents cleanly split along the parser / runtime / codegen seam. The split also lets facet's emitters register into the unified `language/codegen/` pipeline without dragging the facet parser along.

### Why extract the kernel now

The audit confirmed `studio-kernel.ts` is 95% framework-agnostic and owns logic every other Prism app will need (RelayManager, BuilderManager, design tokens, page export, initializer pattern). Waiting until Flux, Lattice, or Musica exist means duplicating 1,500+ LOC three times or doing a harder extraction with three sets of drift to reconcile. Doing it now, with Studio as the only consumer, is the cheapest moment.

### Why `PrismKernel` belongs in Core, not a separate package

It depends on dozens of Core subsystems and is itself wired-together Core logic. A separate package would only add a boundary that every app has to cross. `core/kernel/` is the right home.

### Why rename is low-risk

Per CLAUDE.md: "Never deprecate. Rename, move, break, fix. `tsc --noEmit` is your safety net." All renames and moves are import-only changes; TypeScript will catch every miss. `@prism/*` path aliases absorb most of the churn — external consumers see the new subpath exports and internal code sees the new relative paths.

## Consequences

### Positive

- **Single file abstraction**: `PrismFile` unifies persistence, rendering, and parsing. Callers stop reinventing the save/load dance.
- **Single language registry**: `LanguageContribution` replaces the disjoint `LanguageRegistry`/`DocumentSurfaceRegistry` split. Adding a new language means one registration, not two.
- **Codegen composes**: `CodegenPipeline` accepts all emitter kinds. `LanguageDefinition.serialize` becomes real.
- **Luau has one home**: `language/luau/` holds parse, runtime, debugger, codegen, and contribution.
- **Core is navigable**: 8 broad categories replace 40 flat folders. The category tells you the role.
- **Dependency DAG**: stronger than the old layer rule. `foundation/` can't import from anything; `bindings/` is the only React home; `language/` can't reach into `kernel/`.
- **Kernel reusable**: Flux, Lattice, Musica inherit RelayManager, BuilderManager, design tokens, page export, initializer pattern for free.
- **Studio shrinks**: `kernel/` goes from 22 files to a thin wrapper plus Studio-specific content.

### Negative

- **Very large diff**: dropping `layer1/`/`layer2/` touches nearly every file in Core via imports. `@prism/*` path aliases soften this, but PR review surface is substantial.
- **Package.json subpath exports churn**: 40+ entries in `prism-core/package.json` need rewriting. Downstream consumers (daemon, studio, tests, CLI) all need import updates.
- **External documentation churn**: `SPEC.md`, per-package `CLAUDE.md` files, and `docs/dev/current-plan.md` all mention `layer1`/`layer2` and need updating as part of the migration.
- **Behavioral risk at seams**: `PrismFile` + `LanguageContribution` are new abstractions. The first pass will mis-model edge cases (binary-with-schema files, languages with multiple surfaces). Expect 1–2 follow-up revisions.
- **Category boundaries will be contested**: some subsystems sit uncomfortably between categories. `forms/` in language vs interaction. `page-builder/` in interaction vs bindings. `design-tokens/` in interaction vs bindings. Each call needs to be made and defended; some will be revisited.
- **Codegen `inputKind` discriminator is a mild runtime cost**: emitter dispatch goes through a switch. Negligible in practice; noted for completeness.

### Mitigations: staged migration

Five phases, each independently shippable. Each phase ends with `pnpm test` green (3006 tests per CLAUDE.md), `pnpm typecheck` clean, and `docs/dev/current-plan.md` updated.

#### Phase 1 — New types, compat bridge (additive, low risk)

- Introduce `PrismFile`, `FileBody`, and `LanguageContribution` as new types in their current `layer1/` homes.
- Write a compatibility bridge that adapts existing `LanguageDefinition` + `DocumentContribution` into `LanguageContribution` on read.
- No renames, no moves. Existing code continues to work unchanged.

#### Phase 2 — Fold Luau (small move, isolated blast radius)

- Merge `layer1/luau/` into `layer1/syntax/luau/`.
- Rename the merged folder to `layer1/language/luau/` in-place.
- Update `luau-debugger.ts` imports to co-located paths.
- Add `contribution.ts` registering the unified Luau `LanguageContribution`.

#### Phase 3 — Extract kernel to Core (medium move)

- Create `layer1/kernel/prism-kernel.ts` as a copy of `studio-kernel.ts` with React type imports removed.
- Move `RelayManager`, `BuilderManager`, `DesignTokenRegistry`, `page-export.ts`, `block-style.ts` to temporary Core homes (under their current `layer1/` neighbors — `layer1/relay/`, `layer1/builder/`, etc.). Do not yet move to their final `network/`/`interaction/` homes; that happens in Phase 5.
- Rewrite `studio-kernel.ts` as a thin wrapper that calls `createPrismKernel(config)` with Studio lens bundles and initializers.
- Studio's `kernel-context.tsx`, `builtin-initializers.ts`, `entities.ts` stay.

#### Phase 4 — Unify codegen and language registry (behavioral)

- Introduce the `Emitter<In>` discriminator in `layer1/syntax/codegen/`.
- Migrate `facet/emitters.ts` writers to conform to the new `Emitter` shape.
- Migrate `facet-builders.ts` Luau output onto a new `LuauEmitter`.
- Wire `CodegenPipeline` into `Processor.compile`.
- Collapse `DocumentSurfaceRegistry` into `LanguageRegistry`. Retire the compatibility bridge from Phase 1.
- Markdown gains a real `parse()`. Luau gains a `surface` with code + debug modes.

#### Phase 5 — Domain category reorg (large diff, pure moves + renames)

Single shot. No behavior changes. `tsc --noEmit` is the safety net.

1. Create the 8 top-level category directories under `packages/prism-core/src/`.
2. Move every existing `layer1/X/` into its new category home per Part B.
3. Apply renames: `automaton` + `machines` → `kernel/state-machine/`, `plugins` → `kernel/plugin-bundles/`, `stores` → `foundation/crdt-stores/`, `graph` → `bindings/xyflow/`, etc.
4. Split `language/facet/` into `parser/` + `runtime/` + `codegen/`.
5. Split `actor/` into `kernel/actor/` (executors) + `kernel/intelligence/` (AI).
6. Move `codegen/` up from inside `syntax/` to `language/codegen/`.
7. Move `layer2/` contents into `bindings/`.
8. Delete `layer1/` and `layer2/` directories.
9. Update `packages/prism-core/package.json` subpath exports to the new paths.
10. Update `CLAUDE.md` files, `SPEC.md`, and `docs/dev/current-plan.md` to reflect the new structure.

Split `packages/prism-studio/src/kernel/entities.ts` by entity category in the same phase.

#### Do-not-touch rules during Phase 5

- No behavioral changes. Renames and moves only.
- No new types. No refactors of function signatures. No test rewrites — tests move with their files.
- Any temptation to fix something else is deferred to a follow-up.
