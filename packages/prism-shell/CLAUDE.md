# prism-shell

Single source of truth for the Prism UI tree. Renders through
`prism-ui-runtime` (winit + femtovg native, wasm32 + WebGL browser).
Slint and the entire Slint stack were exorcised in the 2026-05-10
Phase 5 cutover (see `docs/dev/clay-migration-plan.md` §17 + §24-§28).
The shell is now a thin host for three contracts: a parsed
`ui/app.prui` skeleton, a property-bindings table, and a service
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
  `.prui` skeleton edits are picked up on the next respawn.

## Crate layout
- `crate-type = ["cdylib", "rlib"]` — `rlib` for native consumers
  (Studio host, tests), `cdylib` for `wasm-bindgen` to wrap.
- `src/bin/native.rs` — minimal entry point: `Shell::new()?.run()`.
- `ui/app.prui` — the canonical skeleton parsed at boot via
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
  import_resolver.rs   # `FsImportResolver` — host `ImportResolver` (sibling/`prism://`)
  lib.rs               # public surface + `web_start` wasm entry
  project_manager.rs   # Project Vault: folder → persistent object graph (native)
  props.rs             # `ShellPropBindings` (read side — slot → JSON props)
  render.rs            # `render_tree` + `Skeleton` loader
  render_scope.rs      # `RenderScope` — Phase 3 reactive Owner + DirtyQueue
  shell.rs             # `Shell` + `ShellInner` borrow-pack
  state/               # `AppState` + every typed slot (decomposed)
    mod.rs             #   `AppState` struct + `impl AppState` + cursor helpers
    inspector.rs       #   inspector-tree + property-row derivation
    slots_core.rs      #   Project / DevTools / Search / Chrome / Workspace
    overlay.rs         #   `OverlaySlot` + modal/picker/toast sub-types
    slots_doc.rs       #   Builder / Navigation / Catalog / Docs / Menu
    canvas/mod.rs      #   `CanvasSlot` struct + `impl CanvasSlot`
    canvas/parts.rs    #   canvas standalone types + tree helpers
    tests.rs           #   the `#[cfg(test)]` suite
