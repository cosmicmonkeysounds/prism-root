# prism-shell

Single source of truth for the Prism UI tree. Renders through
`prism-ui-runtime` (winit + femtovg native, wasm32 + WebGL browser).
Slint and the entire Slint stack were exorcised in the 2026-05-10
Phase 5 cutover (see `docs/dev/clay-migration-plan.md` §17 + §24-§28).
The shell is now a thin host for three contracts: a parsed
`ui/app.prism-ui` skeleton, a property-bindings table, and a service
registry.

## Build & Test
- `cargo build -p prism-shell` — default feature `native`. Pulls in
  `prism-ui-runtime/femtovg` for the windowing + rendering stack.
- `cargo build -p prism-shell --target wasm32-unknown-unknown
  --no-default-features --features web` — browser build. Then
  `wasm-bindgen --target web --out-dir web/
  target/wasm32-unknown-unknown/<profile>/prism_shell.wasm` emits the
  JS loader pair next to `web/index.html`. `prism build --target web`
  wires this up as one command.
- `cargo test -p prism-shell` — 255+ lib tests covering bindings,
  resolver, every shell block, every service, and end-to-end
  skeleton renders.
- `prism dev shell` — runs the native binary at `src/bin/native.rs`
  inside `dev_loop::DevLoop`. `.rs` changes kill+respawn the child;
  `.prism-ui` skeleton edits are picked up on the next respawn.

## Crate layout
- `crate-type = ["cdylib", "rlib"]` — `rlib` for native consumers
  (Studio host, tests), `cdylib` for `wasm-bindgen` to wrap.
- `src/bin/native.rs` — minimal entry point: `Shell::new()?.run()`.
- `ui/app.prism-ui` — the canonical skeleton parsed at boot via
  `prism_ui_runtime::interpret`. Per §28 the root is
  `<shell.app-window><shell.dock-workspace/></shell.app-window>` plus
  overlay siblings; the active dock tree drives panel routing
  through `prism_dock::PanelKind::tag` (§34 — the shell-side routing
  table was folded into the dock catalog).

## Source tree
Nine entries — the entire feature surface:

```
src/
  bin/native.rs        # `Shell::new()?.run()`
  components/          # 48 shell blocks (chrome, panels, overlays, gizmos)
  services/            # 10 services (write side — see below)
  events.rs            # `dispatch_event` router (read events → service fan-out)
  lib.rs               # public surface + `web_start` wasm entry
  props.rs             # `ShellPropBindings` (read side — slot → JSON props)
  render.rs            # `render_tree` + `Skeleton` loader
  shell.rs             # `Shell` + `ShellInner` borrow-pack
  state.rs             # `AppState` + every typed slot
```

Adding a feature is one of: a new component module under
`components/` (chrome), a new service under `services/` (behaviour),
or a new slot field on `AppState` (data). Per §17/§24 these are the
only edits available — the legacy escape hatches were deleted in the
Phase 5 cutover.

## Features
- `native` (default) — `prism-core/crdt` + `prism-ui-runtime/femtovg`.
  No `prism-daemon`, no `mlua`, no `rfd`, no `enigo` — host concerns
  stay host-side and re-enter through one resource field on `MutCtx`
  + one constructor in `ShellInner::new`.
- `web` — wasm-bindgen + `prism-ui-runtime/web`. Mutually exclusive
  with `native` at link time. The `#[wasm_bindgen(start)]` entry
  point in `src/lib.rs::web_start` boots the same `Shell` the
  native binary does.

