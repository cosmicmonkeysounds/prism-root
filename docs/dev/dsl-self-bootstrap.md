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

- ~~**Per-app skeletons.**~~ Promoted to **ADR-009**, landed.
  `apps/<id>/manifest.toml`'s `[entry] skeleton` field points at a
  `.prism-ui` file; `AppLoader` parses it; `ShellInner.app_skeletons`
  caches; `Shell::render` grafts the active app's body into the host
  skeleton's `<shell.app-window>` via `Skeleton::with_app_body`.
  Default app skeleton (`<shell.dock-workspace/>`) preserves
  existing behaviour for apps that declare no skeleton.
- **Hot-swappable apps.** Today the shell loads apps at boot; a
  running app can't be replaced. The manifest format is forward-
  compatible with hot-swap but we don't implement the swap path yet.
- ~~**Service factories.**~~ Promoted to **ADR-010**, landed.
  `ServiceRegistry::add_factory_scoped` registers a closure that
  builds a service from a `ServiceContext`; `rebuild_app_services`
  re-runs every `App`-scoped factory with a fresh context (drops
  prior commands, re-installs new ones, asserts service-id
  stability). Eager `add_scoped` still works byte-compatibly —
  factories are additive.

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

- 2026-05-13: **Per-app skeletons + service factories landed.**
  Closes the two original out-of-scope items.
  - **ADR-009** (`docs/adr/009-per-app-skeletons.md`): host skeleton
    declares an empty `<shell.app-window>` body; `Skeleton::with_app_body`
    grafts the active app's skeleton into that body each frame.
    `AppLoader::discover` reads `[entry] skeleton` from each
    manifest, parses the file, surfaces it as
    `LoadedApp.skeleton`. `ShellInner.app_skeletons` caches every
    parsed app skeleton; `Shell::render` picks the active one via
    `inner.active_app_skeleton()` (falls back to the default
    `<shell.dock-workspace/>`).
    - Tests: 4 new render-side unit tests + 4 new e2e tests in
      `tests/dsl_self_bootstrap.rs`. Includes
      `shell_render_composes_host_with_active_app_body` which
      drives the full chain and asserts the composed tree contains
      the app skeleton's id.
    - `apps/lattice/shell.prism-ui` ships as a proof-of-concept
      app skeleton.
  - **ADR-010** (`docs/adr/010-service-factories.md`):
    `ServiceRegistry::add_factory_scoped` registers a closure
    `Fn(&ServiceContext) -> Arc<dyn ShellService>`. Eager `add` /
    `add_scoped` keep working unchanged (they store `factory:
    None`). `rebuild_app_services` re-runs every `App`-scoped
    factory with a fresh context, dropping the prior instance's
    commands and installing the new instance's commands; asserts
    the factory must preserve service id between rebuilds.
    - Tests: 7 new service-registry unit tests covering eager-back-
      compat, factory registration, rebuild semantics (rerun with
      new context, skip eager, skip universal, swap commands), and
      the id-stability panic.

  **Workspace total: 3646 tests passing**, zero failures,
  `cargo clippy --workspace --all-targets -- -D warnings` clean.

