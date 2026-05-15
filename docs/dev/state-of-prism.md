# State of Prism

> **Update 2026-05-15.** Several items below have since landed —
> dashboard widget unification, em-dash scanner fix, slint string
> cleanup, banner-heading of superseded docs, the `Block` vs
> `Component` doc, and the first slice of facet template visibility
> in the property panel. See the "What landed on 2026-05-14/15"
> section near the end. The original analysis is preserved.

A consolidated read of every ADR and dev-plan as of 2026-05-13. The goal is
one place to see (a) where each plan actually stands relative to the code,
(b) the cleanup and unification opportunities that fall out of cross-reading
them, and (c) a prioritised punch list of what's left.

This doc is a synthesis — every claim here points back to a primary doc in
`docs/dev/` or `docs/adr/`. Authoritative status still lives in those files;
this one rots faster than they do.

## 1. Where each plan actually stands

### ADRs (`docs/adr/`)

| # | Decision | Status |
|---|---|---|
| 001 | Loro CRDT over Yjs | ✅ implemented, `loro` wired through `prism-core` |
| 002 | `PrismFile` + `LanguageContribution` + eight-category `prism-core` split | ✅ structurally landed; the doc is intentionally immutable, so no "complete" flag |
| 003 | Three-layer layout (Page grid, `Transform2D`, `LayoutMode`) | ✅ shipped on top of Taffy |
| 004 | Five composition patterns (Resources, Modifiers, Variants, Signals, Prefabs) | ✅ all five exist as modules in `prism-builder/src/` |
| 005 | Dockable panels — `prism-dock` crate | ✅ crate exists, integrated into the shell |
| 006 | Live bidirectional Slint builder | ❌ **superseded by ADR-008** (Slint exorcised) |
| 007 | Slint 1.16 upgrade, interpreter scoped to builder | ❌ **superseded by ADR-008** |
| 008 | Replace Slint with Taffy + `prism-ui` DSL | ✅ delivered (Phase 5 cutover) |
| 009 | Per-app skeletons | ✅ each app ships its own `shell.prism-ui` |
| 010 | Service factories with `ServiceContext` | ✅ `add_factory_scoped` + `rebuild_app_services` shipping |

ADRs 006 and 007 are intentionally retained as history — they document the
path that led to ADR-008. They should never be the basis for a code search;
flag in their own headers would help (see §3 cleanup).

### Dev plans (`docs/dev/`)

