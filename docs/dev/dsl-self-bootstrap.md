# DSL Self-Bootstrap Plan

Make Prism apps declare themselves through the framework — manifest +
DSL + Luau — instead of being baked into the shell's Rust source. The
shell becomes a generic host that resolves an app, mounts it, and runs.

## Today's state (2026-05-13)

Architecture is **well-positioned** for this — internal seams are
clean, registries are unified, render pipelines are unified. The gap
is that the **app/framework boundary** is drawn in Rust source rather
than in DSL+config. Four things are hardcoded that want to be
declarative:

| Today (Rust source) | Target (declarative) |
|---|---|
| `prism-shell/src/seed.rs:104` — four hardcoded `AppCard` rows | `apps/<id>/manifest.toml`, discovered at boot |
| `prism-dock/src/panel.rs:185` — `PanelKind::ALL` const array (15 panels) | `DockCatalog` runtime registry seeded by `register_builtins` |
| `prism-shell/src/services/mod.rs:302` — `register_shell_services` adds 14 services unconditionally | App manifest declares required services; some always-on, some per-app |
| `prism-core/src/luau_bindings.rs` — exposes types as `UserData` but no registration verbs | `prism.register_component`, `prism.register_panel`, `prism.register_service` |

## Shared pattern: registry + builtins + extension

Every one of the four loops resolves to the **same DI shape**, already
proven by `prism_builder::ComponentRegistry`:

```rust
pub struct Registry { entries: IndexMap<String, Entry> }

impl Registry {
    pub fn new() -> Self { ... }
    pub fn register(&mut self, entry: Entry) -> &mut Self { ... }
    pub fn get(&self, id: &str) -> Option<&Entry> { ... }
}

pub fn register_builtins(reg: &mut Registry) {
    reg.register(Entry::BUILDER);
    reg.register(Entry::INSPECTOR);
    // ...
}
```

Apps push their own entries through the *same* `register` method.
Built-ins go first, app extensions go after — the registry doesn't
distinguish them. This is the pattern we're already using for blocks
(`SHELL_BUILTINS` + `register_specs`); we generalize it to panels,
services, and the manifest itself.

## Loop 1 — App Manifest format + loader

New: `prism-core/src/app.rs` (the manifest IR + parser) and
`prism-shell/src/app_loader.rs` (the resolver that scans `apps/` and
hydrates a `LoadedApp`).

Manifest (TOML, reusing the parser PRSS already pulls in):

```toml
# apps/lattice/manifest.toml
id = "lattice"
label = "Lattice"
icon = "icons/grid.svg"
summary = "Collaborative workspace with real-time CRDT sync."

# Optional. If absent, app inherits the host's default skeleton +
# every built-in service/panel and just shows up as a launchpad tile.
[entry]
skeleton = "shell.prism-ui"      # relative path
styles = "app.prss"              # relative path
script = "main.luau"             # relative path

[services]
required = ["selection", "builder"]    # subset of global services
optional = ["luau"]

[panels]
include = ["builder", "inspector", "properties"]
add = []                                # additional PanelKind specs
```

Boot flow:
1. `Shell::new()` scans `apps/*/manifest.toml` via `AppLoader::discover()`.
2. Each manifest hydrates into `LoadedApp { manifest, skeleton, styles, script }`.
3. Built-in apps fall through a fallback path so the launchpad always
   has tiles — no manifest = no app, but core ships at least
   `apps/studio/manifest.toml` so Studio is data, not hardcode.
4. `AppCard` rows in `seed.rs` get derived from `AppLoader` output
   instead of being literals.

Compatibility: if `apps/` doesn't exist or is empty, the old
hardcoded list is restored as a fallback so this lands without
breaking any test.

## Loop 2 — Dock catalog as runtime registry

Refactor `prism-dock/src/panel.rs`:

- Keep `PanelKind` as the data shape (no change to its fields).
- Drop `PanelKind::ALL` (the `&'static [&'static PanelKind]` const).
- Add `DockCatalog { entries: IndexMap<&'static str, PanelKind> }`.
- Add `dock::register_builtins(catalog: &mut DockCatalog)` that
  pushes the 15 current panels.
- `PanelKind::from_id` / `PanelKind::tag_for` become methods on
  `DockCatalog` (`catalog.get`, `catalog.tag_for`). Free-function
  shims preserve the old call-sites during migration; once shell is
  switched over to the catalog they retire.
- Shell owns one `DockCatalog` instance, built once at boot. Apps
  push their own panels into it during their load step.

Migration strategy: introduce `DockCatalog` alongside `PanelKind::ALL`,
migrate consumers one at a time, then delete `ALL`. Tests don't change
shape — they assert against `register_builtins(&mut catalog)` output.

## Loop 3 — Per-app service activation