- 2026-05-13: **Deepening wave** — five concrete extensions:
  1. **`prism_core::Catalog<T>`** — generic registry primitive
     (`register / get / iter / len / from_seed / drain / remove`).
     `DockCatalog` collapsed to a thin newtype around it; ADR
     intent ("symmetric to ComponentRegistry / ModifierRegistry")
     made literal. **10 new tests** in `prism-core::registry`.
  2. **`Shell::switch_active_app(id)`** — first-class active-app
     cursor swap. Single call updates `state.workspace.active_app`,
     re-runs every `App`-scoped service factory with a fresh
     `ServiceContext`, and marks `FRAME_DIRTY_SENTINEL`. Idempotent
     (no-op returns `false`). **4 new unit tests** in `shell::tests`.
  3. **`install_services` migrated to ADR-010 factory path.** Luau
     services now register via `add_factory_scoped` so
     `switch_active_app` actually re-binds the bound app id on each
     swap. `LuauScriptedService::new_with_context` is the new
     ctor; `bound_app_id()` accessor lets tests + future Luau
     dispatch confirm the rebind. **2 new app-registry tests**.
  4. **Per-app PRSS stylesheets.** `[entry] styles = "..."` parses
     through `AppLoader`; `ShellInner.app_stylesheets` caches.
     `Stylesheet::merge_with(overlay)` layers app sheet over host
     (overlay wins on conflict for tokens + classes). Render path
     uses the cascaded sheet via `inner.active_app_stylesheet()`.
     **3 new e2e tests**.
  5. **End-to-end `switch_active_app` integration test.**
     `full_swap_chain_skeleton_stylesheet_services_all_track`
     drives all three side effects (skeleton + services +
     stylesheet) through a single `switch_active_app("lattice")`
     call against a manifest declaring all three. **2 new e2e
     tests** plus the headline integration.

  **Workspace total after wave: 3690 tests passing**, zero failures,
  `cargo clippy --workspace --all-targets -- -D warnings` clean.

- 2026-05-13: **Continuation wave** — three more extensions plus a
  cross-crate bug fix:
  1. **`shell.app-card` click → `switch_active_app`.** The launchpad
     event handler used to call `WorkspaceSlot::set_active_app`
     directly, which moved the cursor but bypassed the ADR-009 +
     ADR-010 swap chain. Lifted the body of `Shell::switch_active_app`
     onto `ShellInner` so the public Shell method *and*
     `handle_app_card_click` share one path. **3 new event tests**
     including `pointer_down_on_app_card_drives_full_swap_chain`.
  2. **`ModifierRegistry` collapsed onto `prism_core::Catalog<T>`.**
     Mirrors the DockCatalog refactor — `ModifierRegistry` is now a
     thin newtype around `Catalog<Arc<dyn ModifierBehaviour>>`. The
     `contains` accessor moved to `Catalog`'s general impl block
     (no `HasId` bound) so trait-object catalogs can use it.
     **419 prism-builder tests** unchanged.
  3. **Per-app stylesheet hot-reload methods.**
     `Shell::install_app_stylesheet(id, sheet)` and
     `Shell::uninstall_app_stylesheet(id)` for the dev-loop watcher
     to call when an `app.prss` changes on disk. Marks the frame
     dirty only when the affected app is currently active (idempotent
     for inactive-app cache updates). **5 new shell::tests** + 1 e2e
     test exercising the full install → render flow.
  4. **Bug fix:** `prism_cli::dev_loop::drain_child_io` was aborting
     reader tasks *before* awaiting them, racing with BufReader
     line-forwarding. The flaky `stdout_is_routed_through_the_sink`
     test now passes deterministically. The fix lets readers exit
     naturally on pipe EOF with a 1-second timeout as a backstop.

  **Workspace total after wave: 3712 tests passing**, zero failures,
  `cargo clippy --workspace --all-targets -- -D warnings` clean.

  **Residual follow-up:** binding `prism.register_panel` /
  `prism.register_component` / `prism.register_service` as Luau
  `UserData` methods on a `Arc<dyn AppRegistrar>` handle, threaded
  into the in-process mlua state owned by `prism-daemon`. The
  registrar trait surface is finalised; what remains is the
  `mlua`-side glue and the in-process script lifecycle in
  `prism-daemon::modules::luau_module`.
