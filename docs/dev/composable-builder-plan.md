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
- [ ] **1.8** Luau bindings: `LuauModifier` exposing `schema`,
  `wrap`, `install_effects` from Luau; `modifier:props():read("k")`
  / `:write("k", v)` against the reactive bag. **Deferred to Wave 8.**
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
- [~] **2.4** `color`: clicking opens a `field_focus` text-edit
  session — paste / type hex strings commits to the bound prop.
  *Full HSL `<color-picker>` overlay is Wave 10.*
- [~] **2.5** `file`: clicking opens a `field_focus` text-edit
  session — paste / type a path commits. *`rfd` native dialog
  through `<file-button>` is Wave 10 (needs a desktop-only seam
  in `Vfs` plus feature gating for the wasm target).*
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

### Wave 4 — Connections panel UX
- [ ] **4.1** `BuilderSlot::selected_connection: Option<ConnectionId>`.
- [ ] **4.2** Row click → set cursor; trash → real
  `BuilderService::delete-connection`.
- [ ] **4.3** "+" affordance opens `shell.connection-picker`
  (source × target × action kind, with `ActionKind::Bind` in list).
- [ ] **4.4** Mutators: `add_connection`, `delete_connection`,
  `update_connection_field`; each runs the C-wave resync.

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

### Wave 6 — B6 third-wave remainder
- [ ] **6.1** `NavigationSlot::selected_page` + nav-page chevron
  wiring through `BuilderService::move-page-{up,down}`.
- [ ] **6.2** `SchemaSlot::selected_field` + schema-row trash wiring
  through `BuilderService::delete-schema-field`.
- [ ] **6.3** (Signal-connection trash covered by Wave 4.)

### Wave 7 — Headless visual capture
- [ ] **7.1** `prism-shell --scene <name>` boots and exits after one frame.
- [ ] **7.2** `--screenshot <path>` writes PNG via femtovg offscreen
  surface.
- [ ] **7.3** `prism visual` removes screencapture shim and shells
  through `prism-shell` directly.

### Wave 8 — Luau parity for components + modifiers
- [ ] **8.1** `LuauComponent` exposes per-NodeId
  `ReactiveProps` to Luau: `node:props():read("k")` /
  `:write("k", v)` matches the Rust seam exactly.
- [ ] **8.2** `LuauModifier` ships with the same surface (`schema`,
  `wrap`, `install_effects`, `props`).
- [ ] **8.3** `prism-builder::generate_signal_type_stubs` grows a
  modifier branch — each registered modifier emits a Luau type
  block alongside component stubs in `signals.d.luau` /
  `reactive.d.luau`.
- [ ] **8.4** `register_block_from_luau` and
  `register_modifier_from_luau` are first-class entry points usable
  from a `script.luau` at boot — authoring without recompile.

### Wave 9 — DSL gap-close (prereq for Wave 11)
- [ ] **9.1** `.prism-ui` parser supports `route:` attribute
  namespace (`route:role="..."` → `data-role`).
- [ ] **9.2** `:state` selectors in inline style attrs (`:hovered`,
  `:selected`, `:focused`).
- [ ] **9.3** `bind:` attribute namespace as compile-time sugar for
  `ActionKind::Bind`.
- [ ] **9.4** Animation transitions: `transition:opacity="200ms"`
  on container attrs maps to a per-prop `Effect`-driven animator.

### Wave 10 — Primitive registry
- [ ] **10.1** `PrimitiveRegistry` table in `prism-ui-runtime`
  mirrors `BUILTINS` shape — one `BlockSpec` per primitive.
- [ ] **10.2** 14 primitives landed (§10.3): `<text-input>`,
  `<drag-scrub>`, `<popover>`, `<list-picker>`, `<collapsible>`,
  `<split-handle>`, `<timed-overlay>`, `<focus-trap>`, `<select>`,
  `<color-picker>`, `<file-button>`, `<resize-edge>`,
  `<canvas-paint>`, `<text-buffer>`.
- [ ] **10.3** Each primitive exposes its prop schema through the
  same `Component::schema()` surface, so it slots into the
  Inspector unmodified.

### Wave 11 — `.prism-ui` self-hosting (long tail)
- [ ] **11.1** Three generalization sweeps land (hover modifier,
  `route:` attrs, `<collapsible>` primitive) — deletes duplication
  across 7+ components before any migration.
- [ ] **11.2** Tier-1 migration: ~30 pure-visual components → one
  PR each, ~30 lines `.prism-ui` added / ~100 lines Rust deleted.
- [ ] **11.3** Tier-2 migration: ~14 stateful / gesture components
  using the new primitives.
