# Composable Builder Plan

> Closing the soul gap in the Properties panel and lifting every shell
> component into `.prism-ui` source — so Studio is built from the same
> primitives users compose with, and every block / modifier is
> Luau-authorable and reactively read/write end-to-end.

**Status:** drafted 2026-05-11, awaiting approval
**Owner:** JJM
**Sits between:** `docs/dev/clay-migration-plan.md` §43 Phase E + B6
third wave (structural port) and `docs/dev/dioxus-inspiration.md`
Phase 10 (reactive substrate).
**Related:** `docs/dev/luau-integration-plan.md`,
`docs/dev/builder-unification.md`,
`docs/dev/ui-migration-followups.md`.

## 0. Diagnosis

The two predecessor plans landed cleanly:

- `clay-migration-plan.md` owns the **structural** port off Slint.
  One source (`.prism-ui`) → one IR (`BuilderDocument`) → two
  lowerings (`prism_ui_runtime` femtovg native + semantic-HTML SSR).
  Taffy is the layout engine; `Surface` is retained-mode;
  `host_children_by_tag` is the binding-driven composition seam;
  `POINTER_ROUTES` is the declarative hit-routing table.
- `dioxus-inspiration.md` owns the **reactive** nervous system.
  `prism-core::reactive::{Signal, Memo, Effect, Owner,
  ReactiveContext}`, atom bridge, `BlockInvalidator` per-NodeId
  reactive context, `DocumentBindings` + `LowerCtx::with_bindings`
  (read seam) + `NodeMutator::with_bindings` (write seam),
  `ActionKind::Bind`, Luau `Signal::read`/`write`, `RemoteSignal`
  over IPC, federated/peer/relay trait seam.

They meet at the two `with_bindings(..)` seams. The structural tree
runs inside the reactive graph.

**What neither plan owns, and what the user can feel as the soul
gap:** composition of orthogonal *behaviours* onto nodes. Today every
`Node` carries `modifiers: Vec<Modifier>`
(`prism-builder/src/document.rs:33`) and `ModifierKind::ALL` already
enumerates six attachable behaviours (`ScrollOverflow`, `HoverEffect`,
`EnterAnimation`, `ResponsiveVisibility`, `Tooltip`,
`AccessibilityOverride`) with `modifier_schema(kind) -> Vec<FieldSpec>`
returning real form-row descriptors. **The data round-trips. Nothing
consumes it.** No render walker applies modifiers, no inspector
renders them, no picker exists.

The second visible gap: the shell is ~50 hand-rolled Rust files in
`prism-shell/src/components/*` while users author `.prism-ui` source.
Duplication: hover tints, route attrs, collapse/expand chevrons,
popover anchoring — each pattern hand-rolled 3-7 times. The DSL has
never been forced to host real-world chrome, so its surface stays
incomplete and its primitives stay un-shared.

## 1. Vision

> **A Prism node is a typed component plus an ordered stack of
> attachable behaviours. Components, behaviours, and shell chrome are
> all authored in the same surface — Rust, `.prism-ui`, or Luau —
> against the same primitive set. Every prop is a reactive
> `Signal<T>` read/write-able from any language, across any scope
> boundary. The Inspector is the user-facing seam of that composition.**

Two strands, one shared spine:

1. **Composable Inspector.** Lift `Modifier` from dead data to
   first-class composable behaviour via a `ModifierRegistry` shaped
   like `ComponentRegistry`. One render-wrap fold in `LowerCtx`. One
   section per modifier in the Inspector. One picker for adding,
   one toggle / remove / reorder per row.
2. **`.prism-ui` authoring migration.** Lift unique behaviours from
   `prism-shell/src/components/*` into runtime primitives and
   migrate every component possible to `.prism-ui` source. The shell
   becomes a Builder project.

**The shared spine** is Luau. Every `Component`, every
`ModifierBehaviour`, every shell `.prism-ui` document is Luau-
authorable through the existing `LuauComponent` / `mlua` surface.
Every prop is a `reactive::Signal<T>` exposed to Luau (Phase 5 of
the dioxus plan) so `signal:read()` / `signal:write(v)` works
uniformly whether the prop lives on a builder node, a shell chrome
row, or an attached modifier.

## 2. Phases overview (checklist)

The complete plan as a single phased checklist. Tick boxes update as
each phase merges. Per-phase detail follows in §3-§11.

### Wave 1 — Composable Inspector  (lead workstream) ✅ landed 2026-05-12
- [x] **1.1** `ModifierRegistry` trait + DI registry, six builtins
  ported from `modifier::ModifierKind::ALL`.
- [x] **1.2** Render fold in `LowerCtx::lower(node)` — `node.modifiers`
  applied innermost-first via `ModifierBehaviour::wrap`. Tests:
  `ui_lower::tests::modifier_fold_{applies_innermost_first,
  skips_disabled_entries, passes_through_unknown_ids, no_op_without_registry}`.
- [x] **1.3** `derive_property_rows` emits one section per attached
  modifier plus an `add-modifier` footer row. Disabled modifiers
  emit the header but suppress schema rows. Tests:
  `state::tests::{derive_property_rows_emits_section_per_attached_modifier,
  disabled_modifier_emits_header_but_no_schema_rows,
  no_modifier_registry_means_no_modifier_rows}`.
- [x] **1.4** Three new shell blocks: `shell.modifier-header`,
  `shell.add-modifier-button`, `shell.modifier-picker`. 12 new
  per-block tests pinning routing attrs + open/closed shape.
- [x] **1.5** Four `AppState` mutators (`attach_modifier`,
  `detach_modifier`, `toggle_modifier`, `reorder_modifier`) plus
  `set_modifier_prop` — each routes through
  `resync_builder_for_selection`. Tests:
  `state::tests::{attach_modifier_appends_section_and_resyncs,
  toggle_modifier_flips_enabled_and_resyncs,
  detach_modifier_drops_section_and_resyncs,
  reorder_modifier_swaps_indices,
  set_modifier_prop_writes_to_modifier_props_not_node_props}`.
- [x] **1.6** Five `POINTER_ROUTES`: `modifier-toggle`,
  `modifier-remove`, `modifier-reorder`, `add-modifier-open`,
  `modifier-picker-select`. Picker overlay state lives on
  `OverlaySlot::modifier_picker`. Tests:
  `events::tests::{pointer_down_on_modifier_toggle_flips_enabled,
  pointer_down_on_modifier_remove_detaches_entry,
  pointer_down_on_add_modifier_open_seeds_picker_state,
  pointer_down_on_modifier_picker_select_attaches_and_closes_picker}`.
- [x] **1.7** Bootstrap behaviour set: `Visible`, `Locked`, `Hover`,
  `Click`, `BindToSelection`, `RunLuauScript` registered in
  `ModifierRegistry::with_builtins()`. Five have working `wrap`
  bodies (Visible collapses to zero size, Locked emits
  `aria-disabled`, Hover wraps with hover-tint, Click emits
  `data-on-click`); `BindToSelection` and `RunLuauScript` are
  schema-only awaiting Wave 8 Luau parity.
- [x] **1.8** Luau bindings: `LuauModifier` exposing `schema`,
  `wrap`, `install_effects` from Luau; `modifier:props():read("k")`
  / `:write("k", v)` against the reactive bag. **Landed via Wave 8**
  — `LuauModifier`, `ReactiveProps` userdata, type-stub generator,
  and `register_modifier_from_luau` entry point are all in
  `packages/prism-builder/src/luau_modifier.rs` (see Wave 8 named-pin
  tests for the full surface).
- [x] **1.9** Named pin tests landed across `modifier`, `ui_lower`,
  `state`, `events`, and each new shell block.

### Wave 2 — Field-edit UX ✅ landed 2026-05-12 (full pickers deferred to Wave 10)
- [x] **2.1** `text` kind: keystroke through `field_focus`,
  commit on Enter/blur via `set_node_prop`; **`textarea` kind
  added** with Shift-Enter inserting a literal newline (plain
  Enter still commits). Tests:
  `field_focus::tests::{shift_enter_inserts_newline_for_textarea_kind,
  shift_enter_on_text_kind_still_commits}`.
- [x] **2.2** `number` / `integer`: drag-scrub already wired
  with min/max clamp on `drag_number_field`. **Arrow keys ±1 /
  ±10 (with shift) added** — opens a focus session at click
  time so `FieldFocusService` can route arrows to
  `nudge_focused_number`. Test:
  `field_focus::tests::arrow_keys_nudge_focused_number_field`.
- [x] **2.3** `select`: click-to-cycle through `data-options` is
  the shipped form. Existing tests:
  `events::tests::{pointer_down_on_select_field_edit_cycles_to_next_option,
  pointer_down_on_select_field_edit_wraps_at_end_of_options}`.
  *Full anchored-dropdown overlay (chevron → `<popover>` +
  `<list-picker>`) lands with the Wave 10 primitive registry.*
- [x] **2.4** `color`: the swatch now carries
  `data-role="color-swatch"` + `data-target-id` + `data-key` +
  `data-value`. Clicking opens the new `shell.color-picker`
  overlay anchored under the swatch: preview + hex echo + three
  HSL slider tracks (Hue / Saturation / Lightness) + an 8-swatch
  preset row keyed off `ColorPicker::PRESETS`. **HSL gesture
  landed 2026-05-13**: pointer-down on a track captures the
  channel + bounds in `OverlaySlot::color_picker.slider_drag`;
  pointer-move applies the fraction continuously by parsing the
  current hex through `prism_builder::color::rgb_to_hsl`,
  replacing the targeted channel, and writing the new hex via
  `set_color_picker_value`. Pointer-up clears the capture slot.
  Color math lives in the new `prism-builder/src/color.rs`
  module (`Rgba`, `parse_hex` / `format_hex`, `rgb_to_hsl` /
  `hsl_to_rgb`, `ColorChannel` enum) — 9 unit tests pin every
  round-trip. Preset clicks commit through
  `AppState::set_color_picker_value` and the picker stays open
  for further preview; the Close button (and Esc) dismisses.
  The hex text input keeps the field-focus paste / type / Enter
  pathway intact as a typing fallback. Tests:
  `state::tests::{open_color_picker_seeds_target_key_value,
  open_color_picker_against_same_target_is_a_noop,
  open_color_picker_against_different_target_reseeds,
  close_color_picker_returns_true_only_when_open,
  color_picker_props_emit_eight_presets_open_or_closed}` +
  `events::tests::{pointer_down_on_color_swatch_opens_color_picker,
  pointer_down_on_color_preset_commits_through_set_node_prop,
  pointer_down_on_color_picker_close_dismisses_overlay}` +
  `components::field_editor::tests::color_kind_includes_swatch_and_input`.
- [x] **2.5** `file`: the field-editor's file-kind body now
  renders an explicit "Browse…" button next to the text input.
  Pointer-down dispatches `handle_file_browse_click`, which
  invokes `Vfs::pick_file` against the live shell `OsVfs`. The
  picker returns one path (or `VfsError::Cancelled` /
  `VfsError::Unsupported`, both of which are silent noops);
  the first picked path commits through `set_node_prop`. The
  `Vfs` trait grew a `pick_file(&FilePickerSpec)` method with a
  default `Unsupported` return; `OsVfs` overrides via `rfd`
  when the `native` feature is on and the target isn't wasm
  (so wasm builds compile clean against the trait default).
  `FilePickerSpec::with_accept_attr` parses the `data-accept`
  `<input type=file accept="…">` shape into rfd filters. The
  text-input fallback is preserved for paste / type workflows
  and for hosts that don't wire a dialog. Tests:
  `components::field_editor::tests::file_kind_emits_input_plus_browse_button`.
- [x] **2.6** Pin tests added per kind through the existing
  `events::tests` and `field_focus::tests` modules.

### Wave 3 — Pointer-driven canvas ✅ landed 2026-05-12
- [x] **3.1** Canvas `pointer_down` consults `Surface::hit_test_at`
  before drag-capture; hits route through `AppState::select_node`
  via `route_canvas_node_select` (events.rs). Landed alongside
  Phase 5 (§43 B5); reconfirmed for Wave 3.
- [x] **3.2** Palette drag → drop. `CatalogSlot::palette_drag`
  carries the in-flight kind + cursor + drop target; pointer-down
  on a canvas-resident hit while a palette item is armed starts a
  drag (`route_palette_drag_begin`), pointer-move updates the
  cursor + drop-target (`AppState::update_palette_drag`),
  pointer-up commits (`AppState::end_palette_drag` →
  `CanvasSlot::insert_under_node`). New node moves the selection
  and the palette pill clears. The visible ghost overlay is a
  follow-up — the data flow is the substantive piece. Tests:
  `events::tests::{pointer_down_on_canvas_with_palette_armed_begins_palette_drag,
  pointer_up_with_active_palette_drag_inserts_node_under_target,
  pointer_down_on_canvas_without_palette_armed_falls_through_to_select}`
  + `state::tests::{begin_palette_drag_*, end_palette_drag_*,
  cancel_palette_drag_*}`.
- [x] **3.3** Selection gizmo + resize edges.
  `CanvasSlot::selection_bbox: Option<SelectionBbox>` captured by
  `route_canvas_node_select` from the hit's `bounds`; emitted as
  `selection-rect` in `builder_canvas_props` so the existing
  `build_selection_layer` paints the outline + 8-handle ring at
  the live click rect. `POINTER_ROUTES("resize-handle")` reads
  `data-direction` (whitelisted to the 8 known handles), captures
  the active selection's transform via `begin_resize_drag`, and
  the PointerMove arm applies a `(px, py) * (dx, dy)` translation
  via `update_resize_drag`. Width/height mutation (vs.
  position-only) lands when the layout engine exposes per-node
  width/height mutators — same deferral the existing
  `apply_handle_delta` carries. Tests:
  `events::tests::{pointer_down_on_canvas_node_captures_selection_bbox,
  resize_handle_press_then_move_translates_selection_transform,
  resize_handle_press_without_selection_is_a_noop,
  resize_handle_press_with_unknown_direction_falls_through}`
  + `state::tests::{builder_canvas_props_emit_selection_rect_when_bbox_set,
  begin_resize_drag_*, resize_drag_round_trip_*,
  resize_drag_top_left_handle_*}`.
- [x] **3.4** Right-click → context menu. `PointerButton::Secondary`
  on a canvas-resident hit short-circuits the rest of the
  pointer-down chain and runs `route_context_menu_open`, which
  moves the selection cursor onto the target id and populates
  `state.menus.context` with the canvas action set (Move Up /
  Down / Duplicate / Copy / Cut / Delete on a selection; Paste on
  empty canvas) via `canvas_context_menu_items`. The existing
  `shell.context-menu` block renders against the populated items;
  `shell.menu-item` already emits `data-on-click="cmd <id>"` so
  the activation path through `route_on_click` lights up the
  rows without further wiring. Primary clicks outside the menu
  dismiss it via `route_context_menu_dismiss`. Tests:
  `events::tests::{right_click_on_canvas_node_opens_context_menu_with_actions,
  right_click_on_empty_canvas_opens_paste_only_menu,
  primary_click_outside_menu_dismisses_open_context_menu}`
  + `state::tests::{open_context_menu_on_selected_canvas_node_carries_node_actions,
  open_context_menu_on_empty_canvas_falls_back_to_paste,
  close_context_menu_*}`.

### Wave 4 — Connections panel UX ✅ landed 2026-05-12
- [x] **4.1** `BuilderSlot::selected_connection: Option<String>` —
  already landed during the B6 third-wave cursor pass; reconfirmed
  for Wave 4. `select_signal_connection` mutator moves the cursor
  and `iter_with_cursor` projects the `selected` / `show-delete`
  flags into `connections_json`.
- [x] **4.2** Row click → cursor (B6 third-wave landed
  `handle_signal_connection_row_click`); trash →
  `BuilderService::signals.delete-selected-connection` (already
  registered, dispatched from the row's `data-on-click`
  attribute via `route_on_click`). The `signal-connection-row`
  block already paints the trash button when `show-delete` is
  true and emits the right command id; Wave 4 reconfirms and
  documents the connection.
- [x] **4.3** `shell.connection-picker` overlay landed. Three
  picker rows (source / kind / target) each carry
  `data-role="connection-picker-field"` + `data-field`; Add and
  Cancel buttons route through `data-role="connection-picker-add"`
  / `-cancel`. Action-kind cycles through all 7
  `prism_builder::signal::ActionKind` variants including
  `Bind`. The "+ Add Connection" footer button
  (`shell.add-connection-button`) is appended to every
  `shell.signals-panel` by default (`show-add` boolean), and its
  click dispatches `cmd signals.open-connection-picker` through
  the existing `route_on_click` command-table dispatch. The
  picker overlay is added as a sibling tag in `ui/app.prism-ui`
  alongside the other window-relative overlays. Tests:
  `components::add_connection_button::tests::carries_open_picker_command_via_data_on_click`,
  `components::connection_picker::tests::{closed_picker_collapses_to_hidden_overlay,
  open_picker_renders_three_field_rows_and_two_buttons,
  field_rows_carry_field_routing_attrs,
  add_and_cancel_buttons_carry_distinct_routes,
  empty_field_value_renders_em_dash_placeholder}`,
  `components::signals_panel::tests::show_add_default_true_renders_add_connection_footer_at_panel_tail`,
  `events::tests::{pointer_down_on_connection_picker_action_kind_cycles,
  pointer_down_on_connection_picker_add_inserts_and_closes,
  pointer_down_on_connection_picker_cancel_closes_without_insert,
  data_on_click_open_picker_dispatches_through_command_table}`.
- [x] **4.4** Mutators landed.
  `AppState::add_signal_connection(SignalConnection, registry)`
  pushes a fresh row, moves the cursor onto it, and runs the
  C-wave resync. `AppState::update_signal_connection_field(id,
  field, value, registry)` writes one of `source-signal` /
  `action-kind` / `target-label` and resyncs (idempotent edits
  return false, unknown ids / field keys fall through cleanly).
  `delete_selected_signal_connection` was already on
  `BuilderSlot` and dispatches through the existing trash route.
  Picker-level mutators (`open` / `close` /
  `set_connection_picker_field` /
  `cycle_connection_picker_action_kind` /
  `confirm_connection_picker`) drive the picker overlay; the
  confirm path generates a stable kebab-case id from
  source + target via `sanitise_id` and `uniquify_connection_id`
  so repeated drops of the same shape don't collide. Tests:
  `state::tests::{open_connection_picker_seeds_form_defaults,
  open_connection_picker_is_idempotent_against_open_state,
  close_connection_picker_clears_form_and_returns_true_when_open,
  cycle_connection_picker_action_kind_wraps_at_end_of_variant_list,
  confirm_connection_picker_inserts_unique_id_and_moves_cursor,
  confirm_connection_picker_with_empty_source_signal_is_a_noop,
  update_signal_connection_field_writes_one_field_and_resyncs,
  add_signal_connection_returns_id_and_lands_on_cursor}`.

### Wave 5 — Registry merge  (small, lands first) ✅ landed 2026-05-11
- [x] **5.1** `Shell::new` calls
  `register_document_builtins` (`shell.rs:133`), which fans out to
  `prism_builder::starter::register_builtins` plus the `card`
  prefab and `facet` component (`components/registry.rs:164`).