Today: `register_shell_services` registers 14 services
unconditionally. Target: services declare an **activation scope**, and
the manifest's `services.required` / `services.optional` filter
non-universal ones.

Two refinements to `ServiceRegistry`:

1. Each registration carries a `ServiceScope`:
   - `Universal` — always present (UndoRedo, FieldFocus, CommandPalette,
     Input — all currently mandatory because shell chrome routes
     through them).
   - `App` — gated on manifest declaration (BuilderService,
     SignalsService, LuauService).
2. `register_shell_services` becomes a builtin-seeding step;
   per-app filtering runs after.

This is the smallest change: keep universal services exactly as
they are, just *label* the optional ones, and add a filter pass
between registration and `mount`.

## Loop 4 — Luau registration surface

Today: Luau scripts return `VirtualNode` trees through
`LuauComponent`. They consume Prism but can't *extend* it. Target:
an app's `main.luau` can call:

```lua
prism.register_component {
  id = "my.card",
  render = function(props, children) ... end,
}

prism.register_panel {
  id = "my.timeline",
  label = "Timeline",
  tag = "my.timeline-canvas",
  min_width = 240,
}

prism.register_service {
  id = "my.metronome",
  on_event = function(ctx, event) ... end,
}
```

Implementation:
- Extend `prism-core/src/luau_bindings.rs` with three host functions
  exposed under the `prism` global.
- Each function takes a Lua table, validates it against the registry's
  Rust schema, and pushes through `Mutex<Arc<dyn …Registry>>` handles
  installed by the shell at script-load time.
- The bridge runs **during app load**, not per-frame — Luau-defined
  entries land in the same registries before the shell starts
  rendering, so by the time `lower_ui` runs there's no distinction
  between Rust- and Luau-registered entries.

## Implementation order

| # | Loop | Order rationale |
|---|---|---|
| 1 | Manifest format + loader | Foundation; nothing else makes sense without a place to declare app metadata |
| 2 | DockCatalog runtime | Most isolated; doesn't depend on manifest, no caller cares as long as `tag_for` keeps working |
| 3 | Per-app service activation | Builds on manifest (reads `services.required`) |
| 4 | Luau registration surface | Builds on every other registry being mutable |

Each step lands with:
- New + updated unit tests in the affected crate.
- `cargo check --workspace` + `cargo test --workspace` clean.
- A short note appended to this doc's "Decision log" at the bottom.

## Out of scope (intentional)

- **Per-app skeletons.** The doc framing mentioned splitting
  `app.prism-ui` into per-app `<app>/shell.prism-ui` files. That's a
  natural follow-on, but it's structural — it changes how the shell
  parses + which `app-name` it shows in chrome — and is decoupled from
  the four loops here. Defer to a separate doc when we tackle it.
- **Hot-swappable apps.** Today the shell loads apps at boot; a
  running app can't be replaced. The manifest format is forward-
  compatible with hot-swap but we don't implement the swap path yet.
- **Service factories.** Services are currently registered by direct
  `reg.add(FooService)` calls. A factory-based scheme (`Box<dyn Fn() -> Box<dyn ShellService>>`)
  would let Luau register services too, but Loop 4 wires only the
  *declaration* path through Luau — the actual service body still
  has to be Rust until we land an mlua-backed service shim.

## Decision log

