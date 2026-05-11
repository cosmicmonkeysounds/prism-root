# UI migration follow-ups

**Created:** 2026-05-11 (post-§43 phase E)
**Supersedes:** the "Not in this phase" notes scattered through
`docs/dev/clay-migration-plan.md` §43.
**Relationship:** companion to `clay-migration-plan.md`. That plan
narrates the *transition*; this doc enumerates what's still open
*after* the §43 wave landed and the shell boots end-to-end.

> **Status update — 2026-05-11.** First-wave landings: **A1**
> (`on:*` lowering + `parse_action`), **B1** (femtovg image
> rendering via resvg + `image`), **B5** (pointer-driven canvas
> selection), **D1** (registry merge in `Shell::new`). Workspace
> tests green at 6300+; clippy clean with `-D warnings`. The four
> sections below carry a `LANDED` annotation where applicable;
> the surrounding "Why deferred" / "Why it matters" prose is
> preserved so newcomers still see the rationale.
>
> **Status update — 2026-05-11 (later).** Second-wave landings on
> chrome click routing. New POINTER_ROUTES rows:
> `workflow-page-button` → `DockWorkspace::switch_page_by_id`,
> `dock-tab` → `DockWorkspace::navigate_to_panel`, `palette-item`
> → `state.catalog.palette_selected`, `nav-page-row` →
> `NavigationSlot::select_page_by_id`, `toolbar-device-pill` →
> `state.canvas.device`, `toolbar-zoom-reset` →
> `state.canvas.viewport.zoom = 1.0`. Two blocks (`shell.menu-item`,
> `shell.icon-button`) and one shared helper
> (`chrome::icon_button_node_tinted`) grew a `command` prop /
> parameter that lowers to `data-on-click="cmd <id>"`; the existing
> `route_on_click` dispatches through the command table — no new
> per-feature handler. Adding a clickable chrome button is now one
> arg in the lowering call (or one `command` prop in the source),
> never an `events.rs` edit. Workflow tabs, dock-panel tabs,
> palette pills, nav-page rows, device pills, zoom reset, and
> every menu item with a bound command are clickable end-to-end.
> See **B6** below for the remaining chrome that still has no
> click handler.
>
> **Status update — 2026-05-11 (third wave).** `chrome::icon_button_node`
> grew the `command: Option<&str>` arg its tinted sibling already
> carried, and every consumer threads through it. A new
> `BuilderService` lands the nine commands the toolbar / inspector /
> menu-bar buttons fire: `builder.align-{left,center,right}`,
> `view.zoom-{in,out}`, `builder.move-selected-{up,down}`,
> `builder.delete-selected`, and `navigation.add-page`. Three new
> `CanvasSlot` mutators (`reorder_selection`, `zoom_by`,
> `set_selection_align`) plus one `NavigationSlot::add_page` plug in;
> all selection-affecting commands run
> `AppState::resync_builder_for_selection` so the inspector tree and
> property rows stay coherent. Tests: 331 lib (was 293), clippy clean,
> workspace `cargo test` green. See **B6** below for which chrome
> rows still pass `None` for `command` (nav-page chevrons /
> schema-row trash / signal-row trash) — they're waiting on a
> "selected page / field / connection" cursor on the owning slot.

The Slint exorcism is functionally complete: zero `slint*` rows in
`Cargo.lock`, `cargo test --workspace` is green at 6300+ tests, and
`cargo run -p prism-shell` opens a populated Studio frame. What
remains is a long tail of *named-but-unfinished* surfaces — pieces
where the contract is in place, the smoke test passes, but the
production behaviour is a stub, a deferred follow-up, or a
documentation lie.

Items are grouped by area, ordered roughly by user-visible impact
within each area. Each item carries (a) what's done, (b) what's
missing, and (c) the file(s) that change when it lands.

---

## A. DSL completeness

The grammar parses every attribute namespace from plan §4.3
(`packages/prism-core/src/language/prism_ui/ast.rs:73-97`) and the
runtime interprets a working subset, but most behaviour-bearing
namespaces are *classified-but-not-lowered*. Authoring
`<button on:click="emit save"/>` parses cleanly and renders the
button — but the click does nothing.