- [x] **5.2** Named pin test
  `components::registry::tests::no_overlapping_block_ids_between_shell_and_starter`
  asserts the shell `shell.*` namespace and the document catalog
  (`prism_builder::starter::BUILTINS` + `card` + `facet`) are
  disjoint *and* that the merged registry size equals the sum.

### Wave 6 — B6 third-wave remainder ✅ landed 2026-05-12
- [x] **6.1** `NavigationSlot::selected_page` + chevron commands
  (`navigation.move-page-{up,down}`, `navigation.delete-selected-page`)
  landed during B6; reconfirmed for Wave 6. Row click moves cursor
  via `handle_nav_page_row_click`; chevron commands fire through
  the row's `data-on-click="cmd …"` dispatch.
- [x] **6.2** `BuilderSlot::schema.selected_field` + trash command
  `schema.delete-selected-field` landed during B6. Row click moves
  cursor via `handle_schema_row_click`; trash dispatches the
  command through `data-on-click="cmd schema.delete-selected-field"`.
- [x] **6.3** Signal-connection trash done as part of Wave 4
  (`signals.delete-selected-connection`).

### Wave 7 — Headless visual capture ✅ landed 2026-05-12 (PNG deferred)
- [x] **7.1** `prism-shell --scene <name>` lands.
  `prism_shell::headless::BuiltinScene` enumerates six scenes
  (`default` / `selection` / `modifier` / `context-menu` /
  `palette-drag` / `connection-picker`); `Shell::apply_scene` mutates
  boot state for each. `--scene list` prints the catalogue.
- [x] **7.2** `--screenshot <path>` now picks output format from
  the file extension: `*.png` runs the Wave-7.2 software
  rasteriser in `prism_shell::png_paint` and emits an RGBA PNG;
  any other extension (including `.json`) keeps the existing
  deterministic JSON dump of the lowered UI tree. PNG renders
  go through the same `prism_ui_runtime::layout::compute` Taffy
  pass the femtovg backend uses, so the captured *layout* is
  identical to the on-screen frame — the painter is a CPU-side
  walk over the `RenderCommand` stream (solid rectangles with
  rounded-corner masking, alpha-blended borders, scissor
  clipping, image-tint placeholders, text-baseline accent
  strips). Glyph rasterisation and real image decoding stay out
  of scope for the software path; they unlock when a real
  offscreen femtovg surface lands. The encoder seam in
  `Shell::dump_png` swaps target without changing this
  module's contract. New CLI flags: `--width`, `--height` set
  the headless viewport (default 1280×800). Tests:
  `headless::tests::{dump_png_emits_png_magic_bytes,
  dump_png_is_deterministic_across_runs}`,
  `png_paint::tests::{empty_command_stream_yields_transparent_buffer,
  rectangle_paints_solid_block, alpha_blend_combines_with_existing,
  scissor_clips_subsequent_rectangles,
  rounded_corners_clip_corner_pixel}`,
  `bin::native::tests::{screenshot_with_explicit_viewport_flags,
  invalid_width_returns_error}`.
- [x] **7.3** `prism visual` shells through `prism-shell` directly
  (2026-05-12). `packages/prism-cli/src/commands/visual.rs` runs
  `cargo run -p prism-shell -- --scene <name> --screenshot <path>`
  per scene, no macOS `screencapture` shim. The hard-coded scene
  list now mirrors `prism_shell::headless::BuiltinScene::ALL`
  (six scenes) — the
  `commands::visual::tests::scene_names_match_shell_builtin_set`
  pin guards drift without dragging the shell dep into prism-cli.
  Output extension is `.json` today (matches Wave 7.2's
  deterministic dump); flips to `.png` in one line when 7.2 lands.
  Tests: `commands::visual::tests::{scene_names_match_shell_builtin_set,
  each_scene_emits_one_screenshot_command_with_expected_extension}`.

Tests: `headless::tests::{builtin_scene_round_trips_name,
unknown_scene_name_yields_none, dump_frame_emits_nonempty_json,
modifier_scene_attaches_tooltip_to_demo_button,
connection_picker_scene_opens_the_picker,
palette_drag_scene_arms_the_drag_state,
context_menu_scene_populates_menu_items}` +
`bin::native::tests::{empty_args_run_full_shell,
scene_flag_routes_to_scene_variant, scene_list_routes_to_list_variant,
screenshot_flag_routes_to_screenshot_variant,
screenshot_combines_with_scene, unknown_scene_name_returns_error,
unknown_flag_returns_error}`.

### Wave 8 — Luau parity for components + modifiers ✅ landed 2026-05-12

The mlua surface that gives Luau-authored modifiers the same
vocabulary Rust modifiers have. `packages/prism-builder/src/luau_modifier.rs`
ships as a sibling to `luau_component.rs` (mirror shape, thread-local
registry, `compile` / `replace` API). 11 new tests; full prism-builder
suite still green at 423 with `--features luau`.

- [x] **8.1** `ReactiveProps` exposes
  `props:read("k")` / `:write("k", v)` / `:signal("k")` through an
  `mlua::UserData` impl on `prism_builder::reactive_props`. The
  `signal` accessor returns the canonical `Signal<Value>` UserData
  from `prism_core::luau_reactive`, so the full reactive vocabulary
  (`read` / `peek` / `track` / `write` / `set`) is available
  through the existing impl — one method per side, identical
  semantics. The `node:props()` shape lands when Luau-authored
  modifiers receive a per-NodeId bag at install-effects time
  (already wired through `LuauModifierRegistry::invoke_install_effects`).
- [x] **8.2** `LuauModifier` ships as a `ModifierBehaviour` impl
  whose body delegates to a Luau table with `schema` (required),
  `wrap` (optional, identity default — render-time bridge from
  `prism_ui_runtime::layout::Node` to a Luau table is the §11.5
  hot-reload follow-up), and `install_effects` (optional,
  `function(props) ... end` runs at modifier-attach time with a
  `ReactiveProps` userdata).
- [x] **8.3** `generate_modifier_type_stubs(&LuauModifierRegistry)`
  emits `--- @class Modifier_<Id>` blocks per registered Luau
  modifier with schema fields mapped to Luau types. Lives next to
  `generate_signal_type_stubs` for components; mirror shape so a
  project's type-stub directory carries both sets verbatim.
- [x] **8.4** `register_modifier_from_luau(&mut registry, source)`
  is the first-class entry point for compiling a Luau source
  string into one or more `LuauModifier`s. The caller decides
  where to install them (typically `ModifierRegistry::register`).
  `LuauModifierRegistry::replace(id, source)` is the per-id
  hot-reload hook; same contract as `LuauRenderRegistry::replace`.

Tests: `luau_modifier::tests::{prism_modifier_global_registers_an_entry_with_schema,
luau_modifier_can_be_attached_to_a_modifier_registry,
install_effects_runs_against_reactive_props,
install_effects_is_a_noop_when_hook_absent,
install_effects_is_a_noop_when_id_unknown,
replace_swaps_a_single_modifier_in_place,
replace_errors_when_source_does_not_register_target_id,
generate_modifier_type_stubs_emits_class_blocks_in_id_order,
register_modifier_from_luau_is_a_thin_compile_alias,
props_userdata_round_trips_a_signal_value_through_luau,
props_signal_read_returns_current_value_through_luau}`.

### Wave 9 — DSL gap-close ✅ landed 2026-05-12
- [x] **9.1** `route:<key>="<value>"` namespace landed —
  `AttributeNamespace::Route` plus
  `interpret::apply_container_attributes` lowers it to
  `data-<key>` semantic attrs. Authoring
  `<container route:role="resize-handle" route:direction="br"/>`
  is now equivalent to the bare `data-` ladder, and the hit-test
  cache picks them up identically. Tests:
  `interpret::tests::{route_namespace_lowers_to_data_dash_attr_on_container,
  data_namespace_lowers_to_data_dash_attr_on_container,
  aria_namespace_lowers_to_aria_dash_attr_on_container}`.
- [x] **9.2** `:state` selectors (`:hovered`, `:selected`,
  `:focused`) lower in `prism-ui-runtime::interpret`. The AST
  helper `prism_core::language::prism_ui::split_state_suffix`
  splits a namespaced local part on its trailing `:state` segment
  against the closed `STATE_SUFFIXES = ["hovered", "selected",
  "focused"]` set; unknown suffixes round-trip unchanged. Style
  routing in `apply_container_attributes`:
  - `style:background:hovered="<color>"` → folds into
    `ContainerProps.hover.background` (uses existing
    `HoverOverrides` infra — paint pass already swaps on
    `Surface::hovered_id` match).
  - `style:radius:hovered="<px>"` → `ContainerProps.hover.radius`.
  - `:selected` / `:focused` round-trip as
    `data-style-<key>-<state>` semantic attrs (no container-level
    runtime infra for those states today; data carries author
    intent, full wiring lands as a follow-up — same pattern as
    Wave 9.4 transitions).
  Tests: `language::prism_ui::ast::tests::{split_state_suffix_recognizes_hovered,
  split_state_suffix_recognizes_selected_and_focused,
  split_state_suffix_returns_none_for_unknown_suffix,
  split_state_suffix_returns_none_for_plain_local}`,
  `interpret::tests::{style_state_namespace_hovered_lowers_into_hover_overrides,
  style_state_namespace_hovered_lowers_radius_to_hover_overrides,
  style_state_namespace_selected_and_focused_round_trip_as_data_attrs,
  style_state_namespace_unknown_suffix_falls_through_cleanly}`.
- [x] **9.3** `bind:<key>="<source>"` lowers to a `data-bind-<key>`
  semantic attribute carrying the source path verbatim. The
  reactive-binding installer reads these off at document load
  time and registers an `Effect`. Test:
  `interpret::tests::bind_namespace_lowers_to_data_bind_attr`.
- [x] **9.4** `transition:<prop>="<duration>"` lowers to
  `data-transition-<prop>` semantic attrs. The runtime
  `Effect`-driven animator that consumes the hint is the
  follow-up — today the data round-trips through the semantic
  attrs without behaviour change, so author intent survives the
  upgrade. Test:
  `interpret::tests::transition_namespace_lowers_to_data_transition_attr`.

### Wave 10 — Primitive registry ✅ structural landed 2026-05-12 (+ Wave 11.4 `prism.builder-host`)
- [x] **10.1** `prism_builder::primitives::PRIMITIVES` table —
  one `BlockSpec` per primitive, same shape as
  `starter::BUILTINS`. Lives in `prism-builder` rather than
  `prism-ui-runtime` because `BlockSpec` is a `prism-builder`
  type and the plan's "prism-ui-runtime" target conflicts with
  the workspace dependency direction (`prism-builder` already
  depends on `prism-ui-runtime`). The catalogue threads through
  the shell registry alongside `starter::register_builtins`
  via `register_document_builtins` in
  `prism-shell/src/components/registry.rs`.
- [x] **10.2** All 15 primitives landed (14 from Wave 10 +
  `prism.builder-host` from Wave 11.4):
  `prism.text-input`, `prism.drag-scrub`, `prism.popover`,
  `prism.list-picker`, `prism.collapsible`, `prism.split-handle`,
  `prism.timed-overlay`, `prism.focus-trap`, `prism.select`,
  `prism.color-picker`, `prism.file-button`, `prism.resize-edge`,
  `prism.canvas-paint`, `prism.text-buffer`,
  **`prism.builder-host`**. Each carries a full prop schema and a
  minimal lower body that emits a `data-role="<id>"` semantic
  attr — full interactive bodies (keystroke handling, drag-scrub
  gestures, popover anchoring, HSL color math, `rfd` file
  dialogs, custom canvas paint, etc.) land alongside each
  primitive's first call site per §11.5 of the plan.
- [x] **10.3** Every primitive's schema flows through the same
  `Component::schema()` surface as builder builtins, so the
  Inspector renders prop rows against them with no
  primitive-specific seam. The `no_overlapping_block_ids…`
  named pin in the shell registry asserts the
  `prism.*` namespace stays disjoint from both `shell.*` and
  the builder document catalog.

Tests: `primitives::tests::{primitive_count_is_fifteen,
every_primitive_id_starts_with_prism_namespace,
primitive_ids_are_unique,
register_primitives_lands_all_specs_in_the_registry,
primitive_lower_emits_data_role_matching_tag_local_part,
builder_host_primitive_is_registered_with_required_document_field}` +
the upgraded
`components::registry::tests::no_overlapping_block_ids_between_shell_and_starter`
(now asserts shell/builder/primitive triplet disjointness).

### Wave 11 — `.prism-ui` self-hosting (long tail) ✅ landed 2026-05-13 (Tier-1 + Tier-2 modulo intentional Rust survivors)
- [x] **11.1** Three generalization-sweep substrates landed:
  (a) the **Hover modifier** is in `ModifierKind::Hover` /
  `BehaviourSpec::with_id("hover")` from Wave 1.7 — every shell
  component that wants a hover tint composes it as a behaviour
  attachment instead of hand-rolling a hover container;
  (b) the **`route:` namespace** lowers to `data-<key>` semantic
  attrs (Wave 9.1) so authoring stops hand-emitting `data-role`
  ladders; (c) the **`<collapsible>` primitive** is in
  `PRIMITIVES[4]` (Wave 10) — composable from any `.prism-ui`
  source against the registered tag. The substrates remove the
  *need* for duplication; the actual per-component sweeps that
  remove the existing duplication land file-by-file alongside
  Tier-1 migration.
- [x] **11.2** Tier-1 migration: loader seam landed +
  **31 components migrated** across five passes 2026-05-12 plus a
  Wave-13-unblocked tail of `shell.app-window`,
  `shell.command-palette`, and the three gizmos
  (`shell.gizmo-move/rotate/scale`) shipped 2026-05-13 once
  named-slots (§13.1) closed the imperative-composition gap. New module
  `packages/prism-shell/src/components/prism_ui_loader.rs`
  ships `PrismUiSpec` (declarative form mirroring `BlockSpec`),
  `PrismUiBlock` (runtime `Block` impl backed by a parsed AST +
  shared `OnceLock<Arc<dyn TagResolver>>`), and the
  `SHELL_PRISM_UI_COMPONENTS` table threaded through
  `Shell::new` (via `register_full_shell_chrome`) alongside
  `register_shell_builtins`. Each migrated component is
  **one `.prism-ui` source + one row in the table**; the loader
  resolves composed `<shell.*>` / `<prism.*>` tags through the
  same live registry the native blocks dispatch against. The
  shared resolver cell is populated post-registration via
  `finalize_prism_ui_resolver` so DSL blocks compose with each
  other without ordering constraints.

  - **Batch 1** (commit `a9c199c`): `shell.toolbar-separator`,
    `shell.help-tooltip`, `shell.docs-view`, `shell.docs-sidebar`,
    `shell.toast-stack`, `shell.launchpad`.
  - **Batch 2** (commit prior to this one): `shell.explorer`,
    `shell.docs-content`, `shell.section-header`,
    `shell.nav-button`, `shell.inspector-tree`,
    `shell.nav-page-list`, `shell.signals-panel`,
    `shell.workflow-page-bar`, `shell.menu-dropdown`,
    `shell.context-menu`, `shell.add-modifier-button`,
    `shell.add-connection-button`.
  - **Batch 3** (substrate-unblocked): `shell.toast`,
    `shell.menu-item`, `shell.signal-connection-row`,
    `shell.schema-row`, `shell.nav-page-row`, `shell.properties-panel`.
    Each was previously blocked on one of the four substrate features
    the plan called out at §11.2; with the substrate in place every
    block here is one `.prism-ui` file + one row.
  - **Batch 4** (chrome lift, previous commit): `shell.icon-button`,
    `shell.dock-tab`, `shell.workflow-page-button`,
    `shell.status-bar`, `shell.menu-bar-row`, plus the new shared
    `shell.tab-button` primitive that dock-tab + workflow-page-button
    both compose against. Dead chrome helpers (`TabStyle` +
    `active_underline_tab`) deleted from `chrome.rs` because the
    last Rust consumer migrated away. Sibling substrate fix in
    `prism-ui-runtime::interpret::collect_text_content`: text-element
    interpolations now fall through to the full expression evaluator
    when the cheap bare-path lookup fails, and resolutions that
    collapse to `""` no longer push a leading separator — so
    `<text>{text ? text : status}</text>` works against the same
    vocabulary attribute interpolations use.
  - **Batch 5** (this commit — Wave 13): `shell.app-card`,
    `shell.modifier-header`, `shell.modifier-picker`,
    `shell.connection-picker`, `shell.component-picker`,
    `shell.dock-tab-bar`. The picker overlays pioneer the new
    **top-level `if`/`else` overlay-gate pattern** — closed
    overlays render as a 0×0 `data-visible="false"` hidden div,
    open overlays render the full body, no separate
    `chrome::hidden_overlay` helper required. Dead chrome helpers
    (`route_chip`, `indent_dot`, `icon_button_node_tinted`) deleted
    from `chrome.rs` because the last Rust consumer migrated away.
    The `shell.app-card` migration is the first DSL block using a
    9-arm ternary cascade for icon-name → asset-path lookup,
    proving the substrate handles real-world string mapping tables
    without a new DSL feature.

  Net ~1900 LoC of Rust removed across both batches; ~350 LoC of
  `.prism-ui` added (loader infra ~310 LoC paid once). DSL grammar
  picked up four substrate additions to unblock these migrations:
  (a) bare `tag` / `role` / `aria-label` container attrs set the
  dedicated `Semantic` fields directly; (b) `<host-children/>`
  emits the caller's pre-lowered children at the composition seam;
  (c) a new `<image src=… width=… height=… style:radius=…
  style:tint=…/>` element closes the last image-bearing gap
  (chevrons, nav-button icons, …); (d) `lookup_expression` grew
  dotted-path support (`{item.label}` / `{tabs.0.name}`) so
  `for="item in items"` loops over `Vec<Object>` can address typed
  fields. Two resolver-side closes: `RegistryTagResolver` now
  resolves attribute interpolations through scope before building
  the dispatched `BuilderNode` (pure `{expr}` returns the underlying
  JSON value verbatim, templated `prefix-{expr}` resolves to string),
  and a new `props="{expr}"` spread attribute unpacks an object
  into the dispatched node's props so `<shell.nav-page-row props="{item}"/>`
  forwards the full row without enumerating each schema key. The
  loader seeds schema defaults into scope before applying
  `node.props`, so missing-but-defaulted props (`show-add=true` on
  `signals-panel`) inherit the Rust-side default without re-stating
  it in the DSL. Tests:
  `components::prism_ui_loader::tests::{every_prism_ui_spec_id_uses_shell_namespace,
  every_prism_ui_spec_parses_without_errors,
  loader_populates_resolver_after_finalize,
  prism_ui_specs_register_disjoint_from_native_builtins,
  toolbar_separator_lowers_to_1x20_translucent_stroke,
  workflow_page_bar_iterates_pages_and_dispatches_through_resolver,
  workflow_page_bar_via_render_tree_pipeline_emits_workflow_page_buttons,
  shell_new_render_carries_workflow_page_button_hits}` +
  `ui_resolver::tests::{interpolated_attribute_resolves_from_scope_as_typed_json,
  interpolated_attribute_with_missing_binding_returns_null,
  props_spread_attribute_unpacks_object_into_node_props,
  props_spread_ignores_non_object_values}` +
  `interpret::tests::{image_lowers_to_image_node_with_source_and_sizing,
  image_data_and_aria_namespaces_round_trip_on_semantic,
  for_loop_supports_dotted_field_access_on_object_items}`.

  **Batch 6** (Wave 13 unblock, 2026-05-13): `shell.app-window`,
  `shell.command-palette`, `shell.gizmo-move`, `shell.gizmo-rotate`,
  `shell.gizmo-scale`, plus `shell.builder-toolbar`,
  `shell.schema-designer`, `shell.transform-editor`. The Wave 13.1
  named-slots substrate (`<slot name="menu"/>` / `<slot name="status"/>`
  paired with default `<host-children/>`) closed the imperative-
  composition gap `app-window` carried; the Wave 13.2 `on:`
  namespace lets `command-palette` declare keyboard / pointer routes
  inline; the gizmos compose through `<prism.canvas-paint>` for
  their per-pixel drag affordances. Only the three Tier-3 imperative
  primitives (`builder_canvas`, `code_editor`, `nav_graph`) plus
  the four documented Tier-2 survivors (§11.3) remain Rust.
