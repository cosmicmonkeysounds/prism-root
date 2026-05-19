# Slint-Parity Gap Analysis

**Created:** 2026-05-18
**Author:** gap sweep (screenshots + code inspection)
**Companion to:** `wysiwyg-builder-roadmap.md` (product path),
`ui-migration-followups.md` (post-§43 punch list),
`state-of-prism.md` (plan/ADR reconciliation).

> **Purpose.** The post-Slint Rust shell has *regressed* from a working
> page builder to a near-blank window. This doc enumerates, against
> three captured baselines, every gap between what the current build
> renders and the full Prism Studio experience the Slint app delivered.
> It is a punch list, ordered by severity, with code anchors and
> acceptance bars.

---

## 0. The three baselines

| Tag | Date | Commit | What it renders |
|-----|------|--------|-----------------|
| **Slint** | 2026-04-22 | `d476b0d` | Full Prism Studio: tab bar (Flux/Canvas/Settings/Preview), workspace tabs (Components/Inspector/Explorer), Builder toolbar, palette w/ icons, Page Layout panel, live canvas rendering a real image node, full typography/color/spacing/radius inspector, mode bar (Edit/Design/Code/Fusion), status bar. |
| **Rust-working** | 2026-05-11 | `93997e05` | Page builder functional: component palette (text-only list), canvas rendering a seeded document ("Welcome to S" hero + Submit button), Properties panel (Child spacing / Padding / Border width / Border color), `Desktop` viewport pill, bottom `Edit` pill. **Partial parity.** |
| **Current** | 2026-05-18 | `HEAD` (`bf9ddbf`) | Near-blank window: left icon rail (home / folder / search), a wrapped menu bar (File / Edit / View / Window / Help), one stray `Edit` label at the bottom. **No palette, no canvas, no properties, no document.** |

The diff `93997e05..HEAD` is **+90,180 / −25,545 across 350 files**,
overwhelmingly in `prism-ui-runtime` (`interpret.rs` +11k,
`editor.rs` +2.4k new, `animator.rs` +1.2k new, `luau_scope.rs` +1.9k
new, `layout/mod.rs`, `paint.rs`, `command.rs` heavily reworked). The
regression is somewhere in that wave, **not** in panel wiring (see §1).

---

## 1. P0 — The blank-screen regression (blocks everything else)

> **CORRECTION (2026-05-19, supersedes the DIAGNOSIS / ROOT CAUSE /
> VERIFIED blocks below).** Those blocks were reached with diagnostics
> that are *structurally incapable* of seeing this bug, so their
> "pipeline is correct / fixed / verified" conclusions do not hold for
> the live window. Established empirically:
>
> 1. **The headless pipeline IS correct at HEAD — fully.** Dumping the
>    real `Surface::commands()` (the exact path the femtovg backend
>    runs, via `Surface::new(wrap_root(shell.render()))`) yields 119
>    commands with the seeded document present in correct paint order
>    and colour: `[092]` white `canvas-page`, `[093]` `Welcome to
>    Studio` rgba(20,20,20,255) fs32, `[094]` paragraph, `[095]` blue
>    Submit rect, `[096]` white `Submit`. Scissor depth is balanced
>    (0); nothing clips the content. Tree, layout, and command
>    emission have **no defect**.
> 2. **Every diagnostic the superseded blocks relied on is blind to
>    the actual failure.** `--scene default` is a literal no-op
>    (`headless.rs`: `BuiltinScene::Default => {}`). `dump_png` /
>    `prism visual` have **no glyph rasteriser** — `png_paint` draws
>    each Text command as a 1px placeholder strip at the box bottom
>    (`png_paint.rs`), so a text/glyph/femtovg regression is invisible
>    to it. The JSON dump is pre-paint. So "VERIFIED via `--scene
>    default --screenshot`" verified box geometry only — never a
>    rendered glyph, never the live GL path.
> 3. **Confirmed live symptom (user, `cargo run -p prism-shell`):**
>    chrome (nav rail + menu + palette + properties panel) paints, but
>    the centre canvas is an **empty white area** — *not* a dark
>    rectangle, *not* the seeded heading/paragraph/button. Since the
>    headless command stream for that exact region is correct, the
>    regression is in the **live femtovg paint / window path** — i.e.
>    the `(B) femtovg-runtime` hypothesis the blocks below explicitly
>    (and wrongly) dismissed. It is *not* in any headlessly-reachable
>    code, which is why the test suite is green and every prior
>    diagnostic "passed".
> 4. **One real latent colour bug found (not the blank-screen cause).**
>    `ui/components/builder-canvas.prui:39` sets
>    `style:background="#040000"` — a 6-digit hex = opaque near-black
>    RGB(4,0,0) α255 spanning the whole canvas. It should be
>    `#04000000` (8-digit, alpha `00` = transparent), matching the
>    empty-grid-cell value on line 75. Introduced in `de286ad`
>    ("composable builders, dsl", 2026-05-13). In paint order the
>    white `canvas-page` rect overpaints it, so it does not produce a
>    visible dark rectangle — consistent with the user seeing white,
>    not black — but it is still wrong and should be fixed.
> 5. **Next step (requires the live window — not headlessly
>    reproducible with current tooling):** instrument the femtovg
>    `redraw()` / `paint::draw_at` (or land the femtovg-offscreen
>    capture the `dump_png` comment already anticipates) and compare
>    what femtovg paints for cmds 92–96 against the chrome cmds that
>    *do* paint. The acceptance bar below stands; the bisection
>    sequencing in §3 (item 1: "bisect for the render regression") is
>    misdirected — there is nothing to bisect in headless code.