- 2026-05-13: Doc drafted; implementation kicks off with Loop 1.
- 2026-05-13: **Follow-on wave landed.** Closes the four "remaining
  work" bullets from the prior wave:

  1. **Catalog plumbed through to live rendering.** `ShellInner`
     now owns `app_registrar: ShellAppRegistrar` plus a frozen
     `dock_catalog: Arc<DockCatalog>` snapshot. `PropCtx` gained
     `dock_catalog: Option<&'a DockCatalog>`. The `shell.dock-workspace`
     binding moved from the `SLOT_BINDINGS` table to a closure-style
     registration; it now emits `dock` + `labels` (panel-id →
     friendly label) + `tags` (panel-id → content tag) sidecar
     maps. `dock_workspace_lower` reads from those sidecars instead
     of consulting a catalog at lower time, and passes
     `content-tag` through to `dock_panel_lower`, so
     app-registered panels surface in the live dock end-to-end.

  2. **`register_component` implemented** via a queue +
     `LuauComponentBlock` shim. The block impls
     `prism_builder::Block` so the existing blanket `Component`
     impl picks it up; today's `lower_ui` produces a labelled
     placeholder (`data-role="luau-component"`,
     `data-component=...`, `data-luau-key=...`) — the contract
     every Luau-defined component renders against until the
     in-process runtime supplies a real `script.render` body.

  3. **`register_service` implemented** via a queue +
     `LuauScriptedService` shim. `install_services` drains the
     queue into the live `ServiceRegistry` with
     `ServiceScope::App` so manifest-declared filtering applies
     uniformly. Today's `on_event` returns `EventOutcome::Pass`;
     the dispatch grows when Luau lands.

  4. **End-to-end integration tests.**
     `packages/prism-shell/tests/dsl_self_bootstrap.rs` (13 tests)
     drives the full chain from `apps/<id>/manifest.toml` on disk
     through `PRISM_APPS_DIR` → `app_loader::discover` →
     `ShellAppRegistrar` → `Shell::new`. Coverage:
     - launchpad tile resolution from manifests (override + card
       metadata),
     - manifest `panels.add` rows reaching `ShellInner.dock_catalog`,
     - service filtering respecting `services.{required, optional}`
       declarations,
     - permissive default keeping every service when no manifest
       opts in,
     - `AppRegistrar` trait used polymorphically against
       `ShellAppRegistrar` and `NoopAppRegistrar`,
     - queue-based component / service registration end-to-end,
     - `LuauComponentBlock` rendering the expected placeholder,
     - fallback launchpad when no `apps/` dir exists.

  **Test deltas:** prism-core 2129 (unchanged), prism-dock 86
  (unchanged), prism-shell 338 → 347 lib + 13 new integration tests
  (`tests/dsl_self_bootstrap.rs`). Workspace total: **3603 tests
  passing**, zero failures, `cargo clippy --workspace --all-targets
  -D warnings` clean.

  **Residual follow-up:** binding `prism.register_panel` /
  `prism.register_component` / `prism.register_service` as Luau
  `UserData` methods on a `Arc<dyn AppRegistrar>` handle, threaded
  into the in-process mlua state owned by `prism-daemon`. The
  registrar trait surface is finalised; what remains is the
  `mlua`-side glue and the in-process script lifecycle in
  `prism-daemon::modules::luau_module`.
- 2026-05-13: **All four loops landed.** Final shape:
  - **Loop 1** — `prism_core::AppManifest` (TOML, 5 tests),
    `prism_shell::app_loader::discover` (4 tests),
    `seed::initial_state_with_apps` falls through to hardcoded list
    when no `apps/` dir is present (2 tests). Four manifest stubs
    seeded under `apps/{lattice,flux,musica,studio}/manifest.toml`.
  - **Loop 2** — `prism_dock::DockCatalog` runtime registry (5 tests).
    `PanelKind::ALL` / `from_id` / `tag_for` static API deleted; the
    two prod call sites (`dock_panel.rs`, `dock_workspace.rs`) now go
    through `DockCatalog::with_builtins()`. Catalog-on-`LowerCtx`
    plumbing deferred — built-ins-only lookup is sufficient until
    apps actually push panels (Loop 4 follow-up).
  - **Loop 3** — `ServiceScope::{Universal, App}` tagging on every
    builtin (`add_scoped`), `ServiceRegistry::activate_app_services`
    filter (3 tests). Wired into `Shell::new` with a permissive
    default — manifests that omit `[services]` keep every
    `App`-scoped service intact. Four services tagged `App`:
    `builder`, `signals`, `luau`, `project`.
  - **Loop 4** — `prism_core::AppRegistrar` trait (4 tests) +
    concrete `prism_shell::app_registry::ShellAppRegistrar` (6 tests)
    wrapping `Arc<Mutex<DockCatalog>>`. `install_panels_from_manifests`
    walks every `panels.add` row through the registrar — apps can now
    push panel kinds end-to-end. Component + service registration
    paths are present in the trait but return
    `RegistrationError::Unsupported` until follow-up:
    1. **`register_component`** needs a Luau-backed `Block` impl that
       calls a script's render fn during `lower_ui`, plus mutable
       shared access to `prism_builder::ComponentRegistry` (today
       owned by `ShellInner`, not wrapped in a `Mutex`).
    2. **`register_service`** needs a Luau-backed `ShellService` shim
       that routes `on_event` calls through a script handler.
    3. **Lua bindings** (`prism.register_panel = function(t) ... end`)
       — the host-side `mlua` state lifecycle lives in
       `prism-daemon::modules::luau_module`; adding a `UserData` wrapper
       around `Arc<dyn AppRegistrar>` and binding `prism.register_panel`
       is the natural next step.
    4. **Catalog wiring through `LowerCtx`** — `dock_panel_lower` /
       `dock_workspace_lower` currently construct a transient
       `DockCatalog::with_builtins()`. Threading the shell-owned
       `Arc<Mutex<DockCatalog>>` through `PropCtx` → `LowerCtx`
       (via an opaque extension slot, since `prism-builder` doesn't
       depend on `prism-dock`) makes app-pushed panels visible to the
       live dock.

  **Test deltas across the wave:** prism-core +2041 → 2129 (+88, of
  which 4 are app-registry / 5 are manifest), prism-dock 86 (catalog
  +5, panel test refactor -2), prism-shell 313 → 338 (+25 across
  app_loader, seed, services, app_registry).