### A1. Action grammar (`on:*` attribute) — LANDED
- **Status:** `prism_builder::signal::parse_action` parses every
  shipped verb (`emit`, `cmd`, `set`, `toggle`, `navigate`, `play`,
  `luau {…}`); the runtime's `apply_container_attributes` and the
  registry tag resolver both attach `data-on-<event>="<action>"`
  semantic attrs at lower time. `prism-shell/src/events.rs::route_on_click`
  reads the attr from the hit, calls `parse_action`, and dispatches
  `emit` through `SignalsService::fire_signal` and `cmd` through
  `CommandTable::run`. The other verbs parse cleanly and surface as
  `ParsedAction::*` variants whose executor is a no-op pending the
  owning subsystem.
- **Plan §:** §4.4 (`emit`, `set`, `toggle`, `navigate`, `play`,
  `luau {…}` grammar).
- **Lands in:** `packages/prism-ui-runtime/src/interpret.rs`
  (new `parse_action` helper + handler attach), the resolver in
  `packages/prism-builder/src/ui_resolver.rs` (forward the
  parsed action to the block as a `Connection`), and one wire
  through `SignalsService::on_event`.
- **Why it matters:** every click / hover / submit declared in
  `.prism-ui` source is dead until this lands. Today the host
  works around it with `data-role` hit-test routes in
  `prism-shell/src/events.rs` — a parallel path that should
  collapse onto the action grammar once it works.

### A2. Two-way binding (`bind:*`)
- **Status:** classified in the AST, no lowering. The sugared
  signal-pair (`Connection` for read + write-back) does not exist.
- **Lands in:** the same files as A1 — `bind:value="form.email"`
  desugars to `on:change="set form.email = event.value"` plus a
  read-binding for the initial value.
- **Dependency:** ships *after* A1.

### A3. Style tokens (`style:*`)
- **Status:** partial. The interpreter handles
  `style:color` / `style:background` / `style:radius` over hex
  strings (`interpret.rs:573-690`). Token resolution
  (`style:background="{tokens.colors.surface}"`) is **not** wired
  — `{tokens.x}` interpolation reads the binding scope, not the
  `prism_core::design_tokens` table.
- **Lands in:** `interpret.rs::lookup_expression` (route
  `tokens.*` to `prism_core::DesignTokens`), and the
  `LowerScope` builder picks up a `&DesignTokens` reference.

### A4. Facets (`fct:*`) and signal declarations (`sig:*`)
- **Status:** classified, not lowered. `<facet name="items"
  from="resource:posts">…</facet>` parses; the runtime never
  reaches for `FacetDef` or the resource layer.
- **Lands in:** new `prism-ui-runtime` module hooking
  `prism_builder::FacetDef`. The resolver needs to expand a
  facet element into a repeated subtree at lower-time.
- **Why deferred:** no shipped block consumes facet output yet
  — landing this without a consumer is paying for unused
  infrastructure. Re-evaluate once the first authored Studio
  page wants a list bound to a resource.

### A5. Comment scanner UTF-8 bug
- **Status:** `consume_until_gt` in
  `packages/prism-core/src/language/prism_ui/grammar.rs:600` slices
  by byte offset without char-boundary checks. An em-dash (or any
  multi-byte glyph) inside `<!-- … -->` panics the parser.
- **Lands in:** two-line `char_indices` boundary check.
- **Filed:** plan §15 caveat. The current `app.prism-ui` skeleton
  sidesteps the bug by sticking to ASCII; author-written prose
  with typographic punctuation will trip it.

---

## B. Runtime gaps

### B1. Image rendering on `femtovg` — LANDED
- **Status:** `RenderCommand::Image` paints through
  `prism_ui_runtime::images::ImageCache` (PNG / JPEG via the `image`
  crate's `load_image_mem`, SVG via `resvg` + `tiny-skia` →
  `DynamicImage::ImageRgba8` → `femtovg::ImageSource`). Sources
  resolve through a host-supplied `AssetLoader`; the shell's
  `prism_shell::assets::loader()` plugs in 39 SVG icons via
  `include_bytes!`. Tint paint (image as mask × paint colour) is
  in place via `Paint::set_color`. Negative-cached: a missing
  source doesn't re-hit the loader every frame.
