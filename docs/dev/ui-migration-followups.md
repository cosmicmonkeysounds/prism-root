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
> workspace `cargo test` green.
>
> **Status update — 2026-05-11 (fourth wave).** Two more B6 click
> surfaces light up. `shell.app-card` now carries `data-role="app-card"`
> and routes through a new `POINTER_ROUTES` row to
> `WorkspaceSlot::set_active_app` — clicking a Launchpad card sets
> `state.workspace.active_app`. `shell.nav-page-row` chevrons + trash
> thread through three new commands
> (`navigation.move-page-{up,down}` / `navigation.delete-selected-page`)
> against a new `selected_page: Option<String>` cursor on
> `NavigationSlot`; `pages_list_json` emits `selected` /
> `show-delete` per row from it, and clicking a nav-page-row moves
> both the chevron cursor and the existing `is_active` flag. Tests:
> 340 lib (+9 over the third wave), clippy clean. The only B6 rows
> still passing `None` for `command` are `shell.schema-row` /
> `shell.signal-connection-row` — each needs a "selected field" /
> "selected connection" cursor on its owning panel before a
> stateless `cmd <id>` has a target.
>
> **Status update — 2026-05-11 (fifth wave).** Last two B6 click
> surfaces light up. `shell.schema-row` and `shell.signal-connection-row`
> both grew a cursor-key prop (`field-id` / `connection-id`) that
> lowers to `data-target-id`, and the trash icon button now threads
> `Some("schema.delete-selected-field")` /
> `Some("signals.delete-selected-connection")` instead of the placeholder
> `None`. Two new cursors land on `BuilderSlot`:
> `schema.selected_field: Option<String>` (stores the field name —
> there is no separate id on `SchemaField` and the name is unique
> per schema) and `selected_connection: Option<String>` on
> `BuilderSlot` itself. `SignalConnection` grew an `id: String` field
> (mirroring `prism_builder::Connection::id`) and dropped the
> per-row `selected: bool`; selection is now derived from the cursor
> the same way `NavigationSlot::selected_page` drives nav-page-row
> selection. Two new mutators on `BuilderSlot`
> (`select_schema_field` / `delete_selected_schema_field`) and two
> more (`select_signal_connection` /
> `delete_selected_signal_connection`) plug into two new POINTER_ROUTES
> rows (`schema-row` → schema cursor, `signal-connection-row` →
> connection cursor) and two new `BuilderService` commands
> (`schema.delete-selected-field`,
> `signals.delete-selected-connection`). The
> `schema_fields_json` / `connections_json` emitters now project
> `selected` / `show-delete` per row from the cursor, so the rows
> reveal their chevron / trash cluster exactly when their owning
> slot's cursor lands on them — identical UX shape to nav-page-row.
> Tests: 355 lib (+15 over the fourth wave), clippy clean, workspace
> `cargo test` green. B6 is now empty modulo
> `shell.dock-tab-bar` "close tab" / "+new tab" — still deferred
> until the dock workspace grows user-facing tab management.
>
> **Status update — 2026-05-11 (sixth wave).** Interactivity push.
> Three landings on top of the cursor-row plumbing:
>
> 1. **Boot seed for the panels with click-routes.** `seed.rs` now
>    hydrates `BuilderSlot::schema` (3 starter fields), 
>    `BuilderSlot::signal_connections` (2 starter rows), and
>    `NavigationSlot::pages` (3 pages with 2 edges). Without these,
>    the Data / Edit / Navigation workflow pages rendered the
>    schema-designer / signals-panel / nav-page-list as empty
>    strips, and the B6 click routes had no targets to fire
>    against.
> 2. **Field-edit click cycles select + steps number/integer.**
>    `handle_field_edit_click` grew arms for `select` (cycles
>    through the comma-joined `data-options` list, wrapping at
>    the end) and `number` / `integer` (step `+1`, clamped by
>    `data-min` / `data-max`). The select path threads
>    `FieldKind::Select(options)` through
>    `property_row_from_spec` → field-editor lowering as
>    `data-options`; number/integer surface their
>    `NumericBounds` as `data-min` / `data-max`. Three
>    property-row kinds (boolean, select, number/integer) are
>    now click-interactive end-to-end; text / color / file still
>    defer to the B4 follow-up.
> 3. **`CursorKey` trait + shared helpers** fold the three
>    "cursor + select-row + delete-cursored-row" pairs onto one
>    trait impl and three free helpers
>    (`select_cursor_row`, `delete_cursor_row`,
>    `iter_with_cursor`). Nav pages, schema fields, and signal
>    connections all delegate; the JSON emitters share one
>    iterator helper for the per-row `selected` / `show-delete`
>    derivation. Adding a fourth cursor-driven row is one
>    `impl CursorKey` + three short delegators on the owning
>    slot.
>
> Tests: 364 lib (+9 over the fifth wave), clippy clean,
> workspace `cargo test` green.
>
> **Status update — 2026-05-11 (seventh wave).** B4 landed.
> Text-input focus + drag-scrub close the field-editor
> interactivity loop:
>
> 1. **Text / color / file kinds** open a focus session
>    (`AppState::field_focus`) on click. A new
>    `FieldFocusService` registers ahead of every modal so
>    `Text` events route into `AppState::type_field_text`,
>    `Backspace` into `backspace_field`, `Enter` into
>    `commit_field_focus`, `Esc` into `cancel_field_focus`
>    (restores `original`). Every keystroke flushes the
>    bound prop through `set_node_prop` so the rendered
>    value stays in sync without a "commit on blur" pass.
>    Modifier-bearing keys (Ctrl+S, etc.) pass through so
>    global shortcuts still resolve while the user is mid-
>    edit. Clicking anywhere outside the field commits and
>    clears the focus via a pre-route blur pass in
>    `dispatch_event`.
> 2. **Number / integer kinds** open a `NumberDrag` session
>    on pointer-down. Pointer-move past a 3px threshold
>    scrubs the bound prop proportional to `dx/4`px-per-
>    unit, clamped to `data-min` / `data-max`. Pointer-up
>    without crossing the threshold falls through to the
>    legacy `+1 step` so single clicks still increment.
> 3. **Focus visual.** The properties-panel binding folds
>    `state.field_focus` into the row props
>    (`properties_panel_props_with`) so the focused row
>    carries `focused: true`; the field-editor lowering
>    paints an accent tint + `data-focused="true"` on that
>    row.
>
> Tests: 376 lib (+12 over the sixth wave), workspace
> `cargo test` 3199 green, clippy clean.
>
> **Status update — 2026-05-11 (docs cleanup, partial F).** Slint-era
> headers retired from `prism-core/src/lib.rs`,
> `prism-core/CLAUDE.md`, `prism-builder/src/lib.rs`,
> `prism-builder/src/prism_ui_emit.rs`, and
> `prism-builder/src/schemas.rs` — every doc string that opened with
> "Slint-era" or "Phase-2 target of the Slint migration" now reads
> against the post-cutover reality. The remaining
> `docs/dev/slint-migration-plan.md` cross-references in other
> crates (daemon / studio / cli) point to a still-extant historical
> doc and can stay until they have a `clay-migration-plan.md`
> equivalent or are themselves rewritten.

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