```

The `state/` decomposition (Phase B.4 of the WYSIWYG roadmap) split
the former 7.5k-line `state.rs` along slot/divider seams. Every
production module is now ≤ ~1.2k lines. The split is purely module
boundaries: child modules `use super::*` to inherit intra-`state`
types + crate imports, and items the cross-module callers reach were
widened from private to `pub(crate)` (behaviour-preserving — all
crate-internal). `pub use canvas::*` / `slots_*::*` keep every
`crate::state::X` / `crate::X` path stable; `tests.rs` is the only
module still over the size guideline and is test-only.

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
- `hot-reload` — **Phase 9 of `docs/dev/dioxus-inspiration.md`.**
  Pulls in `subsecond` and wraps the per-frame `render_tree` walk
  inside `subsecond::call`. A swapped-in `lower_ui` body patches
  in-place (preserving the `Surface` tree and the reactive `Owner`
  graph) instead of forcing a kill-and-respawn dev loop. The
  patch-pipeline integration (cargo invocation that emits the
  runtime patch + `subsecond::register_handler` hookup) is a
  prism-cli follow-up; today turning the feature on installs the
  anchor so the rest can land incrementally. Off by default
  because the anchor adds a small per-call indirection.
  `prism dev shell --hot=subsecond` enables it automatically.

## Public surface
From `src/lib.rs`:
- `Shell` / `ShellError` — owning `Rc<RefCell<ShellInner>>` plus the
  parsed `Skeleton`. `Shell::new()` builds the registry, bindings,
  services, parses the skeleton, and constructs the inner state.
  `Shell::run()` hands control to the femtovg backend (or returns
  immediately on web until the web backend's `run` lands).
- `ShellInner` — per-frame shared state: `registry`, `resolver`,
  `bindings`, `services`, `state` (the `AppState`), `viewport`,
  `undo`, `vfs`, `luau`, `clipboard`, `render_scope`. Two methods
  construct the per-frame borrow-packs: `prop_ctx()` for reads,
  `mut_ctx()` for writes. Adding a new datum is one field here and
  one assignment in each ctx builder.
- `RenderScope` — Phase 3 of
  `docs/dev/dioxus-inspiration.md`. Owns a `reactive::Owner` plus a
  `DirtyQueue<NodeId>` (node IDs are `String`s) plus a per-shell
  `prism_builder::ui_lower::BlockInvalidator`. Three reactive
  entrypoints, all sharing the same dirty queue:
  1. **Phase 3a — frame-level context.**
     `render_scope.run_in_render_pass(body)` wraps the per-frame
     render walk in a persistent `ReactiveContext`. Any
     `Signal::read` inside the walk auto-subscribes the frame
     context; signal writes mark `FRAME_DIRTY_SENTINEL` into the
     queue, surfacing as `needs_redraw() = true`. Used by
     `Shell::render` + the femtovg event handler so signal-driven
     redraws happen without any per-binding wiring.
  2. **Phase 3b — per-block contexts.**
     `render_scope.block_invalidator()` is a `BlockInvalidator`
     wired to the dirty queue. The canvas binding plumbs it into
     `state.canvas.lower_document_to_ui_with_invalidator(reg, Some(inv))`,
     and the builder's `LowerCtx::with_block_invalidator(inv)` then
     runs every recursive `lower(node)` inside the node's per-NodeId
     reactive context. Signal reads inside a block's `lower_ui`
     body subscribe to that context; writes mark the block's NodeId
     into the dirty queue.
  3. **Imperative.** `render_scope.invalidate_on(node_id, || sig.read(..))`
     for explicit "mark this node dirty when this signal changes"
     wiring without authoring a new dispatch arm.

  The femtovg event handler reads `render_scope.needs_redraw()`
  after every dispatch and merges it with the existing
  `dispatch_event` bool to decide whether to re-render. Selective
  per-subtree re-lowering — the "only walk dirty subtrees, reuse
  cached `UiNode`s for the rest" half — is a follow-up; today the
  queue's non-emptiness drives a full `render_tree` re-walk, but
  every block already runs inside its own reactive scope so the
  subtree-cache layer is a pure addition when it lands.
- `AppState` + slot types — `BuilderSlot`, `CanvasSlot`,
  `ChromeSlot`, `OverlaySlot`, `NavigationSlot`, `WorkspaceSlot`,
  `ProjectSlot`, `SearchSlot`, `IndexSlot` (IDE Phase 2 — the
  project-wide `prism_core` `SymbolIndex` + the Ctrl+T "Go to Symbol"
  palette state; rebuilt per-file on `editor.file.save` and wholesale
  on `project.open-folder` / `Shell::{open,poll}_project` via
  `AppState::reindex_luau_symbols` — which also drives `DiagnosticsSlot`),
  `DiagnosticsSlot` (IDE Phase 3 — `LuauSyntaxProvider::diagnose` per
  `.luau` file, surfaced through `shell.diagnostics-panel` /
  `PanelKind::DIAGNOSTICS`), plus the leaf records (`Toast`,
  `NavPage`, `InspectorNode`, `PropertyRow`, `SchemaDoc`,
  `SignalConnection`, `TransformSnapshot`, …). Each slot owns its
  own `*_props()` methods so binding closures stay one line.
- `ProjectManager` (native only) — Project Vault per
  `docs/dev/project-vault.md`. `Shell::open_project(path)` reads/creates
  `.prism.json`, hydrates a persistent `CollectionStore` via
  `VaultManager<FileSystemAdapter>`, ingests every file as a
  `GraphObject` (`type "file"`/`"folder"`, deterministic
  `sha256` id, content hashed into a `FileSystemVfsAdapter` blob
  store, image thumbnails), populates `state.catalog.files`, and
  starts a recursive `notify` watcher. `close_project` /
  `save_project` / `poll_project` round it out;
  `Shell::run_with_project` / the `--project <path>` CLI flag drive
  the watcher each idle tick. Held as `Option<ProjectManager>` on
  `ShellInner` — `None` for the default ephemeral session.
- **Presence (IDE Phase D)** — `ShellInner.presence` is a
  `prism_core::network::presence::PresenceManager`; a subscribed
  listener funnels every `PresenceChange` into a host queue that the
  `run_with_project` idle tick (and the standalone `Shell::poll_presence`)
  drains into `state.devtools.presence` via
  `DevToolsSlot::apply_presence_change` (TTL `sweep` first).
  `Shell::presence_receive_remote(PresenceState)` is the
  host/transport seam — the wire (relay/WebRTC) owns the call site.

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
exposes a `TagResolver` so `<shell.*>` tags in `app.prui` route
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

### Shared text-input seam — `services/text_input.rs`
Four services drive a `prism_ui_runtime::editor::TextEditor` —
`FieldFocusService` (property rows), `CodeEditorService` (the
`shell.code-editor` panel), `CommandPaletteService`, and `SearchService`.
All four route through one helper, [`dispatch_text_input`], with the
same engine — caret, selection, Ctrl+A, Ctrl+C/X/V, IME preedit /
commit, arrow nav — and a small declarative bindings struct
(`passthrough_modifier_keys`, `passthrough_plain_keys`) to express the
service-specific opt-outs (CodeEditor lets Ctrl+S/N/O/W/Tab fall
through to EditorFiles; FieldFocus and the modals reserve Enter /
Escape for commit / cancel; CodeEditor lets Enter insert a newline).
Adding a new text-input surface is one wrapper around `dispatch_text_input`
plus the surface's own post-mutation work — flush-to-prop, refilter,
mark-dirty — driven by the returned [`TextInputOutcome`]
(`BufferMutated` / `DisplayMutated` / `Inert` / `PassToGlobal` /
`Ignored`).

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