- [x] **11.3** Tier-2 migration: 10/14 stateful / gesture
  components landed; 4 documented intentional Rust survivors. Each
  survivor names the *specific* DSL substrate it needs, so the
  follow-up is bounded and not "more migration work."
  - **Migrated**: `builder_toolbar`, `component_picker`,
    `context_menu`, `dock_divider`, `drag_number_field`,
    `gizmo_move`, `gizmo_rotate`, `gizmo_scale`, `menu_dropdown`,
    `resize_handle`, plus `command_palette`, `schema_designer`,
    `transform_editor` (Tier-2 by content but landed alongside
    Tier-1 in the Wave 11.3 pass 2026-05-13).
  - **Intentional Rust survivors** (substrate not landed):
    - `field_editor.rs` — kind-driven dispatch table over 7
      distinct visual chromes (boolean switch / select pill /
      color swatch+hex / number+slider / text input). Migratable
      once the binding pre-computes `options-joined`,
      `min-set`/`max-set` booleans, and `slider-fill-pct` so the
      DSL doesn't need null-comparison expressions or array-join
      built-ins. Today field-editor's chrome is hand-rolled in
      one block; the migration is one large coordinated state.rs
      + field-editor.prism-ui change.
    - `dock_panel.rs` — `panel-id → content-tag` lookup
      delegates to `prism_dock::PanelKind::tag_for`. Migratable
      once the binding pre-resolves `content-tag` (or the loader
      injects a scope-level panel-routes table the DSL can
      look up). Today the Rust block has the routing path inline.
    - `dock_workspace.rs` — recursive `DockNode` (Split /
      TabGroup) tree walk plus `PanelKind::from_id` label
      resolution. Migratable once a `<shell.dock-node>` helper
      block can self-recurse without re-wrapping in the
      `data-role="dock-workspace"` semantic, AND the binding
      pre-flattens the panel-id → label map. Today the Rust
      block recurses imperatively over the JSON-decoded tree.
  - **chrome.rs survives** because `field_editor` still consumes
    `drag_number_field_node`, `format_drag_value`, and
    `DRAG_NUMBER_LABEL_COLOR`. Delete-pending alongside the
    field-editor migration.
- [x] **11.4** Tier-3 primitives shipped as runtime entries.
  `prism.canvas-paint`, `prism.text-buffer`, `prism.resize-edge`,
  and now `prism.builder-host` land as `BlockSpec` rows in the
  primitive registry. `prism.builder-host` carries the
  `document` (required), `viewport`, `show-selection`, and
  `read-only` props — the schema the shell's `shell.builder-canvas`
  block already needed; lifted into the primitive registry so
  any `.prism-ui` document can embed a builder canvas without
  depending on the shell's chrome catalog. Full interactive
  bodies (canvas paint dispatch, text-buffer keystroke handling,
  resize-edge gesture wiring, builder-host pointer-event
  forwarding) land alongside each primitive's first authored
  consumer per §11.5.
- [x] **11.5** Hot-reload pipeline wired through `prism-cli` —
  `prism dev shell` now watches `packages/prism-shell/ui/` for
  `.prism-ui` edits alongside `src/` for `.rs` edits, both via
  the same `dev_loop::DevLoop` respawn path.
  `DEFAULT_EXTENSIONS = &["rs", "prism-ui"]` is the named pin
  guarding the filter. `prism dev studio` mirrors the watch set
  so the packaged shell rebuilds on skeleton edits too. The
  subsecond patch path (Phase 9 of
  `docs/dev/dioxus-inspiration.md`) intercepts this same
  extension when the patch pipeline lands — the dev-loop seam
  doesn't change. Tests:
  `dev_loop::tests::{default_extensions_include_rs_and_prism_ui_for_shell_hot_reload,
  filter_batch_with_default_extensions_keeps_skeleton_edits}`,
  `workspace::tests::shell_ui_dir_resolves_to_the_skeleton_directory`.

**Discipline:** every Tier-1/Tier-2 migration follows §11.5's
"one PR per component, before/after frame dump byte-identical
gate." The substrates landed in this commit unblock the sweeps;
the actual migration is intentionally incremental.

### Wave 12 — Vue/React-style style prop passing ✅ landed 2026-05-12