- 2026-05-13: **Luau registrar bindings landed.** Closes the
  previous wave's residual follow-up.
  1. **`prism_core::luau_bindings::RegistrarHandle`** — new
     `mlua::UserData` wrapper around `Arc<dyn AppRegistrar>` with
     three methods (`register_panel` / `register_component` /
     `register_service`) that accept Lua tables and convert them into
     the matching `*Registration` shapes. Field validation lives in
     three free helpers (`panel_from_table` / `component_from_table` /
     `service_from_table`); registrar errors round-trip back to Lua
     through `mlua::Error::external`. `id` is the only required key
     per registration — other fields default to a reasonable shape.
     **7 new unit tests** covering happy paths, defaults, error
     surfacing, missing-id failure, and the `render_key` /
     `on_event_key` synthesis from inline `render` / `on_event`
     function tables.
  2. **`REGISTRAR_HANDLE_TYPE_NAME` / `_DEF`** — type-stub constants
     in `luau_bindings_consts` (always compiled, no `luau` feature
     gate) so the `prism codegen luau-types` pipeline picks up the
     `AppRegistrar` / `PanelRegistration` / `ComponentRegistration` /
     `ServiceRegistration` shapes alongside the existing handle defs.
     Registered in `luau_types::type_defs`.
  3. **`PrismContext::with_app_registrar`** — daemon-side builder
     that stamps an `Option<RegistrarHandle>` onto the `prism`
     userdata, surfaced as `prism.app`. The bare `luau.exec` path
     keeps the field `nil` so scripts can guard with
     `if prism.app then ...`; script-aware hosts (the eventual
     shell-side script loader) build the context with a live
     registrar wired through. **3 new daemon-side tests** in
     `modules::prism_context::tests`: `prism_app_is_nil_by_default`,
     `prism_app_register_panel_flows_through_to_host_registrar`, and
     `prism_app_register_three_verbs_in_one_script` (drives panel +
     component + service through a single script body — the
     realistic shape of a real app's `main.luau`).
- 2026-05-13: **ADR-009 Phase 1 advanced — Musica + Flux skeletons.**
  `apps/musica/shell.prism-ui` and `apps/flux/shell.prism-ui` ship as
  proof-of-concept distinct skeletons (`musica-stage` / `flux-canvas`
  dock root ids). Both manifests gain `[entry] skeleton = "shell.prism-ui"`.
  The matching `<musica.transport>` / `<musica.timeline>` / `<flux.canvas>`
  components remain unbuilt — until they land the skeletons use a
  distinct dock id so the swap is observable without depending on
  unbuilt blocks. **1 new e2e test** in `tests/dsl_self_bootstrap.rs`
  (`musica_and_flux_each_swap_in_a_distinct_app_skeleton`) drives
  all three apps through `Shell::switch_active_app` and asserts each
  rendered tree carries its app-specific dock id end-to-end.

  **Test deltas:** prism-core +7 unit tests (`luau_bindings::tests`,
  `RegistrarHandle`), prism-daemon +3 unit tests
  (`modules::prism_context::tests`, `prism.app` end-to-end),
  prism-shell +1 e2e test (`tests/dsl_self_bootstrap`, Musica + Flux
  skeleton swap).

  **Remaining gaps:**
  - **Shell-side Luau script lifecycle.** `prism-shell` still ships
    `NoopLuauHost` — apps' `main.luau` files aren't yet executed
    during `Shell::new`. Wiring would call
    `prism_daemon::modules::luau_module::exec_with_setup` once per
    discovered app, installing the live `ShellAppRegistrar` through
    `PrismContext::with_app_registrar` and draining the queues
    afterwards. Blocked on the shell taking a dep on `prism-daemon`
    (or factoring the `exec_with_setup` helper out of the daemon
    into a shareable crate).
  - **Luau render / event dispatch.** `LuauComponentBlock::lower_ui`
    and `LuauScriptedService::on_event` still emit placeholders.
    Closing those needs a persistent Lua state in the shell (not
    the daemon's per-call disposable state) so the registered
    closures are alive when the renderer / event router calls them
    back. Architecture-level decision needed before impl.
  - **Distinct Musica / Flux chrome.** The skeleton swap is wired
    through; the actual `<musica.transport>` / `<musica.timeline>` /
    `<musica.mixer>` / `<flux.canvas>` components are unbuilt. Each
    is a discrete `BlockSpec` + lower fn under
    `prism-shell/src/components/` (or a separate `prism-musica` /
    `prism-flux` crate if the surface grows).
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

