# WYSIWYG Fullstack App Builder — Roadmap

> **Scope.** This doc tracks the *product* path: from where the codebase
> is today to the full WYSIWYG fullstack app builder Prism is meant to
> be. It is deliberately distinct from `state-of-prism.md`, which is the
> migration/cleanup tracker (ADR + dev-plan reconciliation). Read that
> one for "is plan X landed"; read this one for "what does done look
> like and what's between here and there."
>
> Status claims are point-in-time (2026-05-16) and synthesised from the
> primary docs + a code sweep. Per house discipline, verify load-bearing
> claims against the code — `cargo check --workspace` is the safety net.

## 0. What "full WYSIWYG fullstack app builder" means for Prism

Five capabilities, each with a concrete acceptance bar:

1. **Visual canvas authoring.** A user drags a block onto a page, sees
   it render exactly as it will ship (femtovg native / WebGL web /
   semantic-HTML SSR — one `RenderCommand` stream), selects it by
   clicking the rendered thing, and edits it via GUI gestures
   (drag/drop/resize, not property-panel overlays — per the
   "inline editing is GUI-only" principle).
2. **One source, three projections.** Every widget is PRUI (tree) +
   PRSS (theme) + Luau (behaviour), authorable as one `.prui` file or
   three siblings, with zero migration cost between the two shapes
   (`prui-luau-fusion.md`). The shell itself is a Builder project built
   from the same primitives users compose with
   (`composable-builder-plan.md` §1).
3. **Fullstack data layer.** Loro CRDT is the source of truth. A user
   opens a folder on disk; files auto-ingest as `GraphObject`s; a
   watcher keeps the object graph live; the Explorer panel browses it;
   blocks bind reactively to that data two-way (`project-vault.md`,
   `data-template-system.md`).
4. **Real dev environment.** Project tree, symbol index,
   jump-to-definition, diagnostics, find-in-files, CRDT inspector,
   presence overlay — the in-shell editor is a real IDE, not a
   textarea (`ide-mode-plan.md`).
5. **Deploy + collaborate.** Rust→WASM web / Rust→native desktop &
   mobile, packaged and self-updating; Loro-backed multiplayer with
   live cursors/selections and federated peer signals.

The acceptance bar for "done": a non-programmer opens Prism, points it
at a folder, drags blocks onto a canvas, binds them to their data by
clicking, scripts an exception in Luau, sees diagnostics inline, and
ships to web + desktop — all without leaving the shell.

## 1. Where the codebase is today (per capability)