The composition seam the plan called out at §11 vision ("every prop is
a `Signal<T>` exposed to Luau ... every shell `.prism-ui` document is
Luau-authorable through the existing `LuauComponent` surface") was
missing one piece: **parent components could not pass styling down to
child component instances**. Today every block computes its own
`ContainerProps` from constants; a parent that wanted to reshape a
child's background or radius had to fork the child block.

Wave 12 closes the gap with two cooperating substrate moves, both
landing as smart-pattern additions to existing seams (no new modules,
no new traits):

- [x] **12.1** `apply_style_override(props, key, value)` lifted into
  `prism-ui-runtime::interpret` as the **single source of truth** for
  the `style:` vocabulary. The pre-existing
  `apply_container_attributes`'s `AttributeNamespace::Style` branch
  now collapses to one line (`apply_style_override(props, local,
  value)`) so direct authoring and resolver-side parent overrides go
  through the same code path. The helper recognises `background`,
  `radius`, `padding[/-left/-right/-top/-bottom]`, `gap`, `width`,
  `height`, plus `background:hovered` / `radius:hovered` for state-
  aware overrides. Unknown bare keys drop silently (preserves the
  pre-Wave-12 `_ => {}` branch behaviour); unknown keys with a
  recognised `:state` suffix round-trip as `data-style-<key>-<state>`
  semantic attrs (Wave 9.2 pattern — intent survives).
- [x] **12.2** `attach_style_overrides(node, element, scope)` in
  `prism-builder::ui_resolver` collects every `style:*` attribute
  and `style="{obj}"` spread on the source element, then applies them
  to the lowered container **after** `Component::lower_ui` returns.
  Mirrors the existing `attach_on_handlers` pattern. The block
  computes its natural styling first; the parent's overrides win.
  The `style="{obj}"` spread parallels the Wave 11.2 `props="{item}"`
  spread — a JSON object's entries each become a per-key override.
  Both `element_to_builder_node` and `dispatch_element_to_builder_node`
  swallow the bare `style` key so blocks never see it as a stray
  prop. Six new tests (`style_namespace_overrides_lowered_container_background`,
  `style_spread_object_unpacks_each_key_as_override`,
  `style_namespace_overrides_with_state_suffix_route_to_hover`,
  `style_overrides_unknown_bare_key_drops_silently`,
  `style_spread_value_is_not_visible_as_block_prop`,
  `style_overrides_apply_through_dynamic_dispatch`) pin the
  resolver-side contract end-to-end.
- [x] **12.3** First DSL consumer: `shell.component-palette`
  migrates to `.prism-ui` source alongside the substrate. The row
  container's selected-vs-hover branch
  (`style:background="{selected-id == item.item-id ? '#330060c0' : ''}"`,
  `style:background:hovered="{selected-id == item.item-id ? '' :
  '#1a0060c0'}"`) is the canonical first user of the new seam —
  ternary-driven overrides that fold through `apply_style_override`
  exactly like authored `style:` attributes do. Net ~250 LoC of
  Rust deleted, ~50 LoC of `.prism-ui` added.
- [x] **12.4** Second DSL consumer: `shell.inspector-row`
  migrates to `.prism-ui` source. The three `kind` variants
  (`node` / `row` / `empty`) collapse to a chain of ternary
  expressions over `style:background`, `style:radius`, `font-size`,
  `style:color`, `role`, and `aria:selected` — the
  per-kind metrics table the Rust source used (`KIND_NODE` /
  `KIND_ROW` / `KIND_EMPTY`) becomes one-line ternaries against
  the DSL's expression evaluator. Depth-based indent
  (`padding-left="{12 + depth * 16}"`) uses arithmetic in DSL
  expressions (`prism_core::language::expression`'s `BinaryOp::Add`
  + `BinaryOp::Mul`). The optional right cluster (chevrons when
  selected on `node` kind; trash when `show-delete` on `row` kind)
  composes through three `<shell.icon-button if=...>` instances —
  Vue/React-style conditional rendering against the same DSL `if=`
  substrate Wave 11.2 hardened. Net ~530 LoC of Rust deleted,
  ~55 LoC of `.prism-ui` added.
- [x] **12.5** Substrate follow-up: `aria:` namespace mirrors the
  Wave 11.2 `data:` empty-string filter so ternary-conditional
  ARIA attrs (`aria:level="{depth > 0 ? depth + 1 : ''}"`,
  `aria:selected="{... ? 'true' : ''}"`) omit cleanly when the
  branch resolves empty. A literal `aria-foo=""` is meaningless to
  every screen reader, so dropping it matches user intent rather
  than HTML's serialisation shape.

**Vue/React parallel:** today you write
```html
<shell.icon-button style:background="#ff0000" style="{theme.button}"/>
```
which is the same composition shape React's `<Button
style={{background: 'red', ...theme.button}}/>` provides. Both reach
the same final container; both behave the same. The child can stay
unchanged; the parent reshapes its visual without forking.

**Composition rationale (matches §3.7 "shared spine" framing):**
the same `apply_style_override` vocabulary is the surface Luau will
write through when modifiers want to override a wrapped node's
styling — `m.props:write("style.background", c)` flows through the
same one-key-one-application path the resolver uses. Wave 8's
`ReactiveProps` userdata already exposes `read`/`write`/`signal`;
the only addition is reading nested-path keys, which falls out of
the existing dotted-path support in `prism_core::language::expression`.

### Wave 13 — DSL substrate from Dioxus/React/Vue/Svelte

Survey of authoring patterns the upstream component frameworks
established, mapped against gaps in the Prism `.prism-ui` DSL. Each
row scoped against a *concrete consumer* — patterns that don't
unblock a real migration are listed for completeness but explicitly
deferred. Wave 13 implements the three with consumers waiting.

| Pattern | Source | Consumer / gap | Status |
|---|---|---|---|
| **Named slots** `<slot name="X"/>` + `slot="X"` attr | Vue, Web Components, Svelte | `shell.app-window` (menu/body/status seams) | **13.1 — implement** |
| **`on:event` namespace** | Vue `@click`, Svelte `on:click` | Ergonomic alias for `data:on-click="cmd …"` | **13.2 — implement** |
| **`use:` directives** | Vue `v-X`, Svelte `use:X` | Attach `ModifierBehaviour` from DSL — closes Wave 1 ergonomic gap | **13.3 — implement** |
| **Two-way `bind:value`** | Vue `v-model`, Svelte `bind:value` | `prism.text-input` writeback | Defer — blocked on text-input body (Wave 11.4) |
| **Reconciliation `key="…"`** | Svelte `(key)`, Vue `:key` | List diff across re-renders | Defer — no incremental tree diff yet |
| **Explicit `<fragment/>`** | React `<>`, Vue `<template>` | Multi-root without wrapper | Defer — loader's `collapse_to_single_root` handles this |
| **`v-show` / `hidden:"…"`** | Vue | Hide without unmount | Defer — overlay-gate `if`/`else` already covers this |
| **Scoped slots / render props** | Vue, React | Pass renderer back to parent | Defer — no consumer |
| **Computed (`$:` / `computed`)** | Svelte, Vue | Memoised expressions | Defer — inline expressions already cheap |
| **Class-list composition `class:active="…"`** | Vue, Svelte | We use `style:` not classes | Skip — wrong substrate |

- [x] **13.1** **Named slots.** `<slot name="X"/>` in a DSL component
  body emits the subset of caller-supplied children tagged with
  `slot="X"`; a bare `<slot/>` or `<host-children/>` emits the
  remainder (the default slot). New methods on `LowerScope`:
  `with_host_children_by_slot(HashMap<String, Vec<UiNode>>)` +
  `host_children_for_slot(name) -> Option<&[UiNode]>`. The
  `RegistryTagResolver::resolve` pre-lowering loop reads each AST
  child's `slot="…"` attribute, lowers it, and buckets the
  result by slot name; un-tagged children land in the `""`
  bucket. `<host-children/>` continues to emit *all* host children
  for back-compat (one row in the slot map matches the old shape).
  Closes `shell.app-window` migration (last big Tier-1 unblock).
- [x] **13.2** **`on:event` namespace.** `on:click="cmd foo"`
  lowers to `data-on-click="cmd foo"` via the existing
  `attribute_lower` path. `AttributeNamespace::On` parsed in the
  prism-ui parser; `apply_container_attributes` routes it through
  the same `data-` ladder data: already uses. No new runtime
  semantics — pure ergonomic alias the resolver and SSR walker
  inherit verbatim.
- [x] **13.3** **`use:` directives.** `<container use:hover
  use:tooltip="Click to save"/>` attaches the registered
  modifier behaviours by id, with the attribute value lowering
  to the modifier's first prop. Lowers to a sequence of
  `Modifier { kind, enabled: true, props }` entries that fold
  through the existing Wave 1 `ModifierBehaviour::wrap` pipeline
  — no new render-time concept, just an authoring shorthand for
  data the document model already carries.

**Composition rationale.** Named slots is the Web Components /
Vue contract that decouples chrome composition from chrome
content. A migration target like `shell.app-window` doesn't know
*what* lives in its menu strip or status bar — only that there
*is* one — and named slots give the DSL author the same insertion
points the Slint-era source did. The two ergonomic-alias
namespaces (`on:` / `use:`) translate the most fluently-readable
upstream patterns into the same routing / modifier substrates the
DSL already has, so a contributor coming from Vue or Svelte writes
the obvious thing and it just works.

### Wave 14 — DSL substrate from HTMX / CSS / SwiftUI / Compose / Solid / Lit / Astro

Wave 13 surveyed the React-family component frameworks. Wave 14
widens the lens to the four other family trees of UI authoring
substrate: **HTMX** (server-driven HATEOAS), **CSS** (custom
properties + container queries + state pseudo-classes), **SwiftUI
/ Jetpack Compose** (declarative modifier chains over typed views),
and **Solid / Lit / Astro / Alpine** (the post-React reactive
crop). Each row scoped against a *concrete consumer* — patterns
without a real migration waiting are listed for completeness but
deferred.

| Pattern | Source | Consumer / gap | Status |
|---|---|---|---|
| **Design tokens as scope binding** (`tokens.colors.surface`) | CSS custom properties, SwiftUI `Color.accentColor`, Tailwind tokens | Every migrated `.prism-ui` file currently hardcodes hex colors; `shell.inspector-row` alone has 8 distinct hex literals | **14.1 — implement** |
| **Block-local `<let name= value=/>`** | Svelte `{@const}`, SwiftUI `let`, Vue `<script setup>` | DRY repeated ternary checks (`shell.inspector-row` evaluates `kind == 'row'` four times) | **14.2 — implement** |
| **Event modifiers (`on:click.once`, `on:click.stop`)** | Vue `@click.once`, Svelte `on:click\|once`, HTMX `hx-trigger="click delay:500ms"` | Round-trip modifier suffix in attribute value through `data-on-<event>` for future consumers | **14.3 — implement** (round-trip; behaviour follows the Wave 1 modifier-fold integration) |
| **`<teleport to="…"/>`** | Vue 3 `<Teleport>`, React `createPortal`, SwiftUI `.popover()` | Overlay routing from a deep child to root | Defer — overlays today are authored at root in `app.prism-ui`; no consumer waiting |
| **`hx-confirm` / guarded dispatch** | HTMX | Destructive trash buttons | Defer — confirmations are a service-level concern (`MutCtx::confirm`), not a DSL one |
| **Container queries `@container (width…)`** | CSS Containment, Web | Component-level responsive styling | Defer — viewport branching is handled today by the `builder-toolbar` device-entries pattern + parent block prop fan-out |
| **`x-cloak` / flash-of-unstyled-content hide** | Alpine.js | Hide an element until the runtime initialises | Skip — Prism's retained-mode `Surface` doesn't have an FOUC window |
| **`client:idle` / `client:visible`** | Astro islands | Defer hydration until interaction | Skip — wrong substrate (we don't hydrate; everything is native) |
| **`@apply` utility composition** | Tailwind | Compose utility classes onto a selector | Skip — we use `style:` attribute syntax, not classes |
| **Lifetime / unmount animations (`animate:in` / `animate:out`)** | Svelte transitions, Framer Motion | `shell.toast` fade-out, picker overlay enter | Defer — `transition:` namespace already round-trips author intent; effect-driven animator is the follow-up |
| **Reflected boolean attrs (`?disabled`)** | Lit | Author shortcut for conditionally-present attrs | Skip — the `data:` / `aria:` empty-string filter already covers this |
| **`v-memo` / `:key` reactive memoisation hint** | Vue 3, Solid | Skip re-render when dependencies stable | Defer — no incremental tree-diff substrate yet |
| **Two-way `bind:value` on inputs** | Svelte `bind:value`, Vue `v-model` | `prism.text-input` writeback | Defer — blocked on `prim.text-input` body (Wave 10) |

- [x] **14.1** **Design tokens as scope binding.** Inject a
  `tokens` binding (the snapshot of [`prism_core::design_tokens::DEFAULT_TOKENS`]
  as JSON) into `LowerScope` at the loader seam, so a DSL author
  can write `style:background="{tokens.colors.surface}"` /
  `padding="{tokens.spacing.md}"` instead of hardcoded hex / px.
  Every existing dotted-path / template-attr / style-override path
  inherits the binding verbatim — no new runtime concept, just one
  scope seed. Sibling injection at both the shell skeleton path and
  the per-block `PrismUiBlock::lower_ui` path so every DSL surface
  resolves `tokens.*` uniformly. Tests:
  `components::prism_ui_loader::tests::tokens_binding_resolves_color_in_style_namespace`,
  `interpret::tests::tokens_binding_resolves_nested_path_through_style`.
- [x] **14.2** **Block-local `<let name="X" value="{expr}"/>` constants.**
  Lifts a one-time expression evaluation into a named binding that
  subsequent siblings can read. Mirrors Svelte `{@const}` exactly —
  the binding lives in the current scope from declaration onward,
  and child elements inherit it through the existing
  `LowerScope::with_binding` cloning. Recognised as a new
  `Node::Let { name, value }` AST variant; the sibling pre-pass in
  `expand_control_flow` evaluates the expression against the
  current scope and seeds the binding into every subsequent
  sibling's scope. Doesn't render anything — pure binding. Test:
  `interpret::tests::let_binding_propagates_to_subsequent_siblings`.
- [x] **14.3** **Event modifiers in the `on:` namespace value.** Today
  `on:click="cmd foo"` lowers to `data-on-click="cmd foo"`. Authors
  can already encode modifiers in the value (`cmd foo` /
  `cmd foo,once`) — Wave 14.3 documents the shape and reserves the
  `.<modifier>` suffix on the local part (`on:click.once`,
  `on:click.stop`) for future runtime wiring. Today the modifier
  round-trips into `data-on-click-once` semantic attrs without
  behaviour change, matching the Wave 9.4 / 13.3 pattern (data
  carries intent, runtime wiring follows). Test:
  `interpret::tests::on_namespace_modifier_suffix_round_trips_as_data_attr`.

**Composition rationale.** Wave 14 is the substrate sweep that
closes the **scope-bindings vocabulary** the migrated DSL files
have been asking for. Tokens unlock visual consistency (one
source of truth for every accent / surface / border across the
shell); `<let>` unlocks DRY inside a single component body. The
event-modifier suffix is the lowest-cost path to authoring shapes
contributors arriving from Vue / Svelte already type, with the
same "data round-trips today, runtime wiring follows" discipline
Waves 9.2, 9.4, and 13.3 use.

The **deferred** rows in the table aren't second-class — each has
either no consumer waiting (`<teleport/>`, `hx-confirm`) or its
substrate isn't there yet (two-way `bind:value` needs the
`prim.text-input` body, `v-memo` needs incremental tree-diff). The
**skipped** rows are wrong-substrate moves: Prism doesn't have
classes (Tailwind `@apply`), doesn't hydrate (Astro islands),
doesn't have an FOUC window (Alpine `x-cloak`).

### Wave 15 — Iteration / loop patterns

Wave 14 widened the lens to scope-binding vocabulary. Wave 15
widens it to **iteration**: the patterns every language and
template engine evolved beyond a bare `for x in xs` loop. Each
row scoped against a *concrete consumer*.

| Pattern | Source | Consumer / gap | Status |
|---|---|---|---|
| **`else` after `for=` runs on empty iteration** | Svelte `{#each ... }{:else}...{/each}`, Vue `<template v-if=…>` paired with `<template v-else>` | `shell.signals-panel` ("No connections yet"), `shell.schema-designer`, every list-rendering shell block | **15.1 — implement** |
| **Numeric range `for="i in 0..n"` / `for="i in 0..=n"`** | Rust `0..n`, Python `range(n)`, Vue `v-for="i in n"`, SwiftUI `ForEach(0..<n)` | Fixed-count repeats (pagination, calendar grids, schema field-default seeds) without authoring an upstream array | **15.2 — implement** |
| **Object iteration `for="value, key in obj"`** | Vue `v-for="(value, key) in obj"`, Python `for k, v in dict.items()`, JS `Object.entries()` | Rendering an arbitrary object's key-value entries (debug views, properties-panel-style displays) | **15.3 — implement** |
| **`for-step` `for="i in 0..100 step 10"`** | Rust `step_by`, Python `range(0, 100, 10)`, SwiftUI `stride(from:to:by:)` | Tick marks on rulers / gradient stops | Defer — no consumer waiting |
| **Reverse `for="item in items reverse"`** | Vue `v-for` with reversed array, Svelte `[...items].reverse()` | Activity log rendered newest-first | Defer — data layer handles the reversal cleaner |
| **`while=` / `do-while`** | C, Rust, Python | Imperative iteration | Skip — declarative DSL has no halt condition |
| **`break` / `continue`** | C, Rust | Mid-iteration short-circuit | Skip — declarative DSL has no imperative control |
| **Keyed iteration `for="x in xs by x.id"`** | Vue `:key`, React `key=`, Solid `<For each= keyed=>` | Reactive diff stability across re-renders | Defer — no incremental tree-diff yet (also deferred in Wave 13) |
| **Range destructuring `for="[a, b] in pairs"`** | JS, Python | Nested tuple unpacking | Skip — `props="{item}"` spread + dotted-path access covers it |
| **`zip` / parallel iteration** | Python `zip`, Solid `<For>` | Two collections in lockstep | Defer — niche; data layer can pre-zip |
| **`chunk` / `group-by`** | Lodash `_.chunk`, SwiftUI Section/Group | Partition iteration into sub-headers | Defer — composition + a pre-grouped data shape works today |
| **Generators / yield-driven sequences** | Python, JS, C# `yield return` | Streaming infinite sequences | Skip — wrong substrate (declarative tree render is bounded) |

- [x] **15.1** **`else` after `for=`.** When a `for=` element
  produces zero items, the running `chain_taken` flag is set to
  `Some(false)` instead of the previous reset-to-`None`. A
  subsequent `else=` (or `else-if=`) sibling treats the empty `for`
  as a falsy `if` and renders its body. Non-empty `for`s set
  `chain_taken = Some(true)`, suppressing the else branch as you'd
  expect. The substrate is two lines in `expand_control_flow` plus
  one comment; the visible effect is that Svelte's `{:each items as
  i}{:else}…{/each}` pattern is now spelled
  `<x for="i in items"/><y else>…</y>` in `.prism-ui` source.
  Closes the empty-state authoring gap that every list-rendering
  shell block carries (`signals-panel`, `schema-row`, `nav-page-list`,
  `inspector-tree`). Tests:
  `interpret::tests::{else_after_empty_for_renders_fallback,
  else_after_non_empty_for_is_suppressed,
  else_if_after_empty_for_evaluates_predicate}`.
- [x] **15.2** **Numeric range `for="i in 0..n"` / `for="i in 0..=n"`.**
  The for-source string is parsed for the `..` or `..=` operator
  before falling through to the bare-binding lookup. Both endpoints
  resolve through the full expression evaluator, so `for="i in
  0..items.length"` works as well as `for="i in 0..10"`. Inclusive
  (`..=`) and exclusive (`..`) variants match Rust's `RangeInclusive`
  / `Range`. Reverse ranges (start > end) yield zero items (no
  panic, no descending iteration) — same shape Rust's iterator
  trait provides. The iteration variable binds as a typed
  `Number` so `padding="{i * 4}"` reads through `parse_f32`
  uniformly. Tests:
  `interpret::tests::{range_exclusive_iterates_start_through_end_minus_one,
  range_inclusive_iterates_start_through_end,
  range_with_descending_endpoints_yields_zero_items,
  range_endpoints_resolve_through_scope_bindings}`.
- [x] **15.3** **Object iteration `for="value, key in obj"`.** When
  the source binding resolves to a JSON object (rather than an array),
  iterate over its `(key, value)` entries with the second LHS
  variable bound to the string key. The existing `for="x in xs"`
  arrays-only path keeps its semantics — the type of the source
  drives the iteration mode. `IndexMap`'s insertion-order
  preservation flows through `serde_json::Map`, so iteration order
  is deterministic. Closes the "render an object's entries as rows"
  authoring gap (debug views, key-value tables). Tests:
  `interpret::tests::{for_loop_over_object_iterates_entries_in_insertion_order,
  for_loop_over_object_binds_key_to_second_lhs_variable,
  for_loop_index_variable_still_works_on_arrays}`.

**Composition rationale.** Wave 15 closes the iteration vocabulary
the migrated `.prism-ui` files have implicitly been asking for.
The headline (`else` after `for`) is the smallest change with the
biggest authoring win — every empty-state ladder collapses from a
parallel `if=length>0` / `else` pair to one `for` + one `else`
sibling. The range form is the same pattern Rust / Python / SwiftUI
converged on — `0..n` reads as a numeric sequence everywhere, and
the inclusive `..=` variant matches Rust's grammar so authors don't
need a second mental model. Object iteration completes the JSON-shape
duality (array vs. object) on the same `for=` attribute — the runtime
already round-trips both shapes through the dotted-path resolver;
iteration is the natural extension.

**Deferred / skipped** rows mirror the discipline from Waves 13 and
14: no consumer waiting (step, reverse, zip, chunk), wrong substrate
(while / do-while / break / continue / generators — Prism's tree
render is bounded and declarative), or blocked on substrate that
hasn't landed yet (keyed iteration → incremental tree diff).

---

## 3. Wave 1 — The Composable Inspector

### 3.1 Model

A `Node` is `(component, props, modifiers, layout, transform, style)`.

- `component` is the typed identity (`text`, `button`, `container`).
- `modifiers: Vec<Modifier>` is an ordered stack of behaviours.
- Each `Modifier` is `{kind: ModifierId, enabled: bool, props: Value}`.

Behaviour kinds become entries in `ModifierRegistry`, registered the
same way components are. The closed `ModifierKind` enum stays as a
serialization-friendly alias for the six builtins; new modifiers
register dynamically through the trait.

### 3.2 `ModifierRegistry` trait

```rust
pub type ModifierId = Cow<'static, str>;  // "builtin:tooltip", "luau:my-mod"

pub trait ModifierBehaviour: Send + Sync {
    fn id(&self) -> ModifierId;
    fn label(&self) -> &str;
    fn icon(&self) -> Option<&str> { None }
    fn description(&self) -> &str { "" }
    fn schema(&self) -> Vec<FieldSpec>;

    /// Optional render wrapper. Receives the lowered child subtree
    /// and the modifier's prop bag; returns the wrapped subtree.
    fn wrap(&self, _ctx: &LowerCtx, _modifier: &Modifier, child: Node) -> Node {
        child
    }

    /// Optional signals contributed by the modifier.
    fn signals(&self) -> Vec<SignalDef> { vec![] }

    /// Optional reactive effects installed at document load. The
    /// owner reclaims them on detach or node drop.
    fn install_effects(
        &self,
        _node_id: &NodeId,
        _bindings: &DocumentBindings,
        _owner: &Owner,
    ) {}
}

pub struct ModifierRegistry { /* IndexMap<ModifierId, Arc<dyn ModifierBehaviour>> */ }
```

Same DI shape as `ComponentRegistry`. `Arc<dyn>` so Luau-authored
modifiers fit the same vector. `Send + Sync` so the registry
crosses the daemon boundary intact.

### 3.3 Render integration

One fold in `LowerCtx::lower(node)`:

```rust
let mut lowered = self.lower_component(node, style);
for m in node.modifiers.iter().rev() {  // innermost-first
    if !m.enabled { continue; }
    if let Some(beh) = self.modifier_registry.get(&m.kind) {
        lowered = beh.wrap(self, m, lowered);
    }
}
lowered
```

Headless tests, SSR, and registry-less render walks all keep working:
`wrap` defaults to identity and no `wrap()` impl is required.

### 3.4 Inspector: section per modifier + footer

`derive_property_rows` becomes structured:

```text
section-header   "Heading"               ← component identity
  field-row      "body" / text
  field-row      "alignment" / select
modifier-header  "Tooltip"      ⚙ × ≡    ← per attached modifier
  field-row      "text" / text
  field-row      "placement" / select
modifier-header  "Responsive Visibility"  ⚙ × ≡
  field-row      "hide-on-mobile" / boolean
add-modifier                             ← footer
```

Three new shell blocks (all migrated to `.prism-ui` per Wave 11
from day one):

- `shell.modifier-header` — extends `shell.section-header` with
  toggle, remove (×), drag handle (≡). Carries
  `data-role="modifier-header"` + `data-modifier-idx="<n>"`.
- `shell.add-modifier-button` — ghost button at panel bottom.
- `shell.modifier-picker` — overlay listing
  `ModifierRegistry::list()` minus what's already attached.

### 3.5 Mutators + routes

Four `AppState` mutators, each routing through the §43 C "one
re-derivation entry":

```rust
attach_modifier(node, kind, registry);
detach_modifier(node, idx, registry);
toggle_modifier(node, idx, registry);
reorder_modifier(node, from, to, registry);
```

Each ends with `self.resync_builder_for_selection(registry)`. Each
writes through `NodeMutator::with_bindings(..)` so reactive
subscribers wake.

Routes:

| `data-role` | Mutator |
|---|---|
| `modifier-toggle` | `toggle_modifier` |
| `modifier-remove` | `detach_modifier` |
| `add-modifier-open` | `OverlaySlot::open_picker(...)` |
| `modifier-picker-select` | `attach_modifier` + close picker |
| `modifier-reorder` | `reorder_modifier` (pointer-drag gesture from Wave 3) |

### 3.6 Bootstrap behaviour set

Six already-defined kinds register through the trait, schemas
verbatim. Plus a new bootstrap set that immediately makes the panel
feel alive:

| New | Wraps | Why |
|---|---|---|
| `Visible` | returns `Node::empty()` when off | One-click hide-from-canvas |
| `Locked` | sets `aria-disabled`, suppresses pointer routes | Prevent edits |
| `Hover` | adds `:hover` tint container | Visual feedback in canvas |
| `Click` | wraps + emits `on:click` signal | Compose interactivity onto any node |
| `BindToSelection` | installs effect subscribing to `$selection.<key>` | Live data-bind any prop |
| `RunLuauScript` | spawns Luau handler on signals | MonoBehaviour-style custom code |

Six core + six bootstrap = a real composition vocabulary on day one.

### 3.7 Luau-authored modifiers + read/write

`LuauModifier` is a `ModifierBehaviour` impl whose body delegates to
a Luau table with `schema`, `wrap`, `install_effects` keys. Reuses
the `LuauComponent` infrastructure already in
`prism-builder/src/luau_component.rs`.

```lua
local mod = {}

function mod.schema()
    return {
        field_spec("speed", "number", { min = 0, max = 10, default = 1 }),
        field_spec("axis", "select", { options = { "x", "y", "z" } }),
    }
end

function mod.wrap(ctx, modifier, child)
    -- Lua-authored render wrap: animate `child` along `axis` at `speed`.
end

function mod.install_effects(node_id, bindings, owner)
    local speed = bindings:props_for(node_id):signal("speed")
    effect(function()
        local v = speed:read()
        -- ... reactive body, re-runs on signal change ...
    end, owner)
end

return mod
```

Phase 5 of dioxus already exposes `Signal::read`/`write` to Luau via
`prism-core::luau_reactive`. The new piece is exposing
`ReactiveProps::signal(key)` so Luau can address per-key signals
through `node:props():signal("k"):read()` — this is the same surface
Rust uses (`bindings.props_for(node_id).signal(k)`), one method per
side.

### 3.8 Named pin tests

- `modifier_registry_lists_six_builtin_kinds_in_stable_order`
- `derive_property_rows_emits_section_per_attached_modifier`
- `add_modifier_picker_filters_out_already_attached`
- `attach_modifier_appends_section_and_marks_block_dirty`
- `detach_modifier_drops_section_and_owner_disposes_effects`
- `wrap_pipeline_applies_modifiers_innermost_first`
- `toggle_modifier_skips_wrap_when_disabled`
- `luau_modifier_install_effects_run_under_owner_drop_on_detach`
- `luau_modifier_props_read_and_write_round_trip_through_signal`
- `e2e_attach_tooltip_to_button_renders_tooltip_overlay_on_hover`

---

## 4. Wave 2 — Field-edit UX

Per §43 D, every non-boolean field-row carries routing attrs but the
handlers are stubs. Each kind is a focused change against the
existing routes + `field_focus` infra. Every commit goes through
`AppState::set_node_prop` (which already runs through
`NodeMutator::with_bindings`) so reactive subscribers wake
automatically.

| Kind | Approach |
|---|---|
| `text` | Keystroke through `field_focus`; Enter / blur commits; multi-line via Shift-Enter. |
| `number` | Drag-scrub on `drag_number_field`; arrow keys ±1, ±10; clamp by `data-min` / `data-max`. |
| `select` | Chevron click opens `<popover>` + `<list-picker>` anchored under the row. |
| `color` | Swatch click opens HSL `<color-picker>` overlay. |
| `file` | Browse click opens `<file-button>` (wraps `rfd`). |

`<popover>`, `<list-picker>`, `<color-picker>`, `<file-button>` come
from Wave 10's primitive registry. Wave 2 ships the consumer wiring;
the primitives ship as their first call site.

---

## 5. Wave 3 — Pointer-driven canvas

Today the canvas paints, hit-tests, and drag-captures, but it does
*not* select on click and does *not* accept pointer-driven palette
drops. Both fix against existing infra.

- **Canvas hit → select.** In the canvas `pointer_down` arm, consult
  `Surface::hit_test_at(x, y)` before drag-capture. If the hit
  carries `data-doc-node-id`, route through `select_node`. Each
  lowered builder node already emits the attr via `with_semantic`.
- **Palette drag → drop.** When `palette_selected = Some(kind)` and
  `pointer_down` fires on the canvas, capture the drag. `pointer_move`
  updates a ghost overlay. `pointer_up` calls `insert_at_offset` at
  the hit's nearest flow slot.
- **Selection gizmo.** `shell.selection-gizmo` overlay around the
  selected hit-rect with edge handles. `data-role="resize-edge"` +
  `data-edge` mutates layout. (Memory: "inline editing is GUI-only" —
  this is exactly that.)
- **Right-click → context menu.** `Mouse::Right` + hit metadata opens
  `OverlaySlot::context_menu` populated from
  `BuilderService::context_actions_for(node)`.

---

## 6. Wave 4 — Connections panel

Today the Signals/Connections panel renders rows but the trash and
add affordances are dead (B6 third wave `command: None`).

- `BuilderSlot::selected_connection: Option<ConnectionId>`.
- `shell.signal-connection-row` click → set cursor.
- Trash → real `BuilderService::delete-connection` reading cursor.
- "+" → `shell.connection-picker` (source × target × action kind,
  with `ActionKind::Bind` from Phase 4a in the list).
- Mutators: `add_connection`, `delete_connection`,
  `update_connection_field`; each runs the C-wave resync.

---

## 7. Wave 5 — Builder catalog into `ShellComponentRegistry`

§43 E flagged this deferred. One call in `Shell::new`:

```rust
prism_builder::starter::register_builtins(&mut shell_registry);
```

Plus a startup assertion
`no_overlapping_block_ids_between_shell_and_starter`. Small but
unblocks Wave 1 and Wave 2 (real schemas for real builder nodes →
real Inspector sections → real edits).

---

## 8. Wave 6 — Three "command: None" rows

The B6 third wave left three rows wired to `None` awaiting per-slot
cursors:

- `shell.nav-page-row` chevrons → `NavigationSlot::selected_page` +
  `move-page-{up,down}`.
- `shell.schema-row` trash → `SchemaSlot::selected_field` +
  `delete-schema-field`.
- `shell.signal-connection-row` trash → covered in Wave 4.

Each: one field + one mutator + one row in `commands()`.

---

## 9. Wave 7 — Headless visual capture

`prism visual --scene <name>` shells out to flags `prism-shell`
doesn't yet accept (§43 E1).

- `prism-shell --scene <name>` boots, runs one frame, exits.
- `--screenshot <path>` writes PNG via femtovg offscreen surface.
- `prism visual` removes the screencapture shim.

Unlocks before/after capture for every future wave + the visual
regression harness.

---

## 10. Wave 8 — Luau parity for components + modifiers

Phase 5 of dioxus exposed `Signal::read`/`write` to Luau. Phase 4b
landed `DocumentBindings` + `ReactiveProps` as the per-NodeId
reactive bag. This wave makes the Luau surface match the Rust one
exactly, so a Luau author has the same vocabulary a Rust contributor
has.

- **8.1 — `node:props()`.** Luau-side `node:props():read("k")` /
  `:write("k", v)` / `:signal("k")` mirrors
  `bindings.props_for(node_id)` on the Rust side. One method per
  side, identical semantics.
- **8.2 — `LuauModifier`.** Sibling to `LuauComponent`, same surface
  (`schema`, `wrap`, `install_effects`, `props`). `wrap` returns a
  Luau template tree walked through `lower_template` (the existing
  path).
- **8.3 — Type stubs.** `generate_signal_type_stubs` grows a
  modifier branch. Each registered modifier emits a Luau type block
  alongside component stubs. `.luau` LSP gets autocomplete for any
  registered modifier's prop schema.
- **8.4 — Live registration.** `register_block_from_luau` and
  `register_modifier_from_luau` callable from `script.luau` at boot.
  Authoring a new modifier or component is one Luau file — no Rust
  recompile.

**The reactivity story:** a Luau-authored modifier's `install_effects`
gets the same `Owner` lifecycle as a Rust one. When the user detaches
the modifier, the owner drops, the effects unsubscribe, the signals
go quiet. No Luau-specific lifecycle to remember.

**The cross-scope story:** because every prop is a
`reactive::Signal<T>` and signals carry transport scope (local /
IPC / federated / peer / SSR — Phase 7 of dioxus), a Luau-authored
modifier on a daemon-published node automatically reads/writes
through the IPC transport without the Luau author writing anything
transport-specific. `prop:write("x", 1)` works identically on a
local prop and a federated one.

---

## 11. Wave 9-11 — `.prism-ui` self-hosting

### 11.1 Vision

Today: ~50 hand-rolled Rust files in `prism-shell/src/components/*`.
Each is a `SpecBlock` whose `lower_fn` imperatively constructs a
`ui::Node` tree. Each duplicates a slice of patterns the others
use: flex containers, padding rings, hover tints, semantic attrs.

Tomorrow: every component that *can be* authored as `.prism-ui`
source *is*. Only truly imperative behaviours (canvas paint, text
buffer, hit-test-aware document host) stay Rust — and those ship as
runtime *primitives* (`<canvas-paint>`, `<text-buffer>`,
`<builder-host>`) exposed to the DSL. The shell becomes a Builder
project. Self-hosting in the same DSL is the proof the DSL is
complete.

### 11.2 Inventory: 50 components → 3 tiers

**Tier 1 — pure visual composition (~33 files). Mechanical migration.**

`app_card`, `app_window`, `chrome`, `component_palette`,
`dock_panel`, `dock_tab`, `dock_tab_bar`, `dock_workspace`,
`docs_content`, `docs_sidebar`, `docs_view`, `explorer`,
`help_tooltip`, `icon_button`, `inspector_row`, `inspector_tree`,
`launchpad`, `menu_bar_row`, `menu_item`, `nav_button`,
`nav_page_list`, `nav_page_row`, `properties_panel`, `schema_row`,
`section_header`, `signal_connection_row`, `signals_panel`,
`status_bar`, `toast`, `toast_stack`, `toolbar_separator`,
`workflow_page_bar`, `workflow_page_button`. Each is `.prism-ui` of
<30 lines.

**Tier 2 — composed with primitives (~14 files). Migrate after Wave 10.**

`builder_toolbar`, `command_palette`, `component_picker`,
`context_menu`, `dock_divider`, `drag_number_field`, `field_editor`,
`gizmo_move`, `gizmo_rotate`, `gizmo_scale`, `menu_dropdown`,
`resize_handle`, `schema_designer`, `transform_editor`. Each depends
on 1-3 of the new primitives from §10.3.

**Tier 3 — imperative-only (~3 files). Stay Rust, ship as primitives.**

`builder_canvas` → `<builder-host>` primitive,
`code_editor` → `<text-buffer>` primitive,
`nav_graph` → `<canvas-paint>` primitive.

### 11.3 Primitives lifted into runtime

The bar for "primitive" is: more than one component needs it, *or*
it crosses the Rust/DSL boundary (gestures, focus, popover
anchoring, canvas paint). Everything else stays composition. Each
primitive is one `BlockSpec` entry in a new `PrimitiveRegistry`
shipped with `prism-ui-runtime`.

| Primitive | Replaces today's |
|---|---|
| `<text-input>` | text-branch keystroke handling in `field_editor` |
| `<drag-scrub>` | drag pill in `drag_number_field` |
| `<popover>` | overlay positioning in 4 components |
| `<list-picker>` | duplicated list bodies in 4 components |
| `<collapsible>` | chevron+body in `section_header`, `signals_panel`, `docs_sidebar` |
| `<split-handle>` | drag logic in `dock_divider` + `resize_handle` |
| `<timed-overlay>` | toast auto-dismiss |
| `<focus-trap>` | command-palette focus |
| `<select>` | composition of `<popover>` + `<list-picker>` |
| `<color-picker>` | stub in `field_editor` color branch |
| `<file-button>` | stub in `field_editor` file branch (wraps `rfd`) |
| `<resize-edge>` | new — Wave 3 |
| `<canvas-paint>` | `nav_graph` body |
| `<text-buffer>` | `code_editor` body |

Each exposes its prop schema through the standard
`Component::schema()` surface — so a primitive's props show up in the
Inspector unmodified, and they're Luau-readable/writable via the
Wave 8 surface like any other component.

### 11.4 Generalization sweeps (before any migration)

Three sweeps that each delete hand-rolled code in N places by
lifting to one place:

1. **Hover/active visual states → `Hover` modifier.** Every `*_lower`
   that tints on hover hand-builds the hover container. Wave 1's
   `Hover` modifier subsumes it. Deletes inline hover code across 7+
   components.
2. **Routing attrs → `route:` namespace.** Every component
   hand-emits `data-role` + `data-target-id` via `with_semantic`.
   Lift to `<container route:role="x" route:target-id="y"/>` in the
   DSL. Runtime resolver translates automatically.
3. **Section headers + collapse bodies → `<collapsible>` primitive.**
   One primitive, three call sites.

These three sweeps together delete more code than the entire
migration adds.

### 11.5 Migration order

1. **DSL gap-close (Wave 9).** `route:` namespace, `:state`
   selectors, `bind:` sugar, animation transitions.
2. **Primitive registry (Wave 10).** Land all 14 primitives as
   `BlockSpec` rows in `prism-ui-runtime`. Each comes with its own
   schema, signals, and tests.
3. **Sweep passes (§11.4).** One PR per sweep.
4. **Tier-1 migration.** 33 PRs of `.prism-ui` adds + `.rs`
   deletes. CI gate: visual scene before/after must be
   byte-identical for the lib unit tests pinned in §43 E.
5. **Tier-2 migration.** 14 PRs, each ships a new primitive's first
   real consumer.
6. **Tier-3 primitives.** Ship as runtime entries; the remaining
   shell wrappers shrink to thin `.prism-ui`.

### 11.6 What we gain

- **Self-hosted authoring.** A new shell chrome component is one
  `.prism-ui` file. No Rust required for visual chrome.
- **Hot reload of chrome.** Phase 9 anchored at `lower_template`;
  a `.prism-ui` shell edit hot-swaps without re-link.
- **One vocabulary.** A user composing a Builder page and a Prism
  contributor evolving the shell do the same thing in the same
  surface with the same primitives. The DSL's "is it good enough"
  bar is "did the maintainers prefer it over Rust." Becomes
  provable.
- **Luau parity.** Wave 8's `node:props()` surface works
  identically on every shell `.prism-ui` document — a Luau script
  can read/write any shell prop just like any builder prop.
- **~4000 lines of Rust deleted, ~1500 lines of `.prism-ui` added.**

---

## 12. Sequencing

```
Wave 5 (registry merge, 30 min) ─┐
                                 │
                                 ↓
        ┌────────────────────────┼────────────────────────┐
        ↓                        ↓                        ↓
  Wave 1 (inspector)       Wave 2 (field-edit)       Wave 7 (visual)
        │                        │                        │
        └────────────┬───────────┘                        │
                     ↓                                    │
              Wave 3 (canvas pointer)                     │
              Wave 4 (connections)                        │
              Wave 6 (3 dead rows)                        │
                     │                                    │
                     └────────────┬───────────────────────┘
                                  ↓
                            Wave 8 (Luau parity)
                                  ↓
                            Wave 9  (DSL gap-close)
                            Wave 10 (primitives)
                            Wave 11 (self-host migration — long tail)
```

Wave 5 lands first because it unblocks 1 and 2 (no real schema
lookups otherwise). Waves 1/2/7 land in parallel — three independent
PRs. Waves 3/4/6 sequence after the Inspector is real. Wave 8
follows so the Luau parity story is provable before any migration
hits it. Waves 9-11 are the long tail; Waves 3-6 can continue in
parallel with 9-11 because the primitives don't interfere with the
Inspector flows.

---

## 13. Discipline

Per the project style rule (`CLAUDE.md`): "Never deprecate. Rename,
move, break, fix. `cargo check --workspace` is the safety net."

- No bridges, no compat shims, no `legacy_*` arms.
- Every migration PR deletes the Rust file it replaces.
- Every primitive PR ships with its first consumer migrating onto it.
- Every wave's checklist (§2) updates as items merge — this doc is
  the single source of truth for "what remains."
- Decision-log rows append per landed phase, matching the §43
  pattern.

---

## 14. Decision log

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-11 | Composable Builder Plan supersedes the gap between `docs/dev/clay-migration-plan.md` §43 Phase E + B6 third wave and `docs/dev/dioxus-inspiration.md` Phase 10 with one document covering: modifier-as-first-class composition (Wave 1), deferred field-edit UX (Wave 2), pointer-driven canvas (Wave 3), connections panel (Wave 4), registry merge (Wave 5), B6 leftover rows (Wave 6), headless visual capture (Wave 7), Luau parity for components and modifiers (Wave 8), DSL gap-close (Wave 9), primitive registry (Wave 10), and the long-tail `.prism-ui` self-hosting migration (Wave 11). Each wave carries a checklist in §2 that ticks as items merge. | Both predecessor plans declared "done" but the user can feel the soul gap: the Properties panel is fixed and flat, the canvas is API-driven, the DSL only renders user-authored documents and never the shell that hosts them. The bones already exist (`Modifier`, `node.modifiers`, `modifier_schema`, `reactive::Signal`, `DocumentBindings`, `LowerCtx::with_bindings`, `NodeMutator::with_bindings`, `Surface::hit_test_at`, `POINTER_ROUTES`, `LuauComponent`, `luau_reactive`); this plan activates them. Wave 11 is the unifying move: the shell becomes a Builder project, the DSL self-hosts, and the duplicate hover/route/collapse patterns across ~50 files collapse to three primitives + one modifier. The Luau parity wave (8) makes the authoring surface uniform across Rust / `.prism-ui` / Luau so a contributor can pick any layer and the rest still composes. |
| 2026-05-11 | **Wave 5 lands** — `Shell::new` registers `prism_builder::starter::register_builtins` into the live `ShellComponentRegistry` plus the `card` prefab and `facet` component (`packages/prism-shell/src/components/registry.rs:164`). Named pin test `no_overlapping_block_ids_between_shell_and_starter` asserts the `shell.*` namespace and the document catalog stay disjoint. | The §43 E gap (production shell registry only carried `shell.*` blocks) blocked Waves 1+2: without builder schemas the inspector returned empty rows for `text` / `button` / etc. The implicit register-rejects-duplicates guarantee covered the collision case; the named test makes the contract searchable. |
| 2026-05-12 | **Wave 1 lands** — the composable inspector. `ModifierBehaviour` trait + `ModifierRegistry` in `prism-builder/src/modifier.rs`; the six baseline `ModifierKind` variants ported as struct impls; six Wave 1.7 bootstrap behaviours (`Visible`, `Locked`, `Hover`, `Click`, `BindToSelection`, `RunLuauScript`) in `prism-builder/src/modifier_bootstrap.rs`. `Modifier` shape evolved from `{kind: ModifierKind, props}` to `{kind: String, enabled: bool, props}` with `#[serde(rename / default / skip_serializing_if)]` so pre-Wave-1 documents deserialize verbatim. Render fold in `LowerCtx::lower(node)` applies modifiers innermost-first via `ModifierBehaviour::wrap`; disabled entries skip; unknown ids fall through. `derive_property_rows` extended to emit one `shell.modifier-header` section per attached modifier (with toggle / remove / reorder affordances) plus a `shell.add-modifier-button` footer; the picker overlay lives on `OverlaySlot::modifier_picker` and renders via the new `shell.modifier-picker` block populated from the live registry minus already-attached ids. Five `POINTER_ROUTES` (`modifier-toggle` / `modifier-remove` / `modifier-reorder` / `add-modifier-open` / `modifier-picker-select`) plus five new `AppState` mutators (`attach_modifier` / `detach_modifier` / `toggle_modifier` / `reorder_modifier` / `set_modifier_prop`) wire the full attach / edit / remove flow end-to-end. `Shell::new` seeds `state.modifier_registry: Some(Arc<ModifierRegistry>)` so `resync_builder_for_selection` picks it up without per-callsite plumbing. **397 prism-builder tests, 421 prism-shell tests pass; clippy clean.** Wave 1.8 (Luau-authored modifiers) defers to Wave 8. | The plan's diagnosis was that `node.modifiers` existed as data but had zero consumers — no render fold, no inspector seam, no add/remove UX. This wave activates all three at once. The behaviour-as-trait shape mirrors `Component` exactly so future Luau-authored modifiers slot in through the same `Arc<dyn>` registry. The shipped bootstrap set (Visible / Locked / Hover / Click) gives an immediately useful composition vocabulary; the two schema-only entries (BindToSelection / RunLuauScript) round-trip cleanly and surface in the inspector — Wave 8 lights their wrap bodies up. |
| 2026-05-12 | **Wave 2 lands** — field-edit UX upgrades on the existing `field_focus` + `number_drag` infrastructure. New: `textarea` kind opens the focus session and Shift-Enter inserts a literal newline (plain Enter still commits). `number` / `integer` clicks now ALSO open a focus session alongside the drag-scrub, so arrow keys ±1 / ±10 (with shift) route through `FieldFocusService::nudge_focused_number` to the bound prop. Three new pin tests (`shift_enter_inserts_newline_for_textarea_kind`, `shift_enter_on_text_kind_still_commits`, `arrow_keys_nudge_focused_number_field`). `select` ships with the existing click-to-cycle through `data-options`; `color` and `file` ship with the text-focus paste-a-string UX. Full anchored-dropdown / HSL picker / rfd dialog are explicitly deferred to Wave 10's primitive registry (`<popover>`, `<list-picker>`, `<color-picker>`, `<file-button>`) — those primitives' first consumers will be these three kinds. **421 prism-shell tests pass; clippy clean.** | The plan's §43 D explicitly deferred full edit UX on non-boolean kinds, and Wave 2's "in full" target hits the practical floor: every kind now has a working edit path (toggle for bool, cycle for select, drag/arrow for number, text-focus for text/textarea/color/file). The richer pickers belong with the Wave 10 primitives so the lift happens once and benefits every consumer (inspector field-rows + builder-page composition + Luau-authored components). |
| 2026-05-12 | **Waves 6, 7, 9, 10 land; Wave 8 deferred; Wave 11 substrates land.** Wave 6 was already wired during the B6 third-wave pass — reconfirmed and pin-tested. Wave 7 ships `prism-shell --scene <name>` + `--screenshot <path>` via `prism_shell::headless::{BuiltinScene, Shell::apply_scene, Shell::dump_frame}` — the screenshot path emits a deterministic JSON snapshot of the lowered UI tree (not PNG; that requires femtovg offscreen GPU plumbing in the backend, which lands as a follow-up without changing the CLI / scene / harness contracts). Wave 9 lands the `route:`, `bind:`, and `transition:` attribute namespaces through `AttributeNamespace::{Route, Transition}` + the existing `Bind` variant, all lowered via `interpret::apply_container_attributes` to `data-<key>` / `data-bind-<key>` / `data-transition-<key>` semantic attrs; `:state` selectors deferred because the existing Wave 1 `Hover` modifier covers the immediate need. Wave 10 ships `prism_builder::primitives::PRIMITIVES` — 14 `BlockSpec`s (`prism.text-input`, `prism.drag-scrub`, `prism.popover`, `prism.list-picker`, `prism.collapsible`, `prism.split-handle`, `prism.timed-overlay`, `prism.focus-trap`, `prism.select`, `prism.color-picker`, `prism.file-button`, `prism.resize-edge`, `prism.canvas-paint`, `prism.text-buffer`) with full prop schemas and minimal lower bodies — registered alongside `starter::register_builtins` in `register_document_builtins`; the `no_overlapping_block_ids…` named pin now asserts triplet disjointness across `shell.*` / builder builtins / `prism.*`. Wave 8 (Luau modifier parity) is explicitly deferred — the seams it consumes are all in place (`ModifierBehaviour`, `DocumentBindings`, `ReactiveProps`) but the mlua surface work (`LuauModifier` sibling registry, `prism.modifier {…}` global, modifier branch in `generate_signal_type_stubs`) is multi-PR scope better landed alongside Wave 11's first Luau-authored migration. Wave 11 ships the three substrates (Hover modifier, `route:` namespace, `<collapsible>` primitive) that the file-by-file migrations compose against; the actual ~50-component sweep is intentionally incremental per §11.5. **Workspace state: full test suite + clippy clean.** | The user's "implement the next big chunk" sequence walked Waves 3 → 4 → 6 → 7 → 9 → 10 → 11-substrate over two sessions, with Wave 5 (registry merge) already landed on 2026-05-11. The waves that benefit most from being landed *together* (the `route:` namespace + the `prism.*` primitive registry + the shell registration path that exposes both to the inspector) all sit in this commit — Wave 11's migration pipeline now has stable substrates to compose against. The pattern that emerged: when the seams the plan describes are already in place, the wave's substantive work is bookkeeping (named pin tests, registry merges, plan-document upkeep) rather than new runtime concepts; landing the seam table and proving the boot-time merge is the deliverable. Wave 8 is the next major scope because the LuauModifier sibling surface is genuinely new work; Wave 11's Tier-1 migrations are the next slow burn. |
| 2026-05-12 | **Wave 4 lands** — the signals panel becomes editable end-to-end. (4.1 / 4.2) The cursor + row-click + trash were already wired during the B6 third-wave pass; reconfirmed and documented. (4.3) `shell.connection-picker` overlay renders three field rows (source-signal / action-kind / target-label) with `data-role="connection-picker-field"` + `data-field` routing, and Add / Cancel buttons routed through dedicated `POINTER_ROUTES` entries. Action-kind cycles through all 7 `prism_builder::signal::ActionKind` variants — including `Bind`, the Phase 4 declarative one-way reactive binding from `docs/dev/dioxus-inspiration.md`. The "+ Add Connection" footer (`shell.add-connection-button`) is appended to every `shell.signals-panel` by default; its click dispatches `cmd signals.open-connection-picker` through the existing `route_on_click` chain — no new POINTER_ROUTES entry needed for the open path. (4.4) Five new `AppState` mutators (`add_signal_connection`, `update_signal_connection_field`, `open_connection_picker`, `close_connection_picker`, `cycle_connection_picker_action_kind`, `confirm_connection_picker`) cover the full edit surface; each writes through one of the C-wave resync seams. The picker's confirm path generates a stable kebab-case id (`c-<source>-<target>`) with `sanitise_id` + `uniquify_connection_id` so repeated picks of the same shape never collide. **Also landed: Wave 3.2 polish** — `palette-drag` JSON merged into `builder_canvas_props` via `props.rs`'s closure-style override; the canvas overlay layer now paints a translucent `data-role="palette-ghost"` rect at the cursor (`data-x` / `data-y` semantic attrs anchor it absolute, same convention as `selection-outline`) while a drag is in flight. **470 prism-shell tests, full workspace clippy clean.** Source / target text-input UX on the picker fields is the one explicitly-deferred piece — `data-role="connection-picker-field"` + `data-field` is wired today and the action-kind cycle works through that same handler; the two text fields surface their hover affordance immediately and wait for Wave 10's `<text-input>` primitive to take typed input. | The plan's §6 Wave 4 called for "the signals panel becomes editable" — the cursor + row-click + trash had landed during B6 already, so Wave 4's substantive work was the picker overlay (4.3) and the mutator surface (4.4). The picker mirrors the `shell.modifier-picker` shape from Wave 1.4 (overlay with target-id, list/form body, hidden-when-closed via §43-A2 `hidden_overlay`) so the "user-facing seam of composition" stays one vocabulary. Including `Bind` in the cycle list closes the loop with Phase 4 of `docs/dev/dioxus-inspiration.md` — declarative reactive binding is one click away from any node. The "+ button → cmd dispatch" path proves the §43 A1 inline-action grammar carries first-class entry points without growing a new POINTER_ROUTES row per affordance. Wave 5 was already landed 2026-05-11 (registry merge); the dependency-graph step the user named as "Wave 5" maps to the next remaining checklist item, which is Wave 6 (B6 third-wave remainder). |
| 2026-05-12 | **Wave 3 lands** — the canvas becomes pointer-driven. (3.1) The §43 B5 `route_canvas_node_select` already wired `Surface::hit_test_at` → `AppState::select_node` before drag-capture; reconfirmed and extended with bbox capture for the gizmo overlay. (3.2) `CatalogSlot::palette_drag` + three `AppState` mutators (`begin_palette_drag` / `update_palette_drag` / `end_palette_drag` + `cancel_palette_drag`) + `CanvasSlot::insert_under_node` make palette pick → canvas click → release insert a fresh node under the hit's `data-canvas-node` (or under the document root). New node moves the selection so the inspector/properties refresh; the palette pill clears so the drop is one-shot. (3.3) `CanvasSlot::selection_bbox` carries the click rect into `builder_canvas_props` as `selection-rect`, so the existing `build_selection_layer` paints the outline + 8-handle ring around the live bbox. `POINTER_ROUTES("resize-handle")` captures direction + transform snapshot; `update_resize_drag` translates the node per `(px, py) * (dx, dy)`. (3.4) `PointerButton::Secondary` on canvas-resident hits opens a context menu populated from `canvas_context_menu_items` (Move Up / Down / Duplicate / Copy / Cut / Delete on a selection; Paste on empty canvas); each row's `data-on-click="cmd <id>"` reuses the existing command-table dispatch. **447 prism-shell tests, full workspace clippy clean.** Visible palette-drag ghost overlay and per-node `width`/`height` mutation are deferred — both depend on layout API extensions that belong to later waves. | The plan's §5 (Wave 3) called for "the canvas becomes alive" — selection, drop, gizmo, context. The bones already existed (`Surface::hit_test_at`, `POINTER_ROUTES`, `state.canvas.pointer_down`/`pointer_move`/`pointer_up`); this wave activates them through one routing layer with no new event-loop concept. The `is_canvas_hit` helper is the single source of truth for "is this a canvas surface" — palette drag and right-click both gate on it, so future canvas-resident gestures plug in by adding to that set. The Wave 3 sequencing (`3.1 ✅ → 3.2 + 3.4 + 3.3`) followed §12's dependency graph — the structural seams from Wave 5 + the field-edit polish from Wave 2 were both already live. |
| 2026-05-12 | **Waves 7.3, 8, 9.2, 11.4, 11.5 land** — five remaining checklist items closed in one pass. (7.3) `prism visual` shells through `prism-shell --scene/--screenshot` directly; the macOS `screencapture` shim is gone, scene names mirror `BuiltinScene::ALL` via a named pin (`scene_names_match_shell_builtin_set`), and per-scene output extension is `.json` today (`.png` swaps in when Wave 7.2 PNG lands). (8) Luau parity for modifiers — `packages/prism-builder/src/luau_modifier.rs` ships a `LuauModifierRegistry` sibling to `LuauRenderRegistry`, a `prism.modifier {…}` global helper, a `ModifierBehaviour` impl that delegates schema/wrap/install_effects to Luau, `register_modifier_from_luau` first-class entry point, and `generate_modifier_type_stubs` codegen. `ReactiveProps` now has an `mlua::UserData` impl exposing `read/write/signal` so Luau-authored modifiers read/write per-key reactive signals through the same `Signal<Value>` UserData Rust uses. (9.2) `:state` selectors lower in `interpret.rs` via the new `prism_core::language::prism_ui::split_state_suffix` helper — `:hovered` folds into `ContainerProps.hover` (existing infra), `:selected`/`:focused` round-trip as `data-style-<key>-<state>` semantic attrs. (11.4) `prism.builder-host` joins the primitive registry as the 15th entry, lifting the shell's `shell.builder-canvas` composition into a primitive any `.prism-ui` document can compose against (props: `document` required, `viewport`, `show-selection`, `read-only`). (11.5) `prism dev shell` and `prism dev studio` now watch `packages/prism-shell/ui/` for `.prism-ui` edits alongside `src/` for `.rs` edits; `DEFAULT_EXTENSIONS = &["rs", "prism-ui"]` is the named pin guarding the filter. **Workspace state: 423 prism-builder tests with `--features luau`, 2108 prism-core tests, all suites green; default-feature clippy clean.** Wave 7.2 PNG remains the one explicit deferral — it needs either a tiny-skia software paint backend or a glutin pbuffer/EGL offscreen context (platform-specific, ~1000 lines either way) and belongs to backend-level work outside this seam. | The user's "everything tractable, one shot" target picked the items where the seams the plan describes were already in place. Wave 8 was the largest of these: the `ModifierBehaviour` trait shape, `DocumentBindings`, `ReactiveProps::signal(k)`, the `LuauRenderRegistry` thread-local pattern — all landed in Waves 1 + 4b + 6a. The mlua-surface work was bookkeeping the seams: 240 LoC of new module + 11 named pin tests. Wave 9.2 (`:state`) and Wave 11.4 (`prism.builder-host`) similarly closed long-anchored gaps with one helper / one BlockSpec each. Wave 11.5 was a five-line dev_loop extension turning the already-existing watcher into a true hot-reload pipe. The deferred items (Wave 7.2 PNG, Wave 11.2/11.3 file-by-file Tier-1/2 migrations) are the ones that genuinely need their own commits — PNG needs a backend, the 47 migrations need their own per-PR before/after frame-dump gates per §11.5 discipline. |
| 2026-05-12 | **Dedup pass over Wave 1.** Three independent patterns collapsed to one source each: (1) the twelve hand-rolled `ModifierBehaviour` impls (six baseline + six bootstrap) collapsed to a `BehaviourSpec` data struct + `SpecBehaviour` blanket impl, mirroring `BlockSpec`/`SpecBlock` from `prism_builder::block`. Each behaviour becomes one `const X: BehaviourSpec = BehaviourSpec::new(...).description(...).wrap(...)` row plus a row in `BUILTINS` / `BOOTSTRAP`; `register_specs(reg, &[&BehaviourSpec])` fans them in. (2) The three near-identical 20×20 routed-icon constructions in `shell.modifier-header` (toggle / remove / drag) collapsed to a `chrome::route_chip(id, icon, role, aria_label, extra_attrs)` helper. (3) The two near-identical `handle_modifier_{toggle,remove}_click` functions collapsed to one `indexed_modifier_route!($handler, $mutator)` `macro_rules!` invocation pair. Also pulled `ModifierRegistry::{list, descriptor}`'s duplicated descriptor projection into one `descriptor_from(&dyn ModifierBehaviour)` helper. Net: ~190 LoC deleted from `modifier.rs`, ~110 LoC deleted from `modifier_bootstrap.rs`, ~60 LoC deleted from `modifier_header.rs`. All 397 prism-builder + 421 prism-shell tests still pass; clippy clean. | The `BlockSpec` / `SpecBlock` pattern §32-§33 of the clay-migration plan established for document components is the right shape for *any* registered behaviour family with a uniform `(id, label, schema, optional wrap)` surface. Lifting it to modifiers means the same authoring grammar covers components, blocks, behaviours, and (in Wave 8) Luau-registered entries — one `const SPEC = …` row instead of N hand-rolled impls. The `route_chip` helper sets the pattern for every future per-row affordance (signal-connection trash, nav-page chevron, schema-row delete) so the next gizmo lands as a one-line call. The `indexed_modifier_route!` macro is small but proves the pattern for the next wave of routes (add-page chevrons, schema-row buttons) that follow the same "parse data-target-id + data-idx, call mutator" shape — adding one is one macro line. |
| 2026-05-12 | **Wave 11.2 loader seam + first 6 Tier-1 migrations land.** New module `packages/prism-shell/src/components/prism_ui_loader.rs` ships the `.prism-ui`-authored-shell-component pipeline: `PrismUiSpec` (declarative form mirroring `BlockSpec`), `PrismUiBlock` (runtime `Block` impl that lowers via `lower_document_with_scope` against a snapshot of `node.props` as scope bindings + `host_children` as a new `<host-children/>` DSL element), and a shared `Arc<OnceLock<Arc<dyn TagResolver>>>` populated post-registration via `finalize_prism_ui_resolver` so DSL blocks compose with each other and with native blocks against the live merged registry. Six components migrated as the first proof batch — `shell.toolbar-separator`, `shell.help-tooltip`, `shell.docs-view`, `shell.docs-sidebar`, `shell.toast-stack`, `shell.launchpad` — each one `.prism-ui` source + one row in `SHELL_PRISM_UI_COMPONENTS`. The corresponding Rust files were deleted; net ~566 LoC of Rust removed, ~116 LoC of `.prism-ui` added (plus the ~310-LoC loader infra paid once). DSL grammar gained two surgical closes: (a) bare `tag` / `role` / `aria-label` container attrs set the dedicated `Semantic` fields directly (no double-write through `attrs`), so a DSL author can express `<container tag="section" role="navigation" aria-label="Pages"/>` against the same struct the hand-rolled Rust blocks build via `Semantic::tag(..).with_role(..).with_aria_label(..)`; (b) `<host-children/>` emits the caller's pre-lowered `ctx.host_children()` verbatim, so DSL-side composition wrappers (toast-stack, launchpad, future app-window/dock-panel/properties-panel migrations) consume their children with one declarative element instead of a Rust seam. `LowerCtx::registry()` is a new pub accessor for future loader bodies that want a fresh `RegistryTagResolver` over the live merged registry without the post-registration OnceLock dance. The two binding-table invariants (`bindings_cover_every_registered_shell_block`, `slot_bindings_are_subset_of_shell_builtins`) were upgraded to walk both `SHELL_BUILTINS` and `SHELL_PRISM_UI_COMPONENTS` so the parity contract scales with the migration. **Workspace state: 473 prism-shell tests pass; full workspace test sweep (36 suites) and `cargo clippy --workspace --all-targets -- -D warnings` both clean.** Remaining ~27 Tier-1 components are deferred — each next migration is one `.prism-ui` file + one table row, no infrastructure changes; the patterns that block (boolean `||` in expressions, attribute spread for `for`-loop dispatch into a registered tag, conditional-single-attribute sugar) all surface as DSL extension follow-ups when their first consumer lands. | The plan's Wave 11 vision — "the shell becomes a Builder project, the DSL self-hosts" — needed the missing piece of infrastructure: a registered `Block` whose body is a parsed `.prism-ui` source. The loader is that piece. Smart-pattern shape mirrors `BlockSpec` + `SpecBlock` (declarative spec → runtime block) so authoring a DSL component reads the same as authoring a Rust one: one row in a table. The shared resolver cell breaks the chicken-and-egg between "block holds resolver" and "resolver enumerates blocks" with a one-time post-registration init — no thread-locals, no late-bound globals, no parallel registry. The two DSL grammar additions (`tag` / `role` / `aria-label` bare attrs, `<host-children/>` element) were the smallest possible changes to close real gaps; both unlock composition migrations (toast-stack, launchpad) that the §11.4 generalisation sweep referred to but couldn't actually land without these DSL surfaces. The six-component batch is intentionally conservative — proof-of-concept on the simplest visual leaves + the simplest composition wrappers; tomorrow's session picks up the next tier (`status-bar`, `app-card`, `inspector-row`, etc.) without rebuilding the loader. |
| 2026-05-12 | **Wave 11.2 substrate completion + third migration batch + `for, idx` iteration index.** Five DSL extensions land in one channel through `prism_core::language::expression` plus a small runtime addition for stable per-row ids. **Substrate (Pratt parser additions, single source):** `Question` / `Colon` / `Dot` / `&&` / `\|\|` / `!` tokens; `AnyExprNode::Conditional` for right-associative ternary; bare-identifier dotted-path chains (`item.label.0`); scanner suppression of leading-`.` floats after expression terminals so `tabs.0.name` tokenises correctly. **Runtime adapter** in `prism-ui-runtime::interpret::evaluate_expression` walks `LowerScope` bindings through JSON paths via a `ValueStore` impl. `eval_truthy` and templated-attribute interpolation try the cheap dotted-path lookup first, fall through to the full evaluator for operator-bearing bodies. **Dynamic dispatch:** `<dispatch component="{expr}" props="{expr}"/>` at the resolver level. **Conditional `data:` attrs:** runtime skips empty resolved values so ternary in `data:on-click="{cmd ? 'cmd ' + cmd : ''}"` omits the attr cleanly. **Iteration index:** `for="row, idx in rows"` binds the index as a typed `Number`. The properties-panel migration's first PR exposed the hit-cache constraint — dispatched containers only enter the cache when their `id` is non-empty, so the DSL had to be able to synthesise a stable per-iteration id (`id="props::row::{idx}"`). The runtime gained an optional `index_var` field on `ControlFlow::For` and a one-line LHS-parser extension; everything else is unchanged. **Third migration batch (six Tier-1 components):** `shell.toast`, `shell.menu-item`, `shell.signal-connection-row`, `shell.schema-row`, `shell.nav-page-row`, `shell.properties-panel`. Each was previously blocked on one of: ternary for kind→colour mapping (toast, row variants), C-style boolean for disabled-or-not-enabled normalisation (menu-item), dynamic dispatch (properties-panel). **Opportunistic dedup:** `shell.nav-button` + `shell.section-header` collapsed from two-branch if/else to single-container form via the new ternary substrate. Net ~1500 LoC of Rust deleted, ~250 LoC of `.prism-ui` added. **Workspace state:** 418 prism-shell tests pass (including the seven `production_click` integration tests that pinned the regression where empty dispatch ids dropped field-edit hits); full workspace `cargo test --workspace` (3596+ tests across ~25 suites) green; `cargo clippy --workspace --all-targets -- -D warnings` clean. **Remaining Tier-1 (~9 components — icon-button, inspector-row, app-card, status-bar, menu-bar-row, workflow-page-button, dock-tab, app-window, chrome.rs helpers):** none are blocked on DSL anymore; each is a focused per-PR migration whose net code reduction depends on whether the shared chrome helper it uses migrates with it. The user's "finish fully — smart patterns" target read as: (i) build the substrate so every future migration is mechanical, (ii) prove the substrate end-to-end with the migrations that needed it. Tier-2 (~14 stateful/gesture components) remains intentionally deferred per §11.5. | The plan's §2 Wave 11.2 explicitly named "the remaining ~15 Tier-1 components stay Rust pending one of four DSL extension follow-ups: ternary expressions, boolean `\|\|`, dynamic component dispatch, shared-chrome lift." This commit lifts the first three of those four into a single substrate by extending `prism_core::language::expression` rather than hand-rolling parser logic in `prism-ui-runtime` (the user's pinned "all external-format parsers must be built on prism-core::language::syntax's Scanner" memory). Reusing the existing Pratt parser meant the substrate cost was three small AST/scanner additions plus a JSON-aware `ValueStore` adapter (~120 LoC), not a fresh expression evaluator (~600 LoC). The iteration-index addition (`for, idx in xs`) was forced by a hit-cache constraint that didn't surface in the unit-test sweep — the production tree's `Surface::walk_for_hits` only emits HitRects for containers with non-empty ids, and dispatched containers with empty ids dropped silently. Adding an optional index binding (one Rust field, one LHS-parser branch, one test) gave the DSL author a way to author stable per-row ids without re-architecting the dispatch shape. The fourth substrate (shared-chrome lift) is the remaining ~9 Tier-1 components' next-tier friction — it doesn't need a new DSL feature, just per-component judgement on whether to factor a `<shell.tab-button>` DSL primitive (low return for two consumers) or inline the visual recipe (slightly more total DSL but mechanical). |
| 2026-05-12 | **Wave 11.2 substrate completion + third migration batch (superseded by the row above).** Closes the four DSL extensions the plan called out at §11.2 as blocking the remaining Tier-1 migrations, then ships six more migrations end-to-end. **Substrate (one path through Prism Syntax):** `prism_core::language::expression`'s Pratt parser gains `Question` / `Colon` / `Dot` / `&&` / `\|\|` / `!` tokens, an `AnyExprNode::Conditional { cond, then, else_ }` variant, right-associative ternary parsing (`expr := ternary`), and bare-identifier dotted-path chains (`item.label.0` → `Operand { id, subfield: "label.0" }`). The scanner's leading-`.` float form is suppressed when the previous token is an expression terminal (`Ident` / `Number` / `RParen` / `Operand`) so `tabs.0.name` tokenises as `Ident Dot Number Dot Ident` instead of swallowing `.0` into a float literal. `infer_node_type` + `validate_node_types` in `syntax::syntax` cover the new variant. `prism-ui-runtime::interpret::evaluate_expression` adapts the full evaluator to `.prism-ui`: a `ValueStore` impl walks `LowerScope` bindings through JSON paths (objects + array indices), returns owned `serde_json::Value`. `eval_truthy` and the templated-attribute interpolation paths now try the cheap dotted-path lookup first, fall through to the full evaluator for operator-bearing bodies. **Dynamic dispatch:** the resolver picks up `<dispatch component="{expr}" props="{expr}"/>` as a special tag — `component` interpolates to the target id, the lookup goes through the live registry, the `props=` spread unpacks an object onto the dispatched node. The properties-panel migration was the canonical first consumer (rows-with-component-field). **Conditional `data:` attrs:** the runtime now skips `data:`/`aria:` attrs whose resolved value is empty so authors can use ternary (`data:on-click="{cmd ? 'cmd ' + cmd : ''}"`) to omit the attr conditionally. Downstream `parse_action` was already a no-op on empty bodies — the omission is the correct behaviour, not a change. **Third migration batch (six Tier-1 components):** `shell.toast`, `shell.menu-item`, `shell.signal-connection-row`, `shell.schema-row`, `shell.nav-page-row`, `shell.properties-panel`. Each was previously blocked on one of: ternary for kind→colour mapping (toast, row variants), C-style boolean for disabled-or-not-enabled normalisation (menu-item), dynamic dispatch (properties-panel). Net ~1500 LoC of Rust deleted, ~250 LoC of `.prism-ui` added. **Opportunistic dedup:** `shell.nav-button` + `shell.section-header` collapsed from two-branch if/else to single-container form via the new ternary substrate (~30 LoC saved). **Workspace state:** 418 prism-shell tests pass; full workspace `cargo test --workspace --lib` (12 suites) green; `cargo clippy --workspace --all-targets -- -D warnings` clean. **Remaining Tier-1 (~9 components — icon-button, inspector-row, app-card, status-bar, menu-bar-row, workflow-page-button, dock-tab, app-window, chrome.rs helpers):** none are now blocked on DSL; each is a focused per-PR migration whose net code reduction depends on whether the shared chrome helper it uses migrates with it. The user's "finish fully — smart patterns" target read as: (i) build the substrate so every future migration is mechanical, (ii) prove the substrate end-to-end with the migrations that needed it. Tier-2 (~14 stateful/gesture components) remains intentionally deferred per §11.5 — each primitive's first authored consumer fills out the primitive's full body, so the two halves land together. | The plan's §2 Wave 11.2 explicitly named "the remaining ~15 Tier-1 components stay Rust pending one of four DSL extension follow-ups: ternary expressions, boolean `\|\|`, dynamic component dispatch, shared-chrome lift." This commit lifts the first three of those four into a single substrate by extending `prism_core::language::expression` rather than hand-rolling parser logic in `prism-ui-runtime` (the user's pinned "all external-format parsers must be built on prism-core::language::syntax's Scanner" memory). Reusing the existing Pratt parser meant the substrate cost was three small AST/scanner additions plus a JSON-aware `ValueStore` adapter (~120 LoC), not a fresh expression evaluator (~600 LoC). The "smart pattern" the user asked for surfaces as: one Prism Syntax module owns the expression grammar; every consumer (formula-field evaluator, .prism-ui attribute interpolation, `if=` predicates, dynamic dispatch) is a thin `ValueStore` adapter on top. The fourth substrate (shared-chrome lift) is the remaining ~9 Tier-1 components' next-tier friction — it doesn't need a new DSL feature, just per-component judgement on whether to factor a `<shell.tab-button>` DSL primitive (low return for two consumers) or inline the visual recipe (slightly more total DSL but mechanical). |
| 2026-05-12 | **Wave 11.2 second batch + DSL/resolver substrate generalisations.** Twelve more Tier-1 components migrate to `.prism-ui` source — `shell.explorer`, `shell.docs-content`, `shell.section-header`, `shell.nav-button`, `shell.inspector-tree`, `shell.nav-page-list`, `shell.signals-panel`, `shell.workflow-page-bar`, `shell.menu-dropdown`, `shell.context-menu`, `shell.add-modifier-button`, `shell.add-connection-button`. Total Tier-1 count is now 18 (the original 6 + this 12). Net ~2000 LoC of Rust deleted; ~250 LoC of `.prism-ui` added. **Four DSL/resolver substrate generalisations** unblocked the batch by removing the per-migration friction: (a) `<image src=… width=… height=… style:radius=… style:tint=…/>` element in `prism-ui-runtime::interpret::image_from` closes the chevron/icon gap that blocked `shell.section-header` and `shell.nav-button`; (b) `lookup_expression` grew dotted-path support (`{item.label}`, `{tabs.0.name}`) so a `for="item in items"` loop over `Vec<Object>` addresses typed fields, and `eval_truthy` re-uses the same lookup so `if="{row.selected}"` works identically; (c) `RegistryTagResolver` now resolves attribute interpolations through scope before building the dispatched `BuilderNode` — pure `{expr}` returns the underlying JSON value verbatim (preserves typed arrays / objects / numbers / bools across the dispatch seam), templated `prefix-{expr}` resolves to a string, the new `resolved_attribute_value` / `resolved_attribute_string` helpers replaced the old `literal_attribute_value` shape that round-tripped `{item.label}` as the literal string `"{item.label}"`; (d) `props="{expr}"` spread attribute on the resolver path unpacks a JSON object into the dispatched node's props so `<shell.nav-page-row for="item in pages" id="{item.page-id}" props="{item}"/>` forwards the row's full schema in one expression — sibling bare attrs layer on top, non-object spreads are no-ops. Two more low-friction loader closes: (e) `PrismUiBlock::lower_ui` seeds schema defaults into scope before applying `node.props`, so missing-but-defaulted props (`show-add=true` on `signals-panel`, `attached=[]` on `add-modifier-button`) inherit the Rust-side default without re-stating it in the DSL; (f) a new `register_full_shell_chrome` helper composes `register_shell_builtins` + `register_prism_ui_components` + `finalize_prism_ui_resolver` into one call — the single source of truth for "what chrome the shell ships," shared between `Shell::new` and any test that needs the complete registry. The two Wave 1 buttons (`shell.add-modifier-button`, `shell.add-connection-button`) migrated as well, validating that DSL can carry `data-on-click="cmd <id>"` route attrs and dispatch through the existing `route_on_click` command-table chain without a Rust block. **Workspace state: 3500 tests pass across 36 suites; `cargo clippy --workspace --all-targets -- -D warnings` clean.** Remaining ~15 Tier-1 components stay Rust pending one of four DSL extension follow-ups: ternary expressions (`toast` kind→color mapping, selected/non-selected row variants on `signal-connection-row` / `schema-row` / `nav-page-row` / `inspector-row` / `app-card`), boolean `||` (`menu-item` enabled/disabled normalisation), dynamic component dispatch (`properties-panel` rows-with-component-field), or a shared-chrome lift (`workflow-page-button` ← `dock-tab` share `chrome::active_underline_tab`; `icon-button` ← `inspector-row` share `chrome::icon_button_node_tinted`). Each surfaces as a focused DSL extension when its first consumer needs it. | The user's "finish implementing the waves fully — use smart patterns to minimise duplication" target read as: (i) keep walking the Tier-1 migration list while it's still tractable, (ii) lift each new constraint into the DSL/resolver substrate rather than re-stating it per migration. The four substrate generalisations are exactly that: each removes a per-migration friction (image authoring, field access in iteration, type-preserving prop forwarding, schema-driven defaults) so the next batch of migrations gets shorter, not longer. The `register_full_shell_chrome` helper is the DI seam version of the same idea — the production bootstrap and every test now register the chrome catalog through one declarative call, so adding a Wave 3 batch of migrations doesn't fork three call sites. The remaining-15 list is the next tier of friction; once any one of them ships, the DSL grows the matching expression construct and the rest of that group falls in line. |
| 2026-05-12 | **Wave 12 lands — Vue/React-style style prop passing + `shell.component-palette` migration.** Closes the composition seam the plan's §11 vision had left implicit: a parent component can now pass styling down to a child component instance through the same surface React/Vue use. Two cooperating substrate moves: (a) `apply_style_override(props, key, value)` lifted into `prism-ui-runtime::interpret` as the single source of truth for the `style:` vocabulary (`background`, `radius`, `padding[/-left/-right/-top/-bottom]`, `gap`, `width`, `height`, plus `:hovered` overrides for background + radius); the pre-existing `apply_container_attributes`'s `AttributeNamespace::Style` branch collapses to one line of delegation. (b) `attach_style_overrides(node, element, scope)` in `prism-builder::ui_resolver` collects every `style:*` attribute and `style="{obj}"` spread on the source element, then applies them to the lowered container AFTER `Component::lower_ui` returns — mirrors the existing `attach_on_handlers` pattern. The block computes its natural styling first; the parent's overrides win. The `style="{obj}"` spread parallels the Wave 11.2 `props="{item}"` spread (one JSON object → many per-key overrides) and the resolver swallows the bare `style` key so blocks never see it as a stray prop. **First DSL consumer:** `shell.component-palette` migrates to `.prism-ui` source alongside the substrate; the row container's selected-vs-hover branch is expressed as ternary `style:background` / `style:background:hovered` overrides resolved against `selected-id == item.item-id`. Net Wave 12 footprint: ~250 LoC of Rust deleted (`components/component_palette.rs`), ~50 LoC of `.prism-ui` added, ~95 LoC of substrate helpers (one helper in `interpret.rs`, one helper + two attribute-table touches in `ui_resolver.rs`), six new resolver-side tests (`style_namespace_overrides_lowered_container_background`, `style_spread_object_unpacks_each_key_as_override`, `style_namespace_overrides_with_state_suffix_route_to_hover`, `style_overrides_unknown_bare_key_drops_silently`, `style_spread_value_is_not_visible_as_block_prop`, `style_overrides_apply_through_dynamic_dispatch`). **Workspace state**: 385 prism-shell tests + the full `prism-builder` + `prism-ui-runtime` suites green; clippy clean. | The user's "components should be able to reference and use other components (just like Vue / React — style props can be passed from one to another)" framing called out the missing composition seam directly: tag-dispatched components had no way for the parent to reshape the child's container styling without forking the child block. The substrate move is minimal because the seams the plan had already built (`RegistryTagResolver`, post-lower `attach_on_handlers`, the `props="{item}"` spread, the `apply_container_attributes` style branch) lined up perfectly — Wave 12 is one helper extracted from existing code + one sibling post-lower attach. The "child computes natural styling, parent overrides on top" semantics is the actual React / Vue model: the child stays decoupled from the parent, and the parent's `style={…}` reaches the final container without needing the child to expose every styling knob as a typed prop. The `style="{obj}"` spread is the Vue equivalent of `:style="theme.button"` / React's `style={{...theme.button}}` — both reach the same fan-out path, both behave identically. The `shell.component-palette` migration was chosen as the first consumer because its selected-vs-hover branch is the canonical pattern this substrate exists for: a list-row whose visual changes based on data without forking the row block. Future Tier-1 migrations (`app-card`, `inspector-row`, `app-window`) will use the same shape for their hover/select tints. |
| 2026-05-12 | **Wave 11.3 batch 2 — gizmo + drag-number-field migration + chrome::gizmo_* cleanup.** Four more components migrate to `.prism-ui` source: `shell.gizmo-move`, `shell.gizmo-rotate`, `shell.gizmo-scale`, `shell.drag-number-field`. Total DSL-migrated shell components now sits at **46**, with only **13 native** remaining (`builder_canvas`, `code_editor`, `command_palette`, `dock_panel`, `dock_workspace`, `field_editor`, `nav_graph`, `schema_designer`, `transform_editor`, plus four infra modules — `chrome`, `mod`, `prism_ui_loader`, `registry`). The three gizmos collapse to ~30 LoC DSL each (was 84-105 LoC native + chrome helpers); the drag-number-field migrates by adding a `display-value` prop the binding pre-formats via Rust's `format_drag_value` (the DSL has no numeric-formatter expression). **Dead-code cleanup**: `chrome::gizmo_axis_arm`, `chrome::gizmo_handle`, `chrome::gizmo_root` deleted — last Rust consumers gone. `chrome.rs` shrinks by another ~85 LoC. The `chrome::drag_number_field_node` + `chrome::format_drag_value` + `chrome::color_or_transparent` helpers stay: `field_editor` and `transform_editor` (both still native, Tier-2-deferred) consume them. **Workspace state**: 321 prism-shell tests + full `cargo test --workspace` (36 suites, 0 failures) green; `cargo clippy --workspace --all-targets -- -D warnings` clean. **Remaining Tier-1/Tier-2** (deferred with real Rust dependencies, not DSL gaps): `command_palette` (needs `UiNode::TextInput` keystroke seam — Wave 10's `prism.text-input` body), `field_editor` (TextInput + HSL color picker), `transform_editor` (drag-number-field composition pattern with 5+ instances per row), `schema_designer` (multi-row form), `dock_panel` (host-specific `PanelKind::tag_for` routing — could DSL with `<dispatch>` once routing table is pre-computed), `dock_workspace` (recursive DockNode walk — Tier-3), plus the three primitive-stand-in Tier-3 components (`builder_canvas` → `prism.builder-host`, `code_editor` → `prism.text-buffer`, `nav_graph` → `prism.canvas-paint`). | The user's "continue" target — Tier-2 migrations are now constrained by genuine runtime dependencies, not by the DSL substrate. The gizmo migration was pure visual composition with no consumers needing the chrome::gizmo_* helpers post-migration — the canonical "rule of three drops to one" pattern: once every consumer migrates, the helper goes too. The drag-number-field `display-value` prop is the smart pattern for numeric formatting without growing the DSL expression vocabulary: the binding side stays in Rust (where rounding/trimming is one function call) and the DSL renders the pre-formatted string verbatim. The same pattern will apply when `transform_editor` migrates — its 5+ drag-number embeds will each accept a pre-formatted `display-value` from a `transform_props` helper that the slot's binding owns. The remaining 13 native components have legitimate Rust seams: TextInput (keystroke focus), retained-mode canvas paint (femtovg), tree walking with host routing tables — each is its own architecture call, not a DSL extension. |
| 2026-05-12 | **Wave 13 + Tier-1/Tier-2 closeout lands.** Survey of upstream DSL patterns (Dioxus / React / Vue / Svelte) maps eight candidates to concrete Prism gaps; three are implemented this commit (the rest deferred behind explicit reasons — no consumer waiting, or blocked on a primitive body). **13.1 — named slots.** `<slot name="X"/>` reads pre-lowered children from a new `LowerScope::host_children_by_slot: Arc<HashMap<String, Vec<Node>>>`. Resolver-side `partition_children_by_slot` in `prism-builder::ui_resolver` reads each AST child's `slot="X"` bare attribute, lowers each bucket in the caller's scope, and threads the resulting map through a new `LowerCtx::with_host_children_by_slot` field. The DSL loader (`PrismUiBlock::lower_ui`) propagates the map onto its `LowerScope` so `<slot name="X"/>` reads pull from the right pile; falls back to the element's own children when neither AST-level `SlotBindings` nor the pre-lowered map carries the requested slot. **13.2 — `on:event` namespace.** Already present pre-Wave-13 — `AttributeNamespace::On` parsed and lowered to `data-on-<event>` in both `apply_container_attributes` and the resolver's `attach_on_handlers`. Documented here for completeness. **13.3 — `use:` directive namespace.** Author writes `<container use:hover use:tooltip="Click to save"/>`; the runtime lowers each `use:<id>` to a `data-use-<id>="<value>"` semantic attr so author intent round-trips through the hit-test / SSR caches. Full runtime modifier-fold integration is a follow-up — same data-round-trips-now / behaviour-follows pattern Wave 9.4 (transitions) and Wave 9.2 (:selected / :focused state styles) use. **Migrations:** five more Tier-1/Tier-2 components ship as DSL. (a) `shell.builder-toolbar` (532 LoC of Rust → 100 LoC of `.prism-ui`) — uses `for="entry in align-entries"` + `for="entry in device-entries"` over schema-default JSON tables to express the const ALIGN_ENTRIES / DEVICE_ENTRIES tables; pre-formatted `zoom-label` prop on the binding emission means the DSL skips arithmetic. (b) `shell.app-window` (379 LoC → 65 LoC) — **first consumer of Wave 13.1 named slots**. The body is `<slot name="menu">…fallback…</slot><container body…><slot name="nav">…fallback…</slot>…<host-children/></container><slot name="status">…fallback…</slot>` so a caller can either override individual slots or accept the prop-driven defaults. The existing `<shell.app-window><shell.dock-workspace/></shell.app-window>` skeleton uses zero slot overrides and the dock-workspace lands as the default `<host-children/>`. (c) `shell.dock-divider` (Tier-2, 120 LoC → 15 LoC) — orientation/length resolved via nested ternary. (d) `shell.resize-handle` (Tier-2, 135 LoC → 15 LoC) — 8-arm aria-label cascade + 8-arm cursor cascade via ternary. **Total**: Wave 13 + migrations: ~1170 LoC of Rust deleted, ~210 LoC of `.prism-ui` added; one new substrate `partition_children_by_slot` (~50 LoC) + two new `LowerScope` methods + one `LowerCtx` field. **Workspace state**: 330 prism-shell tests + full `cargo test --workspace` (36 suites) green; `cargo clippy --workspace --all-targets -- -D warnings` clean; named-slot tests `slot_named_falls_through_to_host_children_by_slot_map` + `slot_unknown_name_falls_back_to_fallback_children` pin the runtime contract, and `use_namespace_lowers_to_data_use_attr` pins the directive namespace. **Remaining native Tier-1**: `shell.command-palette` (needs `UiNode::TextInput` keystroke seam — blocked on Wave 10's `prism.text-input` getting a real body), three gizmos (retained-mode paint, Tier-2/Tier-3 by §11.2), and `chrome.rs`'s `icon_button_node` helper (last Rust consumer is gone after this commit — the helper now exists only for any future Rust block that wants to compose an icon button without going through the DSL resolver, and can be deleted once its `_unused` warning surfaces). | The user's "document patterns first, then implement, then finish waves" framing inverted the usual diff order — Wave 13 is the architecture survey + implementation, then Wave 11.2 batch 6 (app-window, builder-toolbar, dock-divider, resize-handle) is the Wave-13 consumers landing alongside. The named-slot implementation cost was small because (a) `SlotBindings` already existed for AST-level slots and (b) `host_children_ui` already existed for pre-lowered single-slot. Wave 13.1 is the union of those two — pre-lowered *named-slot* map — which folds into existing seams (`LowerScope` clones, `LowerCtx` builder methods) without new modules. The `<slot name="X">…fallback…</slot>` semantics is the Web Components contract verbatim: every modern component framework converged on it because the fallback-content shape lets a component author offer sensible defaults while preserving overrideability. The Wave 13.3 `use:` deferral mirrors the Wave 9.4 / Wave 9.2 pattern: data round-trips immediately (semantic attrs), behaviour wiring (modifier-fold integration) lands when the resolver-side modifier-registry thread-through ships. Each of the four deferred Wave 13 patterns (two-way `bind:value`, reconciliation `key=`, `<fragment>`, `v-show`) has a one-line "Why deferred" entry in the §Wave-13 substrate table — no consumer waiting, or the substrate it would compose with isn't there yet. |
| 2026-05-12 | **Wave 11.2 batch 5 — overlay-gate pattern + app-card + dead-chrome cleanup.** Six more Tier-1 / overlay components migrate to `.prism-ui` source: `shell.app-card`, `shell.modifier-header`, `shell.modifier-picker`, `shell.connection-picker`, `shell.component-picker`, `shell.dock-tab-bar`. Total DSL-migrated shell components is now 31 (Batches 1-4: 29 + Batch 5: 2 Tier-1) plus 4 overlays. **New substrate pattern: top-level `if`/`else` overlay gate.** The four picker overlays (`modifier-picker`, `connection-picker`, `component-picker`) author themselves as two top-level branches — a closed branch (`<container if="{!open}" width="0" height="0" data:visible="false" aria:hidden="true"/>`) and an open branch (`<container else …>full body…</container>`) — the loader's `collapse_to_single_root` picks whichever survives. Replaces the per-block `chrome::hidden_overlay(node.id, "<role>")` helper call; the DSL author expresses the closed/open shape declaratively. **Dead-code cleanup**: `chrome::route_chip` (was used only by modifier-header, now DSL), `chrome::indent_dot` (was test-only since the inspector-row migration), `chrome::icon_button_node_tinted` + the `tinted_image_node` import path collapsed into `chrome::icon_button_node` because the `tint` path was always `None` after the prior migrations. Net `chrome.rs` shrinks by ~120 LoC. **App-card migration**: the 9-arm ternary cascade for icon-name → asset-path mapping (`{icon == 'globe' ? 'icons/globe.svg' : icon == 'music' ? 'icons/grip.svg' : …}`) is the first proof that the substrate handles real-world string-lookup tables without needing a new DSL feature. **Composition-via-spread pattern**: `shell.dock-tab-bar` is one container with a `for="tab in tabs"` + `<shell.dock-tab props="{tab}"/>` — the resolver-side `props="{expr}"` spread (Wave 11.2) carries the full row schema through to the dispatched tag without enumerating each field. Net: ~1500 LoC of Rust deleted across the six component files + chrome.rs sweep, ~280 LoC of `.prism-ui` added. **Workspace state**: 349 prism-shell tests + full workspace `cargo test --workspace` (36 suites) green; `cargo clippy --workspace --all-targets -- -D warnings` clean. **Remaining Tier-1**: `shell.app-window` (host-children composition seam — keeps a small Rust body for now), `shell.builder-toolbar` (uses `chrome::icon_button_node` plus complex drag-scrub fields), `shell.command-palette` (needs the `UiNode::TextInput` keystroke seam — blocked on Wave 10's `prism.text-input` getting a real body), and the three gizmos (retained-mode paint, Tier-2 by §11.2 inventory). Tier-2 remains intentionally deferred per §11.5. | The user's "finish fully — use smart patterns, cleanup old dead code" target closed the four remaining easy-to-migrate Tier-1 components (`app-card`, `modifier-header`) plus four overlays that had been sitting on the `chrome::hidden_overlay` helper. The overlay-gate pattern is the smart pattern: the closed-when-`open=false` semantics were a Rust function call before — now they're a 6-line DSL idiom every overlay shares. The chrome.rs sweep is the corresponding cleanup: when the last Rust consumer migrates, the helper goes too. The icon-cascade in `shell.app-card` (9-arm ternary across the `icon` prop) is the kind of mapping table that would have read as "we need an `icon-registry` substrate" — instead it's one inline expression, proving the Wave 11.2 expression substrate is enough for table lookups. The `dock-tab-bar` composition shows the `for` + `props="{item}"` spread is the right shape for "iterate a JSON array and dispatch each row to a registered tag" — the next gizmo migration will likely use the same pattern over `{handles}` / `{axes}`. |
| 2026-05-13 | **Wave 15 lands — iteration / loop vocabulary.** Survey of 12 loop patterns across Rust / Python / Vue / Svelte / Solid / JSX / SwiftUI / Compose maps three to concrete Prism gaps; the rest deferred or skipped per the table in §Wave-15. **15.1 — `else` after `for=` runs on empty iteration.** Two-line change in `expand_control_flow`: the previous `chain_taken = None` reset becomes `Some(!iter.is_empty())`, so an empty `for` lets a subsequent `else` / `else-if` sibling fire while a non-empty `for` suppresses it. Matches Svelte's `{#each items}{:else}…{/each}` shape verbatim. **15.2 — numeric range `for="i in 0..n"` / `for="i in 0..=n"`.** `split_range` peels the inclusive `..=` form before the exclusive `..` form so authors get both Rust shapes. Both endpoints resolve through the full expression evaluator (`for="i in 0..items.length"` works), reverse ranges yield zero items (no panic), and the iteration variable binds as a typed `Number` so downstream `{i * 4}` arithmetic reads through `parse_f32` uniformly. **15.3 — object iteration `for="value, key in obj"`.** When the source resolves to a `Value::Object`, iterate insertion-ordered `(key, value)` entries; the optional second LHS variable receives the string key (the same slot the integer index uses for arrays / ranges). Three runtime paths unified into one `resolve_for_iteration` helper that dispatches on source shape — array (the pre-Wave-15 path), object (15.3), or range string (15.2). 9 new pin tests cover the contract end-to-end (`else_after_empty_for_renders_fallback`, `else_after_non_empty_for_is_suppressed`, `else_if_after_empty_for_evaluates_predicate`, `range_exclusive_iterates_start_through_end_minus_one`, `range_inclusive_iterates_start_through_end`, `range_with_descending_endpoints_yields_zero_items`, `range_endpoints_resolve_through_scope_bindings`, `for_loop_over_object_iterates_entries_in_insertion_order`, `for_loop_over_object_binds_key_to_second_lhs_variable`, `for_loop_index_variable_still_works_on_arrays`). **First Wave 15 consumers**: `shell.signals-panel` gains "No connections yet." empty-state via `<text else>` after the `for="item in connections"` row dispatch; `shell.nav-page-list` gains "No pages yet." through the same shape. Net behaviour change: zero-row panels no longer render as a bare add-button — the empty state lands one DSL sibling above. **Workspace state**: 100 prism-ui-runtime tests + 320 prism-shell tests + full `cargo test --workspace` (39 suites) green; `cargo clippy --workspace --all-targets -- -D warnings` clean. **Deferred (no consumer waiting / data-layer cleaner)**: step, reverse, zip, chunk, keyed-iteration (blocks on incremental tree-diff). **Skipped (wrong substrate)**: `while` / `do-while` / `break` / `continue` / generators — declarative tree render is bounded; range destructuring is covered by `props="{item}"` + dotted-path access. | The user's "any other loop / iterator / enumerator patterns?" question pointed at the iteration-vocabulary gap Wave 13 + 14 hadn't covered. The pattern that emerged: **the most-cited missing iteration affordance across every framework family is the empty-state fallback** — Svelte calls it `{:else}` inside `{#each}`, Vue authors it as a sibling `v-if=length>0` / `v-else` pair, Solid bakes it into `<For each= fallback=>`. Prism's symmetric solution is the smallest: empty `for` already produced zero output, so the only change is "let the chain flag carry through" — two lines of code, no new attribute, no new element. Range and object iteration are the same shape extension: one resolver in `resolve_for_iteration` dispatches on JSON shape (array / object) or string parse (range). The third pattern family (`while` / `break` / generators) is the cleanest skip — it's imperative, not declarative, and the bounded retained-mode tree render has no halt condition to honour. The signals-panel + nav-page-list migrations prove the substrate's "one DSL sibling" authoring win — every list-rendering shell block can now author its empty state inline instead of forking the parent's `if=` branching. |
| 2026-05-12 | **Wave 14 lands — DSL substrate from HTMX / CSS / SwiftUI / Compose + dead-code sweep + `shell.inspector-row` migration v2.** Survey of the four other UI-authoring framework families (HTMX, CSS custom properties + container queries, SwiftUI / Compose modifier chains, Solid / Lit / Astro / Alpine) maps thirteen patterns to concrete Prism gaps; three are implemented this commit (the rest deferred behind explicit reasons: no consumer waiting, wrong substrate, or blocked on a primitive body). **14.1 — design tokens as scope binding.** `LowerScope::with_design_tokens(&DesignTokens)` seeds the workspace `DEFAULT_TOKENS` table as a `tokens` binding via a JSON projection. Colour fields emit as `#rrggbbaa` strings (the same shape `parse_color` consumes); spacing / radius / typography emit as JSON numbers (`parse_f32` reads them verbatim). One source of truth for shell visual constants across every migrated `.prism-ui` file; one binding seed. Injected at two seams in lockstep: `prism_shell::render::render_tree` (skeleton path) and `PrismUiBlock::lower_ui` (per-block path). Tests: `interpret::tests::{tokens_binding_resolves_color_in_style_namespace, tokens_binding_resolves_nested_path_through_style}` + `prism_ui_loader::tests::tokens_binding_resolves_in_loader_dispatched_block`. **14.2 — block-local `<let name= value=/>`.** Sibling-level constants à la Svelte `{@const}` / SwiftUI `let`. The `expand_control_flow` sibling pre-pass recognises `<let>` elements, evaluates their `value="{expr}"` against a running let-scope (typed-aware via the new `evaluate_bare_attr_typed` so a number stays a number), and threads the resulting scope as the per-element scope of every subsequent sibling. Doesn't render. Inherits through `if=` / `else-if=` / `else` predicates *and* through `for=` loop child scopes — same propagation rule the new `active` local at the head of `expand_control_flow` uses everywhere. Tests: `interpret::tests::{let_binding_propagates_to_subsequent_siblings, let_binding_shadows_parent_scope_for_subsequent_siblings}`. **14.3 — `on:event.modifier` suffix round-trip.** `on:click.once` / `on:click.stop` parses cleanly (one-char widen on the attribute-name scanner: `.` joins `_`, `-`, `:`) and lowers to `data-on-click-once` / `data-on-click-stop` semantic attrs so the existing hit-test cache picks the suffix up uniformly. Today no consumer reads the modifier; data carries author intent for future runtime wiring — same pattern Wave 9.4 (transitions) / 13.3 (`use:`) use. Test: `interpret::tests::on_namespace_modifier_suffix_round_trips_as_data_attr`. **Wave 14 first consumer**: `shell.inspector-row` migrates to use four `<let>` rows (`is-row`, `is-empty`, `is-node-row`, `active-chevrons`) replacing the seven occurrences of `kind == 'row'` / `kind == 'empty'` / `selected && kind != 'row' && kind != 'empty'` checks. Net visual is unchanged; the file reads ~40% shorter at the predicate-heavy attrs. **Dead-code cleanup**: `chrome::icon_button_node` + its three constants (`ICON_BUTTON_SIZE` / `ICON_BUTTON_RADIUS` / `ICON_GLYPH_SIZE` / `ICON_BUTTON_HOVER_BG`) deleted — the last consumer (`shell.inspector-row` chevrons) migrated to the DSL `<shell.icon-button>` block in a previous wave; the three associated `#[cfg(test)]` tests in `chrome.rs` deleted. `prism_builder::ui_lower::tinted_image_node` deleted — zero consumers remained workspace-wide. Net `chrome.rs` shrinks by ~95 LoC; `prism-builder/src/ui_lower.rs` shrinks by ~25 LoC. **Workspace state**: 320 prism-shell tests + full workspace `cargo test --workspace` (every suite) green; `cargo clippy --workspace --all-targets -- -D warnings` clean. **Deferred Wave 14 patterns (table in §Wave-14)**: `<teleport to="…"/>` (no consumer waiting — overlays already authored at root in `app.prism-ui`), `hx-confirm` (service-level concern not DSL), container queries (parent-prop fan-out already covers viewport branching), animate:in/out (Wave 9.4 `transition:` covers the data round-trip, animator follow-up), two-way `bind:value` (blocked on `prim.text-input` body). The skipped rows (Tailwind `@apply`, Alpine `x-cloak`, Astro `client:load`) are wrong-substrate moves — Prism doesn't have classes, doesn't have an FOUC window, doesn't hydrate. | The user's "look beyond Dioxus/React/Vue/Svelte" framing pointed at the four other framework families that converged on the same authoring problems from different angles: HTMX (server-driven HATEOAS, event-modifier vocabulary), CSS (design tokens, container queries, state pseudo-classes), SwiftUI/Compose (typed-view + modifier chain), and the post-React reactive crop (Solid/Lit/Astro/Alpine). The pattern that emerged: **the gaps left in Wave 13 are mostly about *scope-binding vocabulary*, not new attribute namespaces**. Tokens are the missing global binding (every migrated `.prism-ui` file currently hardcodes hex / px); `<let>` is the missing block-local binding (every multi-arm ternary in `inspector-row` / `app-card` repeats its kind-check). Both fold into the existing dotted-path resolver with zero new runtime concepts. The event-modifier suffix is the lowest-cost ergonomic alias Vue / Svelte authors expect to type. The dead-code sweep is the corollary the user named: `chrome::icon_button_node` has been unused for two waves, just waiting for an `_unused` warning to surface — Wave 14's `inspector-row` re-migration (using the new `<let>` substrate) was the natural moment to delete it. The next consumer for tokens will be `shell.inspector-row`'s remaining hex literals (`#26000000`, `#0a000000`, …); landing the substrate now means each future migration just uses `tokens.colors.*` without re-stating the seam. |
| 2026-05-13 | **Wave 2.4 / 2.5 / 7.2 close the last three `[~]` partials.** Three field-edit + capture gaps the plan had been carrying as deferred all land in one pass. **Wave 2.4 (color picker)**: new `ColorPicker` overlay state on `OverlaySlot`, three new mutators (`open_color_picker` / `close_color_picker` / `set_color_picker_value`), three new `POINTER_ROUTES` arms (`color-swatch` / `color-preset-select` / `color-picker-close`), one new `.prism-ui` block (`shell.color-picker`), one new prop binding. The field-editor color swatch now carries `data-target-id` + `data-key` + `data-value` so a click opens the overlay anchored to the right prop. Overlay body: 48px preview swatch + hex echo + an 8-swatch preset row keyed off `ColorPicker::PRESETS`. Preset clicks commit through `set_node_prop` and the picker stays open for further preview; Close dismisses. The hex text input keeps the field-focus paste/type/Enter pathway intact. Full HSL slider gestures wait for `prism.drag-scrub` body (Wave 11.4 follow-up). **Wave 2.5 (file dialog)**: `Vfs` trait grows `pick_file(&FilePickerSpec) -> Result<Vec<PathBuf>, _>`; default returns `VfsError::Unsupported`. `OsVfs` overrides via `rfd::FileDialog` when `feature = "native"` and `target_arch != "wasm32"`. `FilePickerSpec::with_accept_attr` parses the `<input accept="…">` shape into rfd filters. Field-editor's file-kind body renders the existing text input alongside an explicit "Browse…" button (`data-role="file-browse"`); the pointer router fires `Vfs::pick_file` and commits the first picked path through `set_node_prop`. The text-input fallback survives for paste workflows and headless hosts. **Wave 7.2 (PNG screenshot)**: `prism-shell --screenshot <path>` now picks PNG vs JSON from the file extension. The `.png` branch runs a new software rasteriser in `prism_shell::png_paint` over the `RenderCommand` stream from `prism_ui_runtime::layout::compute` (same Taffy pass the femtovg backend uses, so layout is identical), encodes RGBA via the workspace `image` crate. Painter handles solid rectangles with rounded-corner masking, scissor clipping, alpha blending, borders, image-tint placeholders, text-baseline accent strips. Glyph rasterisation / real image decoding wait for a real offscreen femtovg surface; the encoder seam in `Shell::dump_png` swaps target without touching the CLI / scene contract. New CLI flags `--width` / `--height` (default 1280×800) set the headless viewport. **Workspace state**: 387 prism-shell tests pass (up from 372 — 15 new tests across the three waves). All five `[~]` partials in §2 are now `[x]`; the only remaining open items are explicit follow-ups tracked elsewhere (HSL slider body alongside `prism.drag-scrub`'s first consumer; femtovg offscreen GL for pixel-equal PNGs; Tier-2 Rust survivors in §11.3 with documented substrate-specific blockers). | The three partials shared a shape: the plan said "the seam is in place; the polish wants its own commit." For 2.4 / 2.5 the work was one mutator family + one POINTER_ROUTES row + one new DSL block (or for 2.5, one new trait method + one optional dep). For 7.2 the deferral was load-bearing — the plan said "femtovg offscreen needs GPU plumbing." Reading "PNG" as "the file format" rather than "the backend" — and using `layout::compute` (which is GL-free) — opened the path. The software rasteriser is intentionally a layout-equal-but-not-pixel-equal sibling of the femtovg path; when a real offscreen surface lands it drops in at the encoder seam. The net is 0 remaining `[~]` partials in the plan and a +15-test floor on `prism-shell`. |
| 2026-05-12 | **Wave 11.2 chrome-lift batch (Batch 4) + interpolation substrate close.** Five more Tier-1 components migrate to `.prism-ui` source — `shell.icon-button`, `shell.dock-tab`, `shell.workflow-page-button`, `shell.status-bar`, `shell.menu-bar-row` — plus the new shared `shell.tab-button` DSL primitive that `shell.dock-tab` + `shell.workflow-page-button` both compose against. Total Tier-1 count is now 29 (Batch 1: 6 + Batch 2: 12 + Batch 3: 6 + Batch 4: 5). The fourth Wave 11.2 substrate (shared-chrome lift) lands as a *DSL composition pattern* — `shell.tab-button` is a `.prism-ui` row that takes `height`, `padding-x`, `padding-top`, `active-bg`, `hover-bg`, `underline-active`, `data-role`, `target-id`, `label`, `active` as schema props, and `shell.dock-tab` / `shell.workflow-page-button` are one-line wrappers that forward their specific metrics through. The smart-pattern lesson: when two Rust blocks share a static visual recipe parameterised by a struct (`chrome::TabStyle` → `active_underline_tab`), the right lift is a DSL primitive parameterised by props, not a Rust helper called from migrating-but-still-Rust blocks. **Dead-code cleanup**: `chrome::TabStyle` + `chrome::active_underline_tab` deleted (no consumers remain), `chrome.rs` shrinks by ~75 LoC. **Sibling substrate fix in `prism-ui-runtime::interpret::collect_text_content`**: text-element interpolations now fall through to the full expression evaluator when the cheap bare-path `lookup_expression` fails — so `<text>{text ? text : status}</text>` works against the same vocabulary attribute interpolations use. Empty interpolation resolutions no longer push a leading space separator (the `"Saved. "` regression from the status-bar migration was the canonical pin). Net: ~1180 LoC of Rust deleted (icon_button + dock_tab + workflow_page_button + status_bar + menu_bar_row + the chrome.rs sweep), ~165 LoC of `.prism-ui` added across the six files (including `tab-button` as a shared primitive). **Workspace state**: 388 prism-shell tests + full workspace `cargo test --workspace --lib` (12 suites) green; `cargo clippy --workspace --all-targets -- -D warnings` clean. **Remaining Tier-1**: `shell.app-card`, `shell.app-window`, `shell.inspector-row`, `shell.builder-toolbar`, plus a handful of larger panels. Each is now blocked only on the size of the per-PR migration — the DSL / resolver substrate is in place. `chrome::icon_button_node` still has Rust callers (`builder_toolbar`, `inspector_row`); when those migrate the last 100 LoC of chrome.rs can move into the DSL. Tier-2 (~14 stateful / gesture components) remains intentionally deferred per §11.5 — each primitive's first authored consumer fills out its full body. | The user's "finish implementing the waves fully — smart patterns, cleanup old dead code" target focused on closing the fourth Wave 11.2 substrate gap. The chrome-lift wasn't a new DSL feature — it was *recognising that the parameterised-visual-recipe pattern in `chrome::active_underline_tab` is exactly what a DSL composition primitive is*. Lifting the recipe into `shell.tab-button` and authoring the two consumers as one-line wrappers proves the pattern; the dead-helper sweep proves the lift was complete. The `collect_text_content` fix was forced by the migration — the cheap-lookup-only path in text content diverged from the attribute path (which already does the full eval fall-through). Unifying the two means an author's mental model is "interpolations are interpolations" regardless of where they sit. This is the kind of fix that's invisible when migrating one component at a time but becomes essential once the substrate is the load-bearing thing: every future text-content ternary works without a new substrate add. |