**Symptom.** The 2026-05-18 build paints only the chrome shell
(`app-window` nav rail + menu bar). The dock workspace — Builder
canvas, component palette, properties panel — does not paint, even
though the seed populates a full `BuilderDocument` and the dock
defaults to the Edit page.

**What is NOT the cause (verified by code inspection).** The wiring is
intact end-to-end:

- Shell root tree built in `prism-shell/src/render.rs:424-501`
  (`render_tree_with`), skeleton `prism-shell/ui/app.prui`,
  app body grafted via `Skeleton::with_app_body()`
  (`prism-shell/src/shell.rs:1159`).
- Seed hydrates a real document: `prism-shell/src/seed.rs:54`
  (`state.canvas.document = seed_document()`), `seed.rs:333-379`
  builds `BuilderDocument::page_shell()` + demo heading/paragraph/
  button, `seed.rs:62` pre-selects `demo-heading`.
- Dock defaults to the Edit page with Builder + Palette + Properties
  present: `prism-dock/src/workspace.rs:20,25`,
  `prism-dock/src/page.rs:16-43,157-167`.
- All three panel tags (`shell.builder-canvas`,
  `shell.component-palette`, `shell.properties-panel`) are bound to
  AppState slots: `prism-shell/src/props.rs:225,242,243`, and
  dispatched through `dock-panel.prui` /
  `dock-node.prui` (no stubs).

**DIAGNOSIS (2026-05-19) — the render pipeline at HEAD is correct.**
Closed by empirical bisection through six independent diagnostics run
against the *real* production code path (temp scaffolding, since
reverted):

1. `WorkspaceSlot::dock_workspace_props_with_catalog` emits a
   well-formed dock tree (`type`/`axis`/`ratio`/`first`/`second` and
   `panel-id`/`content-tag`/`tabs`). Production catalog is
   `DockCatalog::with_builtins()` (`app_registry.rs:67`), so
   `content-tag` resolves to `shell.builder-canvas` etc.
2. Real `Shell::render()` (full reactive path: `render_scope`,
   memo cache, class_deps, stylesheet) yields the **complete tree**
   — 57 KB JSON, all `data-role`s present (`dock-workspace`,
   `dock-panel`, `builder-canvas`, `component-palette`,
   `palette-item`, `builder-toolbar`, `properties-panel`).
3. `layout::compute` @1280×800 produces **correct rects**: menu
   1280×28, nav 40×800, palette panel 230×800 with tabs
   (Components/Inspector/Explorer) and 17 positioned items.
4. Layout is **also correct at retina-physical viewports**
   (2560×1600, 2880×1800, 3024×1964) — no collapse at the size the
   live femtovg window actually uses.
5. The CPU rasteriser (`png_paint`) of the real frame renders the
   **full builder UI** (palette + near-black canvas + toolbar
   clusters + white page + properties panel).
6. `Surface::commands()` — the **exact** live path
   (`rebuild_if_dirty` → `compute_full_with_hits`) — is
   byte-identical to `compute_full`: 115 commands, 42 rects,
   identical bbox `(0,0)-(1574,841)`, correct colours (opaque white
   content, `#fafafa` palette panel, α192 purple palette rows).

The earlier suspects (`prism.builder-host` Tier-3 stub, `layout/mod.rs`
collapse, `interpret.rs` dispatch, the research-agent hyphenated-
identifier theory) are **all disproven**. `prism-studio/src-tauri`'s
launcher is literally `Shell::new()?; shell.run()?;` — identical to
bare `prism-shell`.

Diagnostics 2 & 5 above were **incomplete, not wrong**: they confirmed
every panel `data-role` and the command/colour geometry, but did not
drill into the **`canvas-page` subtree**. A follow-up JSON-dump
drill-down (`prism-shell --scene default --screenshot frame.json`)
found the real defect:

> The `canvas-page` container has **only** `::grid` + `::overlay`
> children. The seeded `BuilderDocument` (heading/paragraph/button)
> is **never injected**. `data-role="canvas-preview"`, `::preview`,
> and `data-canvas-node` counts are **0** in the lowered tree — i.e.
> `<prism.builder-host>` resolves to nothing.

**ROOT CAUSE (resolved 2026-05-19).** `finalize_prism_ui_resolver`
populates a write-once `OnceLock` shared resolver
(`prism_ui_loader.rs:105,286`). `register_full_shell_chrome` called
it **before** `Shell::new` ran `register_document_builtins`
(`shell.rs:369-372`). So the resolver every `.prui` block uses
snapshotted the registry **without** the `prism.*` primitives
(including `prism.builder-host`) and **without** the document
builtins (`text` / `button` / …). Inside `builder-canvas.prui`,
`<prism.builder-host>` resolved to nothing → the seeded document was
never grafted onto the page → the canvas (the dominant central
region) rendered empty. Chrome survived because every `shell.*` block
was registered *before* `finalize`. This also explains why the live
window reads as "basically blank": the canvas is the largest surface,
and any DSL-composed subtree needing `text`/`button` resolution
inside a block body dropped too.

The earlier (A) stale-binary / (B) femtovg-runtime hypotheses were
**wrong** — the regression *is* in headlessly-reachable code; the
first six diagnostics simply didn't inspect the canvas subtree.

It turned out there were **three independent breaks** stacked on the
canvas-injection path; all three had to be fixed before the document
rendered:

1. **Resolver finalize ordering** (`registry.rs`, `shell.rs`). Fold
   `register_document_builtins` into `register_full_shell_chrome`
   *before* the single write-once `finalize`, and drop the
   now-redundant separate call in `Shell::new` + the test
   double-call. The resolver now snapshots the complete registry
   (chrome + document builtins + `prism.*` primitives), so
   `<prism.builder-host>` resolves. *(Result: `canvas-preview`
   container now emitted — but still empty.)*
2. **Dispatch tag keying** (`prism-builder/src/ui_resolver/mod.rs`).
   A dock-panel routes content via
   `<dispatch component="shell.builder-canvas">`, so the resolver's
   `element.tag` is `"dispatch"`. `host_children_for(&element.tag)`
   looked up the wrong key. Now keyed by the **resolved** target tag.
3. **`host_children_by_tag` doesn't survive descent**
   (`prism-builder/src/ui_resolver/mod.rs`). That map lives only on
   the root `LowerScope`; every nested `PrismUiBlock` rebuilds a
   fresh scope, so by dock-panel depth it is gone. Only
   `tag_emissions` is forwarded at every level (and
   `harvest_tag_emissions` keeps the children slice). Added a
   fallback: `host_children_for(resolved_tag)` **or**
   `tag_emissions[resolved_tag].children`.

**VERIFIED (2026-05-19).** `prism-shell --scene default --screenshot`
now renders the seeded document on the canvas: JSON dump shows
`Welcome to Studio` heading, `demo-paragraph`, `demo-button`/`Submit`,
`data-role="canvas-preview"`, and `data-canvas-node` present; the CPU
PNG shows the heading/paragraph text rows and the blue Submit button
on the page surface — matching the 2026-05-11 working baseline. This
also lifts the broader "basically blank" symptom: every DSL-composed
subtree that needed document-component resolution was affected by
fix 1.

**Doc-vs-reality contradiction (itself a finding).**
`wysiwyg-builder-roadmap.md` asserts *"As of 2026-05-18 every
capability is ✅ / 🟢 with no open code gap."* The 2026-05-18
screenshot falsifies that for the visual-canvas-authoring capability.
Either the roadmap's status sweep was done against tests (which are
green at 6300+) and never against a rendered window, or a regression
landed after the sweep. **Acceptance for this doc's P0: the roadmap's
claim must be re-validated against a screenshot, not the test suite.**

**Acceptance bar (P0 closed when):** launching
`cargo run -p prism-shell -- --app lattice --panel builder` shows the
seeded document on a canvas, the palette list on the left, and
populated property rows on the right — i.e. the 2026-05-11 baseline is
restored. Capture via `prism visual --scene builder` and diff against
`Screenshot 2026-05-11 at 8.03.02 PM.png`.

---

## 2. P1 — Parity gaps that existed even at 2026-05-11

These were never ported to the Rust shell; the 2026-05-11 "working"
build is itself only *partial* parity with Slint `d476b0d`. Listed so
that "restore the regression" is not mistaken for "reach parity".