| Plan | Headline status | Notable remaining work |
|---|---|---|
| `clay-migration-plan.md` | UI cutover done; 6k-line plan is now mostly historical | A2 (bind:), A3 (token interpolation), A4 (`fct:*`/`sig:*` lowering), A5 (UTF-8 comment-scanner bug), B3 (web frame loop), C3 (`.prism-ui` hot-reload), D4 (per-app shell file selection) |
| `composable-builder-plan.md` | 13 of 21 micro-items shipped | Test fixture helpers, `ObjectSnapshot` large-variant boxing, `prism-shell/src/app/` decomposition, `#[visual_node]` migration |
| `dioxus-inspiration.md` | **All 11 phases ✅ (2026-05-11).** `Signal<T>` is the unified primitive across local/IPC/federated/SSR | None — this plan delivered end-to-end |
| `declarative-refactorings.md` | 13 ✅ / 3 🟡 / 5 ⬜ | Same residual items as composable-builder-plan; cleanup not architecture |
| `luau-integration-plan.md` | Phases 1–2 done; macro + `PrismContext` shipping | **Phase 3** (reactive subscriptions: `atoms:watch`, `store:select`, `crdt:on_sync`), Phase 4.4–4.6 (computed fields, widget templates, build-step registration), Phase 5 deeper builder annotations, Phase 6e (shell auto-load via `Shell::open_project`), Phase 7 (manifest scripting, capability scopes) |
| `ui-migration-followups.md` | First + second waves landed | Same A2–A5/B2–B3/C–D items as `clay-migration-plan.md`; D2 (real `mlua`-backed `LuauHost` in shell — currently `NoopLuauHost`) |
| `slint-migration-plan.md` | Obsolete; preserved as history | Should carry a banner header; see §3 |
| `flux-port-gap-analysis.md` | Tier 1 + 2 ✅, Tier 4 ~78%, Tier 3 / Tiers 7–8 ⬜ | Spreadsheet view + chart components (Tier 4 stragglers); 9 messaging providers (Tier 3); OCR/PDF/integrations (Tiers 7–8 — explicitly low priority) |
| `widget-system.md` | Spec — fully designed, mostly implemented | None explicit; see §2.1 below for actual code drift |
| `facets.md` | Spec — phases 1–13 ✅ | None explicit |
| `prui-reference.md` | Spec | Structural diff on the consumer half (producer side done) |
| `prss-reference.md` | Spec | Animations / `@keyframes` deferred (waits on the `transition:` namespace animator) |
| `builder-unification.md` | Superseded by Phase 5 cutover | Should be archived or banner-headed |
| `dsl-self-bootstrap.md` | All four loops + persistent-Luau wave landed | None explicit |
| `data-template-system.md` | Phases 1–5 ✅ | The shell property panel's inline-template editing path is the gap that this doc *describes* but doesn't enforce — see §2.3 |
| `project-vault.md` | V1 in flight | V2 (watcher + Explorer panel) and V3 (folder hierarchy + thumbnails) |

## 2. Unification opportunities (the meat of this doc)

Code is in a much better place than the doc graph would suggest. The
remaining duplication is concentrated in a few places.

### 2.1 The widget system is two systems in a trench coat

This is the one the user flagged, and it's real.

**System A — the unified one.** `WidgetContribution`
(`prism-core/src/widget/contribution.rs`) is a declarative engine-authored
block (calendar, timekeeping, ledger, etc.). It gets wrapped by
`CoreWidgetBlock` (`prism-builder/src/core_widget.rs`), which implements
`Block`, which means it lands in `ComponentRegistry` like any other block.
`FieldSpec` is the shared property vocabulary. This part of the system
already converged on `widget-system.md`'s vision.

**System B — the parallel one.** The dashboard subsystem
(`prism-core/src/interaction/dashboard/`) has its own `WidgetDef` type
(`types.rs:9`), its own `WidgetRegistry` (`controller.rs:595`), and its
own hardcoded `built_in_widgets()` table (`controller.rs:303` —
13 widgets, all spelled out as `WidgetDef { … }` literals). It carries
`Vec<FieldSpec>` for properties, so the vocabulary matches, but the
registry is fully parallel to `ComponentRegistry`. The dashboard
controller looks up widget metadata in System B, not System A.

**Cost.** Adding a dashboard widget requires editing
`built_in_widgets()`. Adding a builder block requires registering with
`ComponentRegistry`. The same conceptual entity (a widget) lives in two
places, with no shared registration path. New "views are builder
components" code patterns (per project memory) do not apply to dashboard
widgets today.

**Fix.** Make dashboard widgets first-class blocks:
1. Move each entry from `built_in_widgets()` into a `BlockSpec`
   registered through `ComponentRegistry` (probably alongside the 17 in
   `prism-builder/src/starter.rs`).
2. Drop `dashboard::WidgetRegistry`; have `DashboardController` query
   `ComponentRegistry.get(id).schema()` instead.
3. Keep `WidgetSlot` (it's a layout concept — grid cell + span — not a
   widget definition).

Estimated 2–3 days. Payoff is "one registry, one schema vocabulary,
one place to add a widget."

### 2.2 Block / Component / BlockSpec naming drift

There's no architectural duplication here, but the terminology is split:

- Rust side: `Block` trait, `BlockSpec`, `register_block`, blanket
  `impl<T: Block> Component`. New blocks use `Block`; the registry
  abstracts over `Component`.