| Capability | Status | Evidence |
|---|---|---|
| Render substrate | ✅ Solid | Single Taffy layout backend, retained `Surface`, three render backends off one `RenderCommand` stream. Slint fully exorcised. |
| Reactive spine | ✅ Solid | `Signal<T>` unifies local/IPC/federated/SSR; all 11 `dioxus-inspiration.md` phases landed incl. selective re-walk + reactive `prism.state`. |
| Component model | ✅ Solid | One `ComponentRegistry`; 17 starter blocks + ~48 primitives + core widgets via `CoreWidgetBlock`; dashboard widgets unified in (no parallel `WidgetRegistry`). |
| One-source/three-projection authoring | ✅ Runtime-complete | `prui-luau-fusion.md` Waves A–I; PRUI/PRSS/Luau fusion ships; hot-reload for `.prui` + `.prss` (incl. per-class PRSS invalidation). |
| Visual canvas authoring | 🟢 Runtime-complete | Inspector tree drives selection; the §4.3 `FacetDef` retirement landed, so facet inline templates are now ordinary `node.children` the normal canvas hit-test / click / inspector path already selects (no composite-id route needed — the gap dissolved with the parallel surface). **Residual:** mapping a *data-bound repeat instance* (`{facet}::{idx}/…`) back to its template source for edit-propagation is a normal-node UX refinement, not a substrate gap. |
| Two-way data binding | 🟢 Runtime-complete | `bind:*` carries through to `data-bind-*` semantic attrs; `SkeletonBindingContext` installs a real `Effect` per slot/selector binding + `refresh` drives the reactive graph (A2 install side) **and** `apply_to_ast` projects the per-node `ReactiveProps` bag back into the skeleton with subscribing reads (A2 read side, wired into `Shell::render` behind `attach_skeleton_bindings`). **Gap:** host refresh cadence — feeding the AppState→JSON snapshot each frame (pairs with the Project Vault data loop). |
| Fullstack data layer | 🟢 Runtime-complete | Project Vault V1–V3 all landed (verified in `project_manager.rs` 2026-05-17): `notify` watcher → live graph, Explorer (`state.catalog.files`), folder hierarchy + image thumbnails, `Shell::{open,close,save,poll}_project` + `--project`. **Residual:** binding-layer consumption of those `GraphObject`s into facet `items` / search (the data is live + queryable; the read-path is the binding lane's call). |
| Real dev environment | 🟡 Early | IDE Mode Phase 1 (project tree) + Phase 2 (symbol index + Ctrl+T Go-to-Symbol palette + jump-to-def) + Phase 4 (Inspector/DevTools) shipped. **Gap:** Phases 3/5/6/7 — diagnostics panel, folding/inlays, find-in-files, split/persistence. |
| Deploy | 🟡 Partial | web (wasm-bindgen) + native build paths ship. **Gap:** Phase 6 — mobile + `cargo-packager`/`self_update` packaging. |
| Collaborate | 🟡 Substrate-only | Loro CRDT + `PresenceManager` exist; probe firing + presence ingest stubbed (no host event-router / `PresenceService` consumer yet). |

The honest summary: **the engine and authoring substrate are done; the
product surface (canvas WYSIWYG, the data loop, the dev environment,
packaging) is the remaining work.**

## 2. The gap, concretely

### 2.1 Canvas WYSIWYG (closed by the §4.3 landing)
**Resolved.** This gap was an artefact of the old `FacetDef`
side-table: a facet's template lived off-tree, so a rendered template
descendant had no real canvas node to hit. The §4.3 retirement landed
— `FacetComponent::lower_ui` now lowers the facet's own
`node.children` (the template) as ordinary nodes (`facet/render.rs`),
so the normal one-seam pointer pipeline (`shell.rs` hit →
`events.rs::POINTER_ROUTES`) selects them with zero special-casing.
No composite-id hit-test branch is needed; the parallel
inline-template surface that motivated it is deleted. The only
remaining nuance — surfacing a *data-bound* repeat instance
(`{facet}::{idx}/…`) back to its single template source so an edit
propagates to every repeat — is a normal-node UX refinement tracked
under canvas polish, not a substrate gap.

### 2.2 The fullstack data loop
`project-vault.md` V2/V3 is the difference between "a UI builder" and a
"fullstack app builder." Disk folder → auto-ingest → live object graph
→ Explorer panel → two-way bound blocks. **V1–V3 all landed**
(`project_manager.rs`, verified 2026-05-17): `notify` watcher, live
graph, Explorer files, folder hierarchy + thumbnails. What remains is
the *binding-layer* consumption of those `GraphObject`s into facet
`items` / search — the data is live + queryable; pairs with the
`bind:*` install path (§2.3). Data with no binding is inert.

### 2.3 `bind:*` install path (A2)
Authors *declare* two-way bindings; the runtime carries them to
`data-bind-*`; `SkeletonBindings::collect` lists them and
`SkeletonBindingContext::install` now registers a real `Effect` per
slot/selector binding against a host-supplied AppState snapshot, with
`refresh` pushing fresh values through the source-signal graph
(literal sources one-shot, missing selectors recorded `unresolved`) —
mirroring `prism_builder::DocumentBindings`. The read side landed:
`SkeletonBindingContext::apply_to_ast` projects the per-node
`ReactiveProps` bag back into the composed skeleton AST as `Bare`
attributes via *subscribing* `signal(key)` reads, so the existing
`fill_compositions → lower_document_with_scope` pipeline renders the
bound value with zero block changes. `Shell::render` runs it inside
`RenderScope::run_in_render_pass` (so a `refresh` wakes the next
frame), gated behind `Shell::attach_skeleton_bindings` —
`None` is byte-identical to the pre-seam path. This is the skeleton
mirror of `DocumentBindings` Phase 4a/4b, at the AST-attribute seam
(the skeleton lowers through the resolver, not per-block `lower_ui`).
**Remaining:** host refresh cadence — producing the AppState→JSON
snapshot each frame; that pairs with the Project Vault data loop
(§2.2) and is deliberately the data-lane's call, not this seam's.

### 2.4 Dev environment depth
Today's editor is Phase 1+2+4. Symbol index + jump-to-definition
**landed** (`prism_core::language::symbol_index` + the Ctrl+T
`shell.symbol-palette`; `editor_files::open_at_offset` is the shared
jump seam). What remains: nested/drag project tree (Phase 2's tree
refinement — the flat depth-encoded explorer ships), find-in-files,
and the diagnostics panel. Phase 3 (diagnostics) is blocked on
`luau-analyze` integration (landed as a CI gate; needs the in-shell
surface) and an open femtovg question (no wavy-underline primitive).

### 2.5 Maintainability tax on velocity — ✅ largely retired
The former rate limiter was `prism-shell/src/state.rs` at 7.5k lines.
**Landed 2026-05-17:** decomposed into a `state/` module dir along
slot/divider seams — `mod.rs` (1168, `AppState` + `impl AppState`),
`inspector.rs` (358), `slots_core.rs` (811), `overlay.rs` (453),
`slots_doc.rs` (800), `canvas/mod.rs` (1082), `canvas/parts.rs`
(741). Every **production** module is now ≤ ~1.2k lines; only the
test-only `state/tests.rs` (2179) is over guideline and is an
optional follow-up (its size doesn't throttle feature velocity).
Behaviour-preserving: child modules `use super::*`; private items the
cross-module callers reach were widened to `pub(crate)`; `pub use`
keeps every `crate::state::X` / `crate::X` path stable. `ui_lower.rs`
/ `ui_resolver.rs` were already split (Phase B.5). The structural
maintainability blocker is cleared; what remains is incremental.

## 3. Phased path to the full vision

Ordered for dependency + payoff. Each phase ends test-green + clippy-clean.

**Phase A — Unblock the data loop.**
1. ~~Finish `bind:*` install path (A2): `data-bind-*` → registered
   `Effect`; render walk consumes the `ReactiveProps` bag.~~ Both
   sides landed — install (`SkeletonBindingContext`) + read
   (`apply_to_ast`, wired into `Shell::render` behind
   `attach_skeleton_bindings`). Remaining is host refresh cadence,
   folded into step 2. (§2.3)
2. ~~Project Vault V2: disk watcher + Explorer panel.~~ ✅ Landed
   (V1–V3 all done — `project_manager.rs` verified 2026-05-17:
   `notify` watcher, Explorer files, folder hierarchy + thumbnails,
   `Shell::{open,close,save,poll}_project` + `--project`). Residual
   is binding-layer consumption, the data lane's call. (§2.2)
3. ~~Canvas hit-test → facet-template composite-id selection.~~
   Dissolved by the §4.3 landing — facet templates are normal nodes
   the existing pointer pipeline already selects. (§2.1)

→ Acceptance: open a folder, see files as data, drag a list block,
bind it to a collection by clicking, edits round-trip. (Binding seam +
canvas selection done; gated only on Vault V2.)

**Phase B — Make the shell maintainable enough to move fast.**
4. ~~Decompose `prism-shell/src/state.rs`~~ ✅ Landed 2026-05-17.
   Done as a `state/` module dir (not `app/` — same effect, smaller
   blast radius): `mod.rs` + `inspector` + `slots_core` + `overlay`
   + `slots_doc` + `canvas/{mod,parts}` + `tests`. Every production
   module ≤ ~1.2k lines; behaviour-preserving (child `use super::*`,
   private→`pub(crate)` widening, `pub use` path stability); all 434
   `prism-shell` lib tests green, clippy clean, workspace check
   clean. Residual: `state/tests.rs` (2179) test-only split.
5. ~~Split `ui_lower.rs` / `ui_resolver.rs` along their natural
   seams.~~ ✅ Landed 2026-05-17. `ui_resolver.rs` (1465) →
   `ui_resolver/{mod,convert}.rs` (985 / 508) along the tag-dispatch
   vs. element→builder-node seam; `ui_lower.rs` (1794) →
   `ui_lower/{mod,nodes}.rs` (~1245 / 491) splitting the
   container/text/image/input helper cluster off `LowerCtx`. Public
   `crate::ui_*::*` paths preserved via `pub use`; behaviour-
   preserving, all `prism-builder` tests green + clippy clean.

→ Acceptance: no single production module > ~1500 lines — **met**
across `prism-builder` (Phase B.5) *and* `prism-shell` (item 4, the
`state/` split); tests green throughout — **met** (434 shell + all
builder, clippy + workspace clean). Phase B is effectively closed;
the lone residual is the test-only `state/tests.rs` split.

**Phase C — Dev environment to fullstack-credible.**
6. IDE Phase 2: real project tree (folders, rename, drag). *(tree
   nesting + drag still open; the flat depth-encoded explorer ships.)*
7. IDE Phase 3: diagnostics panel (resolve the femtovg
   underline question — gutter markers vs. squiggle primitive).
8. ~~IDE symbol index + jump-to-definition.~~ ✅ Landed 2026-05-17.
   `prism_core::language::symbol_index` + `AppState::index` (rebuilt
   on save / project-open) + `editor.go-to-symbol` (Ctrl+T)
   `shell.symbol-palette` (fuzzy, Enter/click jump). **Residual:**
   find-in-files (Phase 6 below) + Ctrl+click in the editor body.

→ Acceptance: jump to a symbol by name from the Ctrl+T palette —
**met**. Author a Luau error → see it inline + in a panel (Phase 3,
femtovg-underline-blocked), find all refs (find-in-files) — open.

**Phase D — Deploy + collaborate.**
9. ~~Project Vault V3 (folder hierarchy + thumbnails).~~ ✅ Landed
   (shipped with V2 above — one `project_manager.rs` pass).
10. Phase 6: mobile target + `cargo-packager` + `self_update`.
11. Wire presence: host event-router → `PresenceService` consuming
    probes; live cursors/selections on the canvas.

→ Acceptance: package a signed desktop build that self-updates; two
users co-edit a page with live cursors.

## 4. Authoring-systems survey & consolidation decisions

A 3-pass evidence survey (runtime DSL path, builder-side model with
real call-site counts, documented intent) of every authoring system,
to decide firmly what we keep, unify, or cut. Status point-in-time
2026-05-16; verify against code before acting.

### 4.1 The "facets" disambiguation

"Facets" is **two unrelated systems sharing a name**:

1. **PRUI `<facet>` element** (`prism-ui-runtime::interpret`, ~8 LOC) —
   a thin sugar wrapper over `resolve_for_iteration`, the exact engine
   `for=` uses. Plus the `fct:` attribute namespace
   (`prism-core::AttributeNamespace::Facet`). **KEEP** — this is the
   canonical data-repeat mechanism, 36 live `for=` uses, full runtime.
2. **`prism-builder::FacetDef` subsystem** (`facet/` dir, ~1447 LOC +
   86 tests) — a separate document-model data construct. **RETIRE**
   (see §4.3).

These are not the same thing. The decision below retires #2 only and
explicitly preserves #1.

### 4.2 Keep / unify / cut table

| System | Call | Rationale |
|---|---|---|
| PRUI / PRSS / Luau (3 projections) | **KEEP** | The authoring surface; roles deliberately non-overlapping. |
| BuilderDocument / ComponentRegistry / Block | **KEEP** | Core spine; universal. |
| PRUI `<facet>` / `for=` / `bind:` / `fct:` | **KEEP** | One `resolve_for_iteration`, 36 live uses, full runtime. |
| Modifiers (+ `ModifierRegistry`) | **KEEP** | Live in every shell frame (`ui_lower.rs:401`). |
| TemplateNode / `CoreWidgetBlock` | **KEEP** | Live in relay SSR + the `PrismBlock` derive; ~13 core domains. |
| `dialects.rs` | **KEEP** | Live in the shell parse pipeline. |
| **`FacetDef` subsystem** | ✅ **CUT (landed)** | Done — see §4.3. `FacetComponent` has a real `lower_ui` over normal `node.children`; the side-table model + `state.rs` parallel surface are deleted. |
| **`prism_ui_emit.rs`** | ✅ **CUT (landed 2026-05-17)** | Vestigial Slint-era source emitter; consumers were only test-only `Page::ensure_source`/`regenerate_source` + self-tests. Module + hooks + re-exports removed; `SavedPage.source` stays as the persistence pass-through string. |
| **`SignalDef` / `Connection`** | **UNIFY** | Doc graph's one undecided convergence: compile `SignalDef`→`reactive::Signal<()>`, `Connection`→`Effect`. Finishing the `bind:*` install path (A2) *is* this at the attribute layer. |
| Prefabs | **REDUCE** | Doc-demoted to internal-only; sole live use powers the `card` builtin + facet-promote. Fold `card` into a SpecBlock; keep `PrefabDef` only as hidden promotion mechanism. |
| Variants | **KEEP (plumbing)** | No independent authoring surface; core-widget variant-spec bridge only. |
| `widget-system.md`, `prui-reference.md` §4 | **DOC FIX** | Stale (reference Slint / mark landed items as pass-through). |

### 4.3 ✅ LANDED — retired `FacetDef`, re-based inline templates on normal nodes

**Status (verified against code 2026-05-17).** Done. `facet/` is now
three small files (`mod.rs` / `render.rs` / `resolve.rs`, ~330 LOC
total); `FacetComponent::lower_ui` lowers the facet's own
`node.children` once per `props.items` entry with `{{field}}`
interpolation via `resolve_template_expressions` and per-instance
`prefix_ids`. `BuilderDocument.facets` / `.facet_schemas`, the
non-`Inline` FacetKinds, schemas, aggregates, calc, variant-rules,
and the entire `state.rs` parallel inline-template surface
(`materialize_facet_templates`, `walk_facet_template`,
`parse_facet_template_id`, `FacetTemplateSelection`,
`select_facet_template`, `set_facet_template_prop`,
`derive_facet_template_property_rows`, `facet_template_rows`,
`FacetLookup`, the `::tpl/` composite-id scheme, the
`CanvasSlot.facet_template_selection` field, the `events.rs`
template-path write branch) are all gone. `resolve_template_expressions`
survives as the documented node-tree helper. The builtin-id assertion
tests in the three crates still list `facet` (it remains a registered
one-off `Component`). The original analysis below is retained for
provenance.

**Why.** `FacetComponent` (the only `Component` impl in `facet/`) has
**no `lower_ui`** — a `facet` block renders as an empty container;
nothing renders facet data in the live tree today. Zero `<facet>`/
`fct:` usages in any shipping skeleton/app vs. 36 live `for=` uses.
The non-`Inline` FacetKinds (Query/Script/Aggregate/Lookup), schemas,
aggregates, calc, variant-rules have **zero shipping use and no render
path** — pure dead code. The only shipped surface is
`FacetTemplate::Inline` + the property-panel editing UI.

**Locked replacement design.** A facet today is two disjoint things
glued by a string id: a `Node{component:"facet"}` whose `children` are
ignored, plus a side-table `FacetDef` whose `FacetTemplate::Inline.root`
holds the real subtree (canvas-only `materialize_facet_templates`
clones it in). The migration **collapses the inline template into the
facet `Node.children`** so the template is ordinary tree data:

- The facet block keeps its template **as real `Node.children`**,
  edited/selected/clicked through the *normal* node paths — deleting
  the entire parallel inline-template-editing surface in
  `prism-shell/src/state.rs` (`materialize_facet_templates`,
  `walk_facet_template`, `parse_facet_template_id`,
  `FacetTemplateSelection`, `select_facet_template`,
  `set_facet_template_prop`, `derive_facet_template_property_rows`,
  `facet_template_rows`, `FacetLookup`, the `::tpl/` composite-id
  scheme, the `CanvasSlot.facet_template_selection` field, the
  `events.rs` template-path write branch).
- The block gains a **real `lower_ui`**: resolve a data source
  (minimal — static items / resource ref, mirroring PRUI), then lower
  the child subtree once per item with `{{field}}` interpolation
  (`resolve_template_expressions` survives as a node-tree helper).
- The heavy `FacetDef` model + 86 isolated-helper tests are deleted.
  `BuilderDocument.facets` / `.facet_schemas` side-tables removed;
  any non-`Inline` data kinds dropped (no shipping use, no render).
- **Out of scope / preserved:** the PRUI `<facet>`/`for=`/`fct:`
  system (§4.1 #1) and `prism-core::AttributeNamespace::Facet`. The
  stale `fct:` "lowered to FacetDef" comment is corrected to the
  plain-repeater reality.

**Risk register** (from the touchpoint map): serde on-disk compat
(`facets`/`facet_schemas` are persisted `#[serde(default)]` fields —
no shipping data exists, so practical loss is moot, but
`project.rs`/back-compat tests change); the render path is *net-new*
behavior, not a 1:1 port; builtin-id assertion tests in 3 crates
(`starter.rs`, `relay/state.rs`, `shell/components/registry.rs`) fail
the instant registration changes — update in lockstep; the pointer-hit
one-seam invariant must hold (template descendants must be real
canvas nodes the normal tagging covers); `promote_inline_to_component`
+ `FacetBinding` coupling resolved by keeping `{{field}}` helpers as
node-tree utilities.

### 4.4 Other firm calls

- ✅ **`bind:*` install + read path (A2)** — landed (§2.3).
  `SkeletonBindingContext` install + `apply_to_ast` read seam mirror
  `DocumentBindings` Phase 4a/4b. The deeper `SignalDef` →
  `reactive::Signal`/`Effect` *attribute-layer* convergence is
  satisfied at the binding layer; the connection-graph compile is the
  only residual and stays tracked.
- ✅ **Cut `prism_ui_emit.rs`** — landed 2026-05-17. Module + the
  `Page::ensure_source`/`regenerate_source` hooks deleted (only
  test-only consumers); `SavedPage.source` stays as the persistence
  pass-through string.
- ✅ **Reduce Prefabs** — landed 2026-05-17. `card` folded into a
  declarative `BlockSpec` (`schemas::CardProps` + `card_lower` + the
  `CARD` row in `BUILTINS`, same `"card"` id so the builtin-id
  assertion tests in all three crates stay green). Deleted
  `card_prefab_def` / `builtin_prefab` / `materialize_prefab` + their
  re-exports; the shell palette path now drops `card` as a vanilla
  `Node` like every other block. `PrefabDef` / `PrefabComponent` /
  `ExposedSlot` remain solely as the hidden user/promotion mechanism
  (`BuilderDocument.prefabs`). Unblocked once the `state.rs`
  decomposition (Phase B.4) isolated the consumer into
  `state/canvas/parts.rs` — no longer a cross-lane edit.

## 5. Cross-references

- Cleanup/migration reconciliation: `docs/dev/state-of-prism.md`
- Substrate roadmap (Tier 1–3): `docs/dev/prism-cross-cutting-systems.md`
- Live UI punch list: `docs/dev/ui-migration-followups.md`
- IDE surface: `docs/dev/ide-mode-plan.md`
- Authoring vision: `docs/dev/prui-luau-fusion.md`,
  `docs/dev/composable-builder-plan.md`
- Data/backend story: `docs/dev/project-vault.md`,
  `docs/dev/data-template-system.md`
- Locked DSL/renderer decision: `docs/adr/008-clay-prism-ui-dsl.md`