### 2.1 Top tab bar / document tabs
Slint had a real tab strip: `Flux | Canvas | Settings | Preview | +`
with close affordances. Current shell has only the OS menu bar.
- Now: `workflow-page-bar.prui` / `workflow-page-button.prui`
  exist and bind at `props.rs:240` but render the Edit/Design/Code
  *mode* pages, not document/app tabs.
- **Gap:** open-document tab strip with new-tab (`+`) and per-tab
  close, distinct from the workflow mode bar.

### 2.2 Workspace tabs (Components / Inspector / Explorer)
Slint grouped the left dock into three named tabs. Current shell has
a dockable tab bar (`dock-tab-bar.prui`) but no seeded
Inspector/Explorer grouping — only the palette shows.
- **Gap:** seed the left dock with the three-tab group and wire
  Inspector + Explorer panel content (Explorer file tree exists in
  catalog seed `seed.rs:116-137` but is not surfaced).

### 2.3 Builder toolbar
Slint canvas had a toolbar: align L/C/R, distribute, group, z-order
up/down, delete, viewport `Desktop|Tablet|Mobile`, zoom %, node count.
- Now: `BuilderService` commands exist
  (`builder.align-*`, `view.zoom-*`, `builder.move-selected-*`,
  `builder.delete-selected`) per `ui-migration-followups.md` third
  wave, and a `Desktop` pill renders, but the **toolbar surface
  itself is not laid out** above the canvas.
- **Gap:** the toolbar row component, populated with the existing
  commands, viewport device pills, zoom control, and live node count.

### 2.4 Page Layout panel
Slint left rail had `Show grid` toggle, `Size: Responsive`, Columns,
Column Gap, Rows, Row Gap sliders bound to the page-shell grid.
- **Gap:** no Page Layout panel in the current dock. The grid model
  exists (`BuilderDocument::page_shell()` is a Taffy grid); needs a
  panel + property bindings for columns/rows/gaps + grid overlay
  toggle.

### 2.5 Full property inspector
Slint inspector exposed: Image source, Alt text, Object fit, Font
family/size/weight, Line height, Text color, Background, Accent,
Spacing, Radius — i.e. a typed, per-component schema.
- Now: properties panel emits only generic box props (Child spacing /
  Padding / Border width / Border color) — `slots_doc.rs:109-120`.
- **Gap:** per-component typed property schema (typography, color,
  layout, media) driven off the component registry, not a fixed
  four-row box editor.

### 2.6 Mode bar (Edit / Design / Code / Fusion)
Slint had a centered four-way mode switch at the bottom; current build
shows a single stray `Edit` label.
- **Gap:** the four mode buttons rendered and switching workflow
  pages (the page model exists in `prism-dock`; only `Edit` is
  surfaced and it is mis-laid-out as loose text).

### 2.7 Status bar
Slint footer: `Editor · Flux · Canvas · Selected · 1 nodes · Prism
Studio`. `status-bar.prui` exists (added in the
`93997e05..HEAD` diff) but is not visible.
- **Gap:** lay out and bind the status bar (active editor, app, panel,
  selection, node count).

### 2.8 Palette polish
Slint palette had per-item icons and tighter typography; current
palette (when it rendered at 2026-05-11) is a text list with wrapping
labels ("Cod e", "Column s", "For m").
- **Gap:** palette item icons + label layout that does not wrap.

---

## 3. Suggested sequencing

1. **P0 first** — bisect `93997e05..HEAD` for the render regression
   (start with §1 suspects 1→2→3). Nothing else is verifiable until
   the canvas paints. Restore the 2026-05-11 baseline.
2. **2.6 + 2.7 + 2.3** — mode bar, status bar, toolbar are layout-only
   given the commands/pages already exist; cheap parity wins.
3. **2.2 + 2.4** — Explorer/Inspector tabs and Page Layout panel;
   medium effort, data already seeded.
4. **2.5** — typed per-component inspector; largest, needs registry
   schema work; do last.
5. **2.1 + 2.8** — document tab strip and palette icon polish.

## 4. Verification protocol

Per CLAUDE.md workflow §5: every item closes with a screenshot, not a
green test run. `cargo test --workspace` passing is necessary but has
been demonstrably *insufficient* (tests green at 6300+ while the
window is blank). For each gap:

- `cargo run -p prism-shell -- --app lattice --panel builder`
- `prism visual --scene builder` and `--scene builder-tablet`
- diff the capture against the matching baseline screenshot in repo
  root (`Prism Studio 2026-04-22…png`, `Screenshot 2026-05-11…png`).

Add a `builder-slint-parity` scene to `BuiltinScene`
(`prism-shell/src/testing.rs`) reproducing the Slint `d476b0d` layout
so parity is regression-tested going forward.