- **Lands in:** `paint.rs` + a new image cache keyed by
  `source: &str` → decoded `femtovg::ImageId`. SVG decode
  via `resvg` / `usvg` (icons are all SVG); PNG / JPEG via
  `image` crate.
- **Why it matters:** every chrome glyph (nav buttons, icon
  buttons, gizmo handles) is invisible on the native render
  path. SSR shows them correctly; the native window is blank
  where glyphs should be.

### B2. Image-tint runtime extension
- **Status:** noted as the lone deferred runtime gap at the
  close of plan §12. `Node::Image` has no `tint: Option<Color>`
  field, so the same monochrome glyph cannot be re-coloured per
  state (resting / hover / active) without N copies of the SVG.
- **Lands in:** one field on `Node::Image`, one branch in the
  femtovg paint pass (multiply against the decoded glyph), one
  CSS `filter: …` mapping in `semantic_html`.
- **Dependency:** ships after B1.

### B3. Web backend frame loop
- **Status:** `backends::web::mount` grabs an existing
  `<canvas>` and renders **one frame**
  (`packages/prism-ui-runtime/src/backends/web.rs:49`). No rAF
  loop, no event pump.
- **Lands in:** mirror the native backend's
  `ApplicationHandler` + `EventLoopExtWebSys::spawn_app` path
  the Phase-1 update already documented. The shell's
  `web_start` (`prism-shell/src/lib.rs`) hands the Surface to
  the new pumped loop.
- **Why it matters:** `prism build --target web` produces a
  binary that paints once and never updates. Every interaction
  the native window handles is dead in the browser.

### B4. Text-input focus / IME / drag scrubber
- **Status:** `Node::TextInput` renders as a static rect with
  a value; `dispatch_event::Text` arms are no-ops for
  non-modal focus. The boolean field-edit path
  (`prism-shell/src/events.rs`) works end-to-end through a
  `data-role` route; text / number / select / color / file
  rows carry the same routing attrs but defer real edit UX.
- **Plan §:** §43 Phase C2 "Not in this phase" note.
- **Lands in:** runtime focus model on `Surface`
  (current selection or null), `dispatch_event::Text` routes
  printable characters to the focused input, drag-scrubber
  becomes a pointer-down → pointer-move handler on
  `data-role="drag-number"` rows.

### B6. Remaining chrome click routes
- **Status:** workflow-page-button / dock-tab / palette-item /
  nav-page-row / toolbar-device-pill / toolbar-zoom-reset now route
  through `POINTER_ROUTES`. `shell.menu-item` and `shell.icon-button`
  both grew a `command` prop that lowers to
  `data-on-click="cmd <id>"`; the existing `route_on_click`
  dispatches through the command table. Adding a clickable chrome
  button no longer needs a new POINTER_ROUTES row — wire the
  command id through the binding closure and the rest is free.