- DSL / docs side: `<component name="…">`, `ComponentRegistry`,
  "component schema".

That's fine — `Component` is the interface, `Block` is the trait-impl
sugar — but it's not written down anywhere a reader would find it.
A one-line `block.rs` module doc clarifying the relationship would save
every future reader the trip.

### 2.3 Inline-template editing is designed but unwired in the shell

`data-template-system.md` describes facets-as-inline-templates as the
core reuse pattern. The data side landed:
- `FacetTemplate::Inline` exists in `prism-builder/src/facet/`.
- `promote_inline_to_component()` extracts `{{field}}` expressions into
  exposed slots when an inline template gets promoted.

The shell side did not:
- `prism-shell/src/panels/properties.rs::facet_rows()` has no
  special-case for `FacetTemplate::Inline`. There is no visual
  template-editing surface on the canvas.

This is the largest "described-but-not-built" gap in the docs.
Per project memory "Inline editing is GUI-only", which is fine — but the
GUI for it is missing. Closing this is ~3–5 days and finally honours
the data-template plan end-to-end.

### 2.4 Five lowering call sites, one logical pipeline

`lower_ui` is invoked from at least five places:
- main flow (`prism-builder/src/ui_lower.rs::LowerCtx::lower`)
- template lowering (`prism-builder/src/template_lower.rs`)
- facet template lowering (`prism-builder/src/facet/`)
- Luau components (`prism-builder/src/luau_component.rs`, including the
  embedded `VirtualNode → UiNode` translation)
- SSR (`lower_semantic_html` in the runtime)

They all bottom out in the same `Component::lower_ui` contract, so this
is not a correctness duplication. The opportunity is to extract the
`VirtualNode → UiNode` translation out of `luau_component.rs` as a
standalone helper — it's the natural adapter for any future scripting
runtime that emits a tree (and it's an obvious extension point for the
Phase 3 Luau reactive subscriptions). ~1 day, low risk.

### 2.5 What is already well-unified (don't fix)

- **Layout.** Single Taffy backend, threaded through `LayoutMode`. No
  parallel paths.
- **Styling cascade.** Tokens → PRSS classes → inline `style:` is a
  three-layer cascade with clear precedence, not duplication.
- **Render backends.** `femtovg` (native), `wasm-bindgen` web, and
  `lower_semantic_html` (SSR) all consume `RenderCommand` / the
  retained `Surface`. One source of truth.
- **Reactive primitives.** `Signal<T>` from
  `dioxus-inspiration.md` unifies local state, daemon IPC,
  federated/peer signals, and SSR caching. That migration is done.
- **Service registration.** ADR-010's `ServiceFactory` +
  `ServiceContext` is the single path; `add_factory_scoped` +
  `rebuild_app_services` are in production.

## 3. Cleanup punch list

Sorted high-impact to cosmetic.

### High impact

- **Unify dashboard widgets into `ComponentRegistry`** (§2.1).
- **Wire inline-template editing into the shell property panel** (§2.3).
- **Replace `NoopLuauHost` in the shell with a real `mlua`-backed
  host** (`ui-migration-followups.md` D2). The daemon already has
  persistent Luau; the shell stub is the last seam holding back the
  Phase 3 reactive subscriptions and the manifest-scripted apps.
- **Fix the UTF-8 em-dash bug in the `.prism-ui` comment scanner**
  (clay-migration A5). It's a one-line panic that bites any author
  who copies prose into a comment.

### Medium

- **Land the missing PRUI features**: `bind:*` two-way binding (A2),
  `{tokens.colors.surface}` interpolation in `style:` (A3),
  `fct:*` / `sig:*` lowering (A4). All four are scoped in
  `ui-migration-followups.md`.
- **Web backend frame loop** (B3) — currently renders one frame, no
  `requestAnimationFrame` loop.