## Public surface
From `src/lib.rs`:
- `Shell` / `ShellError` — owning `Rc<RefCell<ShellInner>>` plus the
  parsed `Skeleton`. `Shell::new()` builds the registry, bindings,
  services, parses the skeleton, and constructs the inner state.
  `Shell::run()` hands control to the femtovg backend (or returns
  immediately on web until the web backend's `run` lands).
- `ShellInner` — per-frame shared state: `registry`, `resolver`,
  `bindings`, `services`, `state` (the `AppState`), `viewport`,
  `undo`, `vfs`, `luau`, `clipboard`. Two methods construct the
  per-frame borrow-packs: `prop_ctx()` for reads, `mut_ctx()` for
  writes. Adding a new datum is one field here and one assignment
  in each ctx builder.
- `AppState` + slot types — `BuilderSlot`, `CanvasSlot`,
  `ChromeSlot`, `OverlaySlot`, `NavigationSlot`, `WorkspaceSlot`,
  `ProjectSlot`, `SearchSlot`, plus the leaf records (`Toast`,
  `NavPage`, `InspectorNode`, `PropertyRow`, `SchemaDoc`,
  `SignalConnection`, `TransformSnapshot`, …). Each slot owns its
  own `*_props()` methods so binding closures stay one line.

## Three contracts

### Read side — `props.rs` + `render.rs`
`ShellPropBindings::with_builtins()` registers one closure per shell
tag that emits a `serde_json::Value` of props from the current
`AppState`. `render_tree(skeleton, bindings, resolver, ctx)` is the
per-frame pipeline: `bindings.snapshot(ctx) → fill_compositions →
lower`. The §22 / §28 discipline: every binding is one row,
returning `slot.foo_props()` — no inline JSON anywhere.

### Components — `components/`
48 shell blocks declared as `pub const FOO_SPEC: BlockSpec` rows
and registered via the `SHELL_BUILTINS: &[&BlockSpec]` table in
`components/registry.rs` (one-line fan-out through
`prism_builder::register_specs`). No per-component struct, no
per-component `impl Block` — each file just exposes a `foo_lower`
free function (and optional `foo_schema` / `foo_signals`) plus the
const spec; the same `prism_builder::SpecBlock` interprets every
spec at runtime. Chrome blocks lower through promote-then-reuse
helpers (`chrome::drag_number_field_node`, `format_drag_value`, …)
so adding a new transform-row variant or field-editor kind is one
literal in the relevant declarative table. The `ShellComponentRegistry`
exposes a `TagResolver` so `<shell.*>` tags in `app.prism-ui` route
to the matching block. See §33 of the migration plan for the
collapse rationale.

### Write side — `services/` registry
Sister to the read side: one declarative table
(`register_shell_services`), one trait (`ShellService`), one
borrow-pack (`MutCtx`). Adding a feature is one
`impl ShellService` plus one row.

Ten services land out of the box:

| id | role |
|---|---|
| `shell.base` | undo/redo/palette commands, focus tick |
| `undo-redo` | snapshot stack on every mutating command |
| `command-palette` | modal capture while open |
| `input` | layered `InputScheme` stack, key combo → command id |
| `selection` | mutators on the slot that owns selection |
| `clipboard` | `serde_json::Value` cell, copy/cut/paste/duplicate |
| `persistence` | `file.{new,save,save-as,open}` against `Vfs` |
| `project` | `project.{open-folder,close-folder}`, file ingest |
| `search` | TF-IDF over node tree, modal capture |
| `help` | hover lifecycle queue with Esc-clears |
| `menu` | dropdown + context menu close |
| `signals` | recursive `fire_signal` cascade dispatcher |
| `luau` | `luau.run-selection` over `MutCtx::luau` |

Cross-service reach is through `MutCtx` resource fields, not
`registry.get(id)` — `SignalsService` calling Luau is `ctx.luau.exec(...)`,
not a registry lookup. The registry's `get` exists for the rare
modal-capture case.

`MutCtx<'a>` carries `state: &mut AppState`, `viewport: Viewport`,
`undo: &mut UndoStack`, `vfs: &mut dyn Vfs`, `luau: &mut dyn LuauHost`,
`clipboard: &mut Clipboard`. Every command body and every
`on_event` impl is `|ctx| { ... }` over this single carrier.

### Event router — `events.rs`
`dispatch_event(&inner, event)` is the single entry the femtovg
backend calls on every winit event. Pointer-arm events (move /
press / release / wheel) hit hit-testing fast paths first; everything
else fans out through `ServiceRegistry::fan_out` in declared order,
short-circuiting on the first `Handled`. Returns `bool` — true when
the next frame needs to redraw.

## Skeleton + dock routing
Per §28 the active dock tree drives content selection at runtime.
`shell.dock-workspace` walks `DockNode` (Split / TabGroup) recursively;
`shell.dock-panel` resolves its body through
`prism_dock::PanelKind::tag_for(panel_id)` when no AST children are
authored. Adding a dockable panel is one row in
`prism_dock::PanelKind::ALL` (its `tag` field carries the shell
content tag); the dock-panel block, the workspace walker, and every
parsed skeleton inherit the new mapping with zero additional edits. The skeleton itself is three lines of meaningful
content plus seven sibling overlay tags.

## Workflow
1. Write tests in `src/**/*.rs` (`#[cfg(test)]`).
2. `cargo test -p prism-shell` and (when touching anything non-trivial)
   `cargo clippy -p prism-shell --all-targets -- -D warnings`.
3. Update the relevant `CLAUDE.md` if the public API changed.
4. Update `docs/dev/clay-migration-plan.md` decision log if a
   structural decision moved.

## Downstream
- `prism-studio/src-tauri` embeds this crate as a library with
  `default-features = false, features = ["native"]`. Studio spawns
  the daemon sidecar, then runs the shell's event loop.
- `prism dev web` drives the wasm-bindgen pipeline; the
  hand-written `web/index.html` imports the generated
  `prism_shell.js` module and calls `init()`, which boots
  `Shell::run()` into `<canvas id="canvas">`.