- **Update — 2026-05-11 (third wave).** `chrome::icon_button_node`
  itself now takes the `command: Option<&str>` arg the helper's
  sibling (`icon_button_node_tinted`) has carried since the second
  wave, and every shell call-site threads through it. Wired
  end-to-end:
  - `shell.builder-toolbar` align buttons → `builder.align-left`
    / `builder.align-center` / `builder.align-right`
    (set `text-align` on the canvas selection).
  - `shell.builder-toolbar` zoom buttons → `view.zoom-in` /
    `view.zoom-out` (multiplicative step on
    `canvas.viewport.zoom`, clamped to the toolbar schema's
    `[0.1, 8.0]`).
  - `shell.inspector-row` chevrons + trash →
    `builder.move-selected-up` / `builder.move-selected-down` /
    `builder.delete-selected` (act on `canvas.selection`).
  - `shell.menu-bar-row` "+" → `navigation.add-page` (appends a
    fresh `Page N` to `NavigationSlot::pages`, activates it).

  All nine commands live on a new `BuilderService` registered
  between `SelectionService` and `ClipboardService`; they re-run
  `AppState::resync_builder_for_selection` after mutations that
  could change the inspector tree or property rows so derived
  panels stay coherent. Eight new unit tests on the service plus
  the state-test sweep cover the mutators end-to-end (331 lib
  tests up from 293).

  Still dead-clickable:
  - `shell.app-card` (Launchpad): carries `data-app="<id>"` but no
    `data-role` and no state hook for "active app." Needs a model
    decision (does clicking a card switch the loaded
    `app.prism-ui` skeleton, set a slot field, dispatch a command?)
    before the route is meaningful.
  - `shell.nav-page-row` chevrons + trash, `shell.schema-row`
    trash, `shell.signal-connection-row` trash. The lowering call
    sites now thread `command: None` for API uniformity, but the
    slots don't yet carry a "selected page" / "selected field" /
    "selected connection" cursor that a stateless `cmd <id>` body
    could target. Threading commands here lands once those
    cursors exist — three small slot fields plus three commands
    on `BuilderService`.
  - `shell.dock-tab-bar` "close tab" / "+new tab" affordances —
    not authored yet; deferred until the dock workspace grows
    user-facing tab management.
- **Lands in:** one new row in `POINTER_ROUTES` per role *or*
  threading a `command` id through `icon_button_node` /
  `icon_button_node_tinted` / `menu-item` / new
  `shell.icon-button` consumers. The latter is now the working
  precedent — both `chrome::icon_button_node` variants and three
  blocks (`shell.menu-item`, `shell.icon-button`, plus every
  call-site that composes them) compose on it.

### B5. Pointer-driven canvas selection — LANDED
- **Status:** `shell.builder-canvas` tags every container under its
  preview slot with `data-canvas-node="<id>"` (recursive walk in
  `tag_canvas_subtree`). `events::route_canvas_node_select` reads
  the attr from the hit and routes through `AppState::select_node`
  before the canvas-tool drag capture runs. Chrome containers
  whose ids would collide with canvas doc ids (`root` is the
  canonical collision — both `<shell.app-window id="root">` and
  `BuilderDocument::page_shell()`) are safely disambiguated by the
  presence of the `data-canvas-node` attr.
- **Plan §:** §43 Phase C "Not in this phase" — "Pointer-driven
  canvas selection (clicking a rendered node on the canvas)
  needs a hit-test surface on `prism_ui_runtime::Surface` that
  the canvas slot can consult."
- **Lands in:** the canvas slot consults
  `Surface::hit_test_at` (already shipped in §43 C2/D) for any
  pointer-down within its bounds, looks up the resolved hit's
  `id` against `state.canvas.document`, and routes through
  `AppState::select_node`. ~15 LoC.

---

## C. Build / tooling

### C1. `prism-ui-build` wiring in `prism-shell/build.rs`
- **Status:** `packages/prism-ui-build/src/lib.rs` ships
  `compile_source` + `compile(path)` that validate the
  `.prism-ui` source at build time and emit a Rust module
  carrying `SOURCE` / `COMPONENT_NAMES` / `nodes()`. **No
  consumer**: `prism-shell` does not have a `build.rs`, and
  `Skeleton::load` reads `include_str!("../ui/app.prism-ui")`
  and re-parses at runtime
  (`prism-shell/src/render.rs:37-42`).
- **Why deferred:** the runtime parse is also the live-edit
  path, so wiring `prism-ui-build` is *correctness gating*,
  not *behaviour gating* — today a parse error in `app.prism-ui`
  is a runtime panic in `Skeleton::load` rather than a build
  failure.
- **Lands in:** new
  `packages/prism-shell/build.rs` calling
  `prism_ui_build::compile("ui/app.prism-ui")`, plus the
  emission re-validates against the resolver registry shape
  so unknown shell tags fail the build.

