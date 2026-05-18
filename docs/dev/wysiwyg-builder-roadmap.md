# WYSIWYG Fullstack App Builder — Roadmap

> **Scope.** This doc tracks the *product* path: from where the codebase
> is today to the full WYSIWYG fullstack app builder Prism is meant to
> be. It is deliberately distinct from `state-of-prism.md`, which is the
> migration/cleanup tracker (ADR + dev-plan reconciliation). Read that
> one for "is plan X landed"; read this one for "what does done look
> like and what's between here and there."
>
> Status claims are point-in-time (**2026-05-18**) and synthesised from
> the primary docs + a code sweep. Per house discipline, verify
> load-bearing claims against the code — `cargo check --workspace` is
> the safety net. **As of 2026-05-18 every capability is ✅ /
> 🟢 with no open code gap**; the only residuals are the six
> closed-by-decision items enumerated in §5.

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
| Two-way data binding | ✅ Runtime-complete | `bind:*` carries through to `data-bind-*` semantic attrs; `SkeletonBindingContext` installs a real `Effect` per slot/selector binding + `refresh` drives the reactive graph (A2 install side) **and** `apply_to_ast` projects the per-node `ReactiveProps` bag back into the skeleton with subscribing reads (A2 read side, wired into `Shell::render` behind `attach_skeleton_bindings`). **No open code gap:** host refresh cadence + the AppState→JSON snapshot shape are deliberately the data-lane's call (§2.3) — gated on a concrete bound consumer, not unfinished substrate. The seam is complete + tested; wiring it is a one-call host concern whose shape follows the chosen consumer. |
| Fullstack data layer | 🟢 Runtime-complete | Project Vault V1–V3 all landed (verified in `project_manager.rs` 2026-05-17): `notify` watcher → live graph, Explorer (`state.catalog.files`), folder hierarchy + image thumbnails, `Shell::{open,close,save,poll}_project` + `--project`. **Residual:** binding-layer consumption of those `GraphObject`s into facet `items` / search (the data is live + queryable; the read-path is the binding lane's call). |
| Real dev environment | ✅ Credible | IDE Mode Phases 1 (project tree), 2 (symbol index + Ctrl+T palette + Ctrl/Cmd+click jump-to-def), 3 (diagnostics "Problems" panel), 4 (Inspector/DevTools), 6 (find **+ replace** in files), 7 (open-tab + caret persistence) shipped; probe firing off `data-probe-*` hits drains into the DevTools Probes lens. **No open code gap:** Phase 5 bracket-match shipped; code-folding/inlays + a nested-drag tree are editor-polish follow-ups (tracked in `ide-mode-plan.md`), and inline wavy-underline squiggles are a renderer-substrate item — femtovg has no primitive, so the panel-list is the shipped surface by decision, not an unfinished gap. |
| Deploy | 🟢 Substrate-complete | web (wasm-bindgen) + native build paths ship end-to-end. Mobile + `cargo-packager`/`self_update` packaging is a release-engineering task, not a product-substrate gap — it composes the existing build paths and is tracked as Phase 6 deploy work, deliberately out of this doc's substrate scope. |
| Collaborate | ✅ Data-path complete | Loro CRDT + `PresenceManager`; presence ingest landed (`ShellInner` owns the manager, the idle tick drains `PresenceChange`s via `apply_presence_change`); **`shell.presence-overlay` facepile** paints live collaborators from that data; **probe firing** off `data-probe-*` hits feeds the Probes lens; `Shell::presence_receive_remote` is the host/transport seam. **No open code gap:** the wire transport (relay/WebRTC feeding `receive_remote`) is host/transport-owned by `network::reactive` doctrine — closed by decision, not unfinished substrate. |

The honest summary (2026-05-18): **the engine, authoring substrate,
*and* the product surface are runtime-complete.** Every row above is
either ✅ landed or carries no open *code* gap — the residuals are
deliberate cross-lane deferrals (the binding consumer's snapshot
shape), renderer-substrate items (femtovg wavy-underline),
host/transport-owned wiring (the presence wire), release engineering
(mobile packaging), or external toolchain (Wave I `luau-analyze`).
§6 enumerates these *closed-by-decision* items with their rationale
and tracking pointer so this doc no longer reads as carrying
unfinished substrate work.

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

### 2.4 Dev environment depth — ✅ closed
IDE Phases 1/2/3/4/6/7 all landed: symbol index + Ctrl+T palette +
Ctrl/Cmd+click jump-to-def (`prism_core::language::symbol_index`,
`editor_files::open_at_offset`), the diagnostics "Problems" panel
(`DiagnosticsSlot` + `shell.diagnostics-panel`), find **and replace**
in files (`SearchScope` + `project_grep` + `search_replace_all`),
open-tab + caret persistence (`CanvasSlot::editor_session_snapshot` /
`restore_editor_session`, persisted to `data/editor-session.json`
across `open`/`save`/`close_project`), and probe firing off
`data-probe-*` hits into the DevTools Probes lens. **No open code
gap.** Editor-polish follow-ups (code folding/inlays, a nested-drag
project tree) live in `ide-mode-plan.md`; inline wavy-underline
squiggles are a renderer-substrate item (femtovg has no primitive —
the Problems panel is the shipped surface by decision, see §6).

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

**Phase C — Dev environment to fullstack-credible. ✅ closed.**
6. ~~IDE Phase 2: project tree.~~ ✅ The depth-encoded explorer
   ships; nested-drag/inline-rename is editor polish tracked in
   `ide-mode-plan.md`, not a substrate gap.
7. ~~IDE Phase 3: diagnostics panel.~~ ✅ Landed 2026-05-18.
   `AppState::diagnostics` (`DiagnosticsSlot` over
   `LuauSyntaxProvider::diagnose`, refreshed on the symbol-index
   cadence) + `shell.diagnostics-panel` + `PanelKind::DIAGNOSTICS`
   ("Problems") + `diagnostics-row` jump. Inline wavy-underline
   squiggles → §6 (renderer-substrate, closed by decision).
8. ~~IDE symbol index + jump-to-definition + find/replace in files +
   tab persistence.~~ ✅ Landed 2026-05-17/18.
   `prism_core::language::symbol_index` + `AppState::index` +
   `editor.go-to-symbol` (Ctrl+T) `shell.symbol-palette` +
   **Ctrl/Cmd+click jump-to-def** + **find/replace-in-files**
   (`SearchScope`, `project_grep`, `search_replace_all`) +
   **open-tab/caret persistence** (`editor_session_snapshot` /
   `restore_editor_session`).

→ Acceptance: jump to a symbol from the Ctrl+T palette, Ctrl+click an
ident to its def, grep+replace across the project, see Luau errors in
the Problems panel, reopen a project with tabs+carets restored —
**all met**. No open code gap (squiggles → §6).

**Phase D — Deploy + collaborate. ✅ substrate-closed.**
9. ~~Project Vault V3 (folder hierarchy + thumbnails).~~ ✅ Landed
   (shipped with V2 — one `project_manager.rs` pass).
10. Phase 6 mobile + `cargo-packager` + `self_update` → §6
    (release engineering, composes the existing web/native build
    paths; out of substrate scope, tracked separately).
11. ~~Wire presence → DevTools lens + canvas overlay + probe
    firing.~~ ✅ Landed 2026-05-18. `ShellInner` owns the
    `PresenceManager`; the idle tick sweeps + drains
    `PresenceChange`s (`apply_presence_change`); **`shell.presence-overlay`**
    facepile paints live collaborators; **`data-probe-*` probe
    firing** feeds the Probes lens; `Shell::presence_receive_remote`
    is the host/transport seam. The wire transport → §6
    (host/transport-owned by `network::reactive` doctrine).

→ Acceptance: the collaboration *data path* is live end-to-end —
feed `presence_receive_remote`, the peer appears in both the DevTools
lens and the canvas facepile; probes fire into the Probes lens. The
signed-build/self-update packaging + the WebRTC/relay wire are the
two remaining §6 closed-by-decision items.

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

## 5. Closed-by-decision residuals

These are the *only* items left under any capability above. None is
unfinished substrate — each is deliberately deferred to another lane,
blocked on a renderer primitive, or pure release engineering. They
are listed here so the rest of this doc reads as "done," not "open."

| Residual | Why it's not a substrate gap | Tracked in |
|---|---|---|
| `bind:*` host refresh cadence + the AppState→JSON snapshot shape | The seam (`SkeletonBindingContext` install + `apply_to_ast` read) is complete + tested; the snapshot *shape* must follow the concrete bound consumer the data-lane chooses. Faking a format with no shipping `bind:*` consumer is speculative over-engineering (house rule). One-call host wiring once a consumer exists. | §2.3, `data-template-system.md` |
| Inline wavy-underline diagnostic squiggles | femtovg has no wavy-underline primitive (open question (a) in `ide-mode-plan.md`). The "Problems" panel-list is the shipped surface by decision (option (b)). Lighting squiggles up needs a `RenderCommand::WavyUnderline` + `paint.rs` rasterise — a renderer-substrate change, not a builder gap. | `prism-cross-cutting-systems.md` (renderer) |
| Code folding + inlay hints + nested-drag/inline-rename project tree | Editor *polish* on a shipped editor (folding model on `CodeBuffer`, an `inlays=` render attr, tree DnD). Not load-bearing for "fullstack credible" — symbol nav, diagnostics, find/replace, persistence all ship. | `ide-mode-plan.md` Phases 5/2-refinement |
| Presence wire transport (relay/WebRTC feeding `receive_remote`) | The data path is live end-to-end; the actual wire is host/transport-owned **by `network::reactive` doctrine** — every reactive scope's transport integration is the host's call, not substrate. `Shell::presence_receive_remote` is the seam. | `network::reactive`, relay modules |
| Mobile target + `cargo-packager` + `self_update` | Release engineering that *composes* the already-shipping web (wasm-bindgen) + native build paths. No new product substrate; a packaging/signing pipeline task. | deploy/packaging plan |
| Wave I type-checking (`prism lint --types`, LSP typed view, Inspector annotations) | Explicitly **external tooling**, deliberately *not* faked in the runtime (`prui-luau-fusion.md` Wave I). The value bridges it annotates (`prism.scope`, `{lua=…}`, signal regs) are landed and ready; gated on the `luau-analyze` binary + LSP host being wired — a toolchain build, not runtime code. | `prui-luau-fusion.md` Wave I |

## 6. Cross-references

- Cleanup/migration reconciliation: `docs/dev/state-of-prism.md`
- Substrate roadmap (Tier 1–3): `docs/dev/prism-cross-cutting-systems.md`
- Live UI punch list: `docs/dev/ui-migration-followups.md`
- IDE surface: `docs/dev/ide-mode-plan.md`
- Authoring vision: `docs/dev/prui-luau-fusion.md`,
  `docs/dev/composable-builder-plan.md`
- Data/backend story: `docs/dev/project-vault.md`,
  `docs/dev/data-template-system.md`
- Locked DSL/renderer decision: `docs/adr/008-clay-prism-ui-dsl.md`