### B4. Text-input focus / IME / drag scrubber — LANDED
- **Status:** every shipped field-editor kind is now click-
  interactive. Boolean toggles, select cycles through
  `data-options`, number/integer step `+1` clamped by
  `data-min` / `data-max` on a release-without-drag, and drag
  scrubs the value proportional to pointer delta. Text /
  color / file rows open a text-input focus session
  (`AppState::field_focus`) that captures the keyboard:
  `Text` events append to the draft, `Backspace` pops one
  char, `Enter` commits, `Esc` cancels (restoring the
  original value). Every keystroke flushes the bound prop
  through `set_node_prop` so the renderer stays in sync.
  Clicking anywhere outside the focused field commits + clears.
  Modifier-bearing keys (Ctrl+S etc.) pass through so global
  shortcuts still work mid-edit.
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
  - `shell.dock-tab-bar` "close tab" / "+new tab" affordances —
    not authored yet; deferred until the dock workspace grows
    user-facing tab management.

  **Landed in the fourth wave (2026-05-11):**
  - `shell.app-card` carries `data-role="app-card"` and routes
    through `POINTER_ROUTES` to `WorkspaceSlot::set_active_app`.
    `WorkspaceSlot` gained an `active_app: Option<String>` cursor;
    the per-app skeleton swap that should eventually consume it is
    the D4 follow-up. The "create" card naturally falls through
    the route (no `data-app` attr).
  - `shell.nav-page-row` chevrons + trash route through
    `navigation.move-page-up` / `navigation.move-page-down` /
    `navigation.delete-selected-page`. `NavigationSlot` gained a
    `selected_page: Option<String>` cursor disjoint from
    `is_active`; `pages_list_json` emits `selected` / `show-delete`
    per row from it. Clicking a nav-page-row both moves the active
    flag (existing behaviour) and the chevron cursor.

  **Landed in the fifth wave (2026-05-11):**
  - `shell.schema-row` trash routes through
    `schema.delete-selected-field`. `BuilderSlot::schema` gained
    `selected_field: Option<String>` (stores the field `name` —
    `SchemaField` has no separate id and the name is unique per
    schema); the lowering threads the cursor key through a new
    `field-id` prop that surfaces as `data-target-id`. A new
    POINTER_ROUTES row (`schema-row` → `select_schema_field`)
    moves the cursor on click; `schema_fields_json` emits
    `selected` / `show-delete` per row from it.
  - `shell.signal-connection-row` trash routes through
    `signals.delete-selected-connection`. `SignalConnection` gained
    an `id: String` field (mirrors `prism_builder::Connection::id`)
    and dropped its per-row `selected: bool` — selection is now
    derived from the new `BuilderSlot::selected_connection`
    cursor, matching the nav-page-row shape. New `connection-id`
    prop surfaces as `data-target-id`; matching POINTER_ROUTES row
    moves the cursor on click; `connections_json` emits
    `selected` / `show-delete` from the cursor.
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