### C2. Shell binary `--scene` / `--screenshot` flags
- **Status:** `prism visual --scene <name>` is the documented
  harness; `commands/visual.rs` lists `"builder"` /
  `"builder-empty"` / per-viewport variants. The CLI shells
  out to a flag set the shell binary doesn't accept, so
  screenshots are captured manually via `cargo run` + OS
  screencapture.
- **Plan §:** §43 Phase E "Not in this phase".
- **Lands in:** `prism-shell/src/bin/native.rs` learns
  `--scene <id>` and `--screenshot <path>`; the native backend
  gains a one-frame offscreen capture path that writes a PNG
  and exits.
- **Dependency:** trivial — does not block any phase.

### C3. Live edit / hot reload of `.prism-ui`
- **Plan §:** §4.6 and the "What breaks (and is fine)" list in
  §17 ("File-watch reload comes back as a `Surface::set_tree`
  re-lower against the same `app.prism-ui` source — strictly
  simpler.").
- **Status:** no file-watcher exists. `prism dev shell` rebuilds
  on `.rs` changes only; `app.prism-ui` edits require a respawn
  via cargo recompile because the file is `include_str!`-ed.
- **Lands in:** a small `notify`-backed watcher in
  `dev_loop::DevLoop` (or `prism-shell`'s native binary) that
  re-parses + re-builds the skeleton + re-renders. Pairs with
  C1 — once `build.rs` validates the file, runtime live-reload
  is a `Skeleton::from_source(fresh)` swap.

---

## D. Shell features and follow-ups from §43

### D1. Merge `prism_builder::starter::BUILTINS` into the live registry — LANDED
- **Status:** `ShellComponentRegistry::register_document_builtins`
  delegates to `prism_builder::starter::register_builtins`, called
  from `Shell::new` alongside `register_shell_builtins`. The boot
  resync now finds schemas for `text` / `button` / `image` / …
  selections; the E3 e2e test dropped its hand-rolled
  `for spec in BUILTINS` workaround. A
  `shell_and_document_builtin_ids_are_disjoint` test pins the
  namespace invariant at the table level so any future collision
  fails before the binary ships.
- **Plan §:** §43 Phase E "Not in this phase" note. The E3
  test (`e2e_palette_pick_drop_select_edit_updates_tree`)
  works around it by extending the test registry with
  `prism_builder::starter::BUILTINS`.
- **Lands in:** one call in `Shell::new` that calls
  `prism_builder::starter::register_builtins(&mut reg.inner)`
  alongside the existing `register_shell_builtins`. The
  collision check is: today there are zero id overlaps
  (`shell.*` vs `text`/`button`/…), but a follow-up should
  pin set-disjointness as a registration-time invariant so
  future additions panic if they collide.

### D2. Real `mlua`-backed `LuauHost`
- **Status:** the only impl is `NoopLuauHost` in
  `prism-shell/src/services/luau.rs:30-41`, which records
  every call and returns `Null`. `mlua` was deliberately
  dropped from `prism-shell`'s deps in the Phase-5 cutover
  (per §24.7 — host concerns stay host-side).
- **Lands in:** a new feature `luau = ["dep:mlua",
  "prism-luau-derive"]` on `prism-shell` *or* a separate
  `prism-shell-luau` crate exposing a real `MluaLuauHost`.
  `Shell::new` constructs the appropriate host behind a
  `cfg(feature = "luau")` gate.
- **Why it matters:** `SignalsService::Custom` action arm
  (when A1 lands) silently no-ops every user script. Until
  the real host plugs in, every "run this Luau snippet on
  click" handler in authored docs is dead.

### D3. Service-side IO follow-ups
- **Status:** `PersistenceService` / `ProjectService` ship
  against `Vfs` with `OsVfs` for prod and `InMemVfs` for
  tests. Pickers (rfd file dialog, native folder browser)
  are *not* in the service — the discipline is "host writes
  `state.project.current_file`, dispatches the command."
  Today no host actually drives that path.
- **Lands in:** the desktop bin (`prism-shell/src/bin/native.rs`)
  grows a clipboard / file-picker resource that handles
  Ctrl+O / Ctrl+S key events by spawning `rfd::FileDialog`
  and then dispatching `file.save` / `file.open` against the
  populated slot.
- **Web equivalent:** the web backend writes to
  `state.project.current_file` from a `<input type=file>`
  click and dispatches into the same service.

### D4. Per-app shells
- **Status:** the canonical `app.prism-ui` skeleton is one
  composition (`<shell.app-window><shell.dock-workspace/>`).
  The four apps (Lattice, Musica, Flux, Studio) currently
  share that frame, with content discrimination living
  inside the dock panels. Plan §32 ("Starter catalog")
  documents the four-apps story but the per-app skeleton
  swap is not implemented.
- **Lands in:** `Shell::new` picks a skeleton based on the
  active `AppId` from `BootConfig`; per-app `app-<id>.prism-ui`
  files live next to the canonical one. No new infrastructure
  — the resolver, the registry, and every service stay
  identical.

---

## E. Phase 6 — Mobile and packaging

Untouched. Plan §8 Phase 6 sketches the path:

- iOS / Android via winit's mobile targets + a chosen
  GL/Metal context (winit-on-mobile exists; femtovg's GL
  backend works against any context).
- `cargo-packager` packaging is unchanged in shape — the
  binary surface is `prism-ui-runtime` + femtovg, no Slint.

No `prism-shell` source needs to change. The work is build
configuration, signing, and a host shim equivalent to
`src/bin/native.rs` for each target.

---

## F. Documentation rot

- **`prism-core/src/lib.rs:1-55`** still says "shared
  foundations for the Slint-era Prism stack" and references
  `docs/dev/slint-migration-plan.md`. The file has been
  through three phase cutovers; only the header is stale.
- **`prism-core/CLAUDE.md`** (per-module CLAUDE.md) — opens
  with "Shared Rust foundations for the Slint-era Prism
  stack. Phase-2 target of the Slint migration."
- **`prism-builder/src/lib.rs:9-11`** carries a Slint-era
  comment explaining "the Slint DSL emission path was
  deleted in the Phase 5 cutover follow-up" — accurate, but
  the comment should move to a one-line "post-Slint, this
  crate emits `.prism-ui` via §29's `prism_ui_emit`".
- **`prism-builder/src/prism_ui_emit.rs:3-4`**, **`schemas.rs:2,134`** —
  same Slint references in module headers.
- **`packages/prism-studio/src-tauri/`** directory name is
  a pre-cutover historical artefact; renaming it is a
  followup called out in the root `CLAUDE.md`.

These are pure cosmetics — no behaviour rides on them — but
they corrode the trustworthiness of in-tree docs as a source
of truth. A single cleanup PR retires every one.

---

## Priority recommendation

For the most impact-per-PR, the suggested order is:

1. **A1 (action grammar)** — unblocks every authored signal
   handler in `.prism-ui` source. Today the host hand-rolls
   parallel `data-role` routes in `events.rs`; landing A1
   collapses that duplication onto one declarative path.
2. **B1 (femtovg image)** — every chrome glyph is invisible
   on the native window until this lands. SSR already
   works; the discrepancy is the visible regression.
3. **D1 (registry merge)** — full property-row derivation
   for builder nodes is one line of `Shell::new`. Today the
   E3 e2e test fakes this; the production shell silently
   shows empty property panels when a builder node is
   selected.
4. **B5 (pointer canvas selection)** — clicking a rendered
   document node should select it. ~15 LoC against the
   already-shipped `Surface::hit_test_at`.
5. **C1 + C3 (build.rs + hot reload)** — together they
   move `.prism-ui` from "include_str + runtime parse +
   recompile to edit" to "validated at build, hot-reloaded
   at runtime." Best done as one pair so the validation
   path and the live-edit path share a single re-parse
   entry.
6. **B3 (web frame loop)** — necessary before any web
   demo. Mechanical port of the native backend's pumped
   loop.
7. **F (docs cleanup)** — single cosmetic PR, low cost,
   high readability win for any newcomer.

Everything else (A2-A5, B2, B4, C2, D2-D4, E) is a
deliberate "next year" set — the contracts are in place and
the gaps are documented; they ship as authoring demand
materialises.
