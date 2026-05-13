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