- [ ] **11.4** Tier-3 primitives shipped as runtime entries
  (`<canvas-paint>`, `<text-buffer>`, `<builder-host>`); the
  remaining shell wrappers around each shrink to thin `.prism-ui`
  shells.
- [ ] **11.5** Every shell `.prism-ui` document is hot-reloadable
  via subsecond (Phase 9 anchor at `lower_template` already in
  place).

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
| 2026-05-12 | **Wave 3 lands** — the canvas becomes pointer-driven. (3.1) The §43 B5 `route_canvas_node_select` already wired `Surface::hit_test_at` → `AppState::select_node` before drag-capture; reconfirmed and extended with bbox capture for the gizmo overlay. (3.2) `CatalogSlot::palette_drag` + three `AppState` mutators (`begin_palette_drag` / `update_palette_drag` / `end_palette_drag` + `cancel_palette_drag`) + `CanvasSlot::insert_under_node` make palette pick → canvas click → release insert a fresh node under the hit's `data-canvas-node` (or under the document root). New node moves the selection so the inspector/properties refresh; the palette pill clears so the drop is one-shot. (3.3) `CanvasSlot::selection_bbox` carries the click rect into `builder_canvas_props` as `selection-rect`, so the existing `build_selection_layer` paints the outline + 8-handle ring around the live bbox. `POINTER_ROUTES("resize-handle")` captures direction + transform snapshot; `update_resize_drag` translates the node per `(px, py) * (dx, dy)`. (3.4) `PointerButton::Secondary` on canvas-resident hits opens a context menu populated from `canvas_context_menu_items` (Move Up / Down / Duplicate / Copy / Cut / Delete on a selection; Paste on empty canvas); each row's `data-on-click="cmd <id>"` reuses the existing command-table dispatch. **447 prism-shell tests, full workspace clippy clean.** Visible palette-drag ghost overlay and per-node `width`/`height` mutation are deferred — both depend on layout API extensions that belong to later waves. | The plan's §5 (Wave 3) called for "the canvas becomes alive" — selection, drop, gizmo, context. The bones already existed (`Surface::hit_test_at`, `POINTER_ROUTES`, `state.canvas.pointer_down`/`pointer_move`/`pointer_up`); this wave activates them through one routing layer with no new event-loop concept. The `is_canvas_hit` helper is the single source of truth for "is this a canvas surface" — palette drag and right-click both gate on it, so future canvas-resident gestures plug in by adding to that set. The Wave 3 sequencing (`3.1 ✅ → 3.2 + 3.4 + 3.3`) followed §12's dependency graph — the structural seams from Wave 5 + the field-edit polish from Wave 2 were both already live. |
| 2026-05-12 | **Dedup pass over Wave 1.** Three independent patterns collapsed to one source each: (1) the twelve hand-rolled `ModifierBehaviour` impls (six baseline + six bootstrap) collapsed to a `BehaviourSpec` data struct + `SpecBehaviour` blanket impl, mirroring `BlockSpec`/`SpecBlock` from `prism_builder::block`. Each behaviour becomes one `const X: BehaviourSpec = BehaviourSpec::new(...).description(...).wrap(...)` row plus a row in `BUILTINS` / `BOOTSTRAP`; `register_specs(reg, &[&BehaviourSpec])` fans them in. (2) The three near-identical 20×20 routed-icon constructions in `shell.modifier-header` (toggle / remove / drag) collapsed to a `chrome::route_chip(id, icon, role, aria_label, extra_attrs)` helper. (3) The two near-identical `handle_modifier_{toggle,remove}_click` functions collapsed to one `indexed_modifier_route!($handler, $mutator)` `macro_rules!` invocation pair. Also pulled `ModifierRegistry::{list, descriptor}`'s duplicated descriptor projection into one `descriptor_from(&dyn ModifierBehaviour)` helper. Net: ~190 LoC deleted from `modifier.rs`, ~110 LoC deleted from `modifier_bootstrap.rs`, ~60 LoC deleted from `modifier_header.rs`. All 397 prism-builder + 421 prism-shell tests still pass; clippy clean. | The `BlockSpec` / `SpecBlock` pattern §32-§33 of the clay-migration plan established for document components is the right shape for *any* registered behaviour family with a uniform `(id, label, schema, optional wrap)` surface. Lifting it to modifiers means the same authoring grammar covers components, blocks, behaviours, and (in Wave 8) Luau-registered entries — one `const SPEC = …` row instead of N hand-rolled impls. The `route_chip` helper sets the pattern for every future per-row affordance (signal-connection trash, nav-page chevron, schema-row delete) so the next gizmo lands as a one-line call. The `indexed_modifier_route!` macro is small but proves the pattern for the next wave of routes (add-page chevrons, schema-row buttons) that follow the same "parse data-target-id + data-idx, call mutator" shape — adding one is one macro line. |