- **Hot-reload of `.prism-ui` source** (C3) — anchor is in place from
  the Dioxus phase-9 work; just needs the per-frame integration.
- **Decompose `packages/prism-shell/src/app/`** — last remaining item
  from `declarative-refactorings.md` and one of the biggest god-modules
  still in the tree.
- **Extract `VirtualNode → UiNode` translation** (§2.4) — sets up the
  Luau Phase 3 work.

### Cosmetic but worth doing

- **Rename `packages/prism-studio/src-tauri/`.** It still exists. The
  name is a known historical artefact (CLAUDE.md says so). One commit
  to rename, update Cargo paths, and remove the explanation from
  CLAUDE.md.
- **Banner headers on superseded docs.** Add a one-line "⚠️ Superseded
  by …" banner at the top of `docs/adr/006-…`, `docs/adr/007-…`,
  `docs/dev/slint-migration-plan.md`, and `docs/dev/builder-unification.md`.
  Today a reader has to know the project history to realise these are
  historical.
- **Slint string residue.** Two test fixtures reference `"slint"` as a
  language tag (`prism-shell/src/state.rs`, `prism-builder/src/schemas.rs`
  docstring). Rename to `"prui"`. The remaining grep hits in
  `prism-core/src/foundation/mod.rs` and `prism-daemon/` are doc
  references to the historical plan and harmless.
- **Document the `Block` vs `Component` relationship** in
  `packages/prism-builder/src/block.rs` (§2.2).
- **Archive `clay-migration-plan.md`'s closed phases.** At 6286 lines
  the doc is now mostly history; the live punch list is `ui-migration-followups.md`.
  Either split the closed phases into a `*-archive.md` or add a
  table-of-contents that surfaces the open A/B/C/D items at the top.

## 4. Suggested ordering

If we tackled this end-to-end, the order I'd argue for:

1. Real `mlua`-backed `LuauHost` in the shell — unblocks Luau Phase 3
   (reactive subscriptions), Phase 6e (shell script auto-load), and the
   inline-template editing surface (which wants Luau-authored widgets
   to be editable too).
2. Unify dashboard widgets into `ComponentRegistry`. Self-contained,
   high payoff, frees `built_in_widgets()`'s table to be the last
   parallel definition.
3. PRUI A2/A3/A4 batch (`bind:*`, token interpolation, `fct:*`/`sig:*`
   lowering). These three should land together because authors who
   reach for `bind:` also reach for `fct:` and `{tokens.…}`.
4. Inline-template editing in the property panel.
5. Web frame loop (B3) + `.prism-ui` hot reload (C3). Pairs naturally
   with the shell-side hot-reload anchor that's already installed.
6. Cleanup pass: rename `src-tauri/`, banner-head the superseded docs,
   archive the closed phases of `clay-migration-plan.md`, rename
   `"slint"` test fixtures.

## 5. Doc graph for reference

If you only read three docs after this one:
- `docs/dev/ui-migration-followups.md` — the live punch list for the
  UI runtime.
- `docs/dev/luau-integration-plan.md` — the live punch list for Luau.
- `docs/dev/data-template-system.md` — the data-side spec that the
  shell still needs to honour visually.

Superseded / historical (skip unless you're investigating *why*
something exists): ADR-006, ADR-007, `slint-migration-plan.md`,
`builder-unification.md`, and the closed phases of
`clay-migration-plan.md`.

## What landed on 2026-05-14/15

### Cleanups

- **Banner headers on superseded docs.** ADR-006, ADR-007,
  `slint-migration-plan.md`, and `builder-unification.md` now open
  with a "⚠️ Superseded by …" callout so readers don't have to know
  the project history to spot history.
- **Slint string residue.** The two `"slint"` language-tag fixtures
  (`prism-shell/src/state.rs` test code; `prism-builder/src/schemas.rs`
  docstring) renamed to `"prui"`.
- **`Block` vs `Component`.** Already documented at
  `prism-builder/src/block.rs:1–12`; no code change needed. State-of-
  Prism's "naming drift" section can be closed.
- **A5 — em-dash scanner bug.** Already fixed when audited —
  regression test
  `comment_with_non_ascii_content_round_trips` in
  `prism-core/src/language/prism_ui/grammar.rs` pins it.
  `ui-migration-followups.md` updated to mark A5 LANDED.

### Dashboard widget unification (§2.1 done)

The legacy parallel `WidgetDef` / `WidgetRegistry` / `built_in_widgets()`
system in `prism-core/src/interaction/dashboard/` was deleted. All
12 dashboard widgets are now declared as `WidgetContribution`s in
`controller.rs::widget_contributions()` with bare ids matching the
preset slot `widget_type` fields (`stats`, `tasks`, `timer`, …). They
flow through `prism-builder::CoreWidgetBlock` → `Block` →
`ComponentRegistry` like every other widget — one registry, one
schema vocabulary, one declaration per widget. The
`collect_all_contributions()` count moves from 45 → 52. A new test
(`widget_contributions_covers_every_preset_widget_type`) pins that
every `widget_type` in the default presets resolves to a registered
contribution.

### Facet template visibility in the property panel (§2.3 first slice)

`derive_property_rows` in `prism-shell/src/state.rs` now appends a
"Template (inline)" or "Template (component ref)" section under the
schema rows when the selection is a facet node. Inline templates
expose the root component + one inspector row per immediate child;
component-ref templates expose a single row pointing at the
referenced component id.

This is the **data-side** of inline template editing — visibility
into what's there. The full edit surface (canvas-side selection of
template descendants → click router branch → facet template
mutator) is still a multi-day follow-up; the new `facet_template_rows`
helper carries a doc comment pointing at the next step.

### Followup doc corrections (2026-05-15)

- **A3 (`{tokens.*}` interpolation) marked LANDED in
  `ui-migration-followups.md`.** It was claimed deferred there
  but is actually fully wired through `LowerScope::with_design_tokens`
  (Wave 14.1) with a passing regression test
  (`tokens_binding_resolves_color_in_style_namespace`). PRSS
  `[tokens.*]` overrides merge on top.
- **A2 (`bind:*`) re-classified PARTIAL.** The parser + runtime
  carry every binding through to `data-bind-<key>` semantic
  attrs (`interpret.rs:2168`). The actual install path
  (`data-bind-*` → registered `Effect`) is the missing piece;
  the parser-side claim of "no lowering" was stale.

### `clay-migration-plan.md` archived in place

The 6286-line plan now opens with a STATUS preamble that calls
out the migration as landed and points active readers at
`ui-migration-followups.md`. A quick-navigation index lists the
section blocks so readers can skip directly to whichever
implementation chapter their CLAUDE.md cross-reference points at.
The body is unchanged — every numbered section is still
addressable for back-references — but new readers no longer have
to know the project history to spot the difference between
decision record and live punch list.

### Still on the punch list

- Real `mlua`-backed `LuauHost` in the shell (replacing
  `NoopLuauHost`).
- PRUI A2 + A4. A3 (`{tokens.*}` interpolation) is **already
  landed** as Wave 14.1 — the followups doc was stale. A2
  (`bind:*`) is half-wired: parser + `data-bind-*` carry-through
  ship, the skeleton-side bind installer is the missing piece.
  A4 (`fct:*`/`sig:*`) truly isn't lowered.
- Web backend frame loop (B3).
- `.prism-ui` hot reload (C3).
- `prism-shell/src/state.rs` decomposition (6646 lines and growing).
- Inline-template canvas-side editing surface (§2.3 second slice).
- `packages/prism-studio/src-tauri/` rename (touches packaging — flag
  before committing).
- Archive closed phases of `clay-migration-plan.md`.

The "Suggested ordering" earlier in this doc still applies for what
remains.
