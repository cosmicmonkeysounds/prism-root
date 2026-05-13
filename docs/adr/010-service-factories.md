# ADR-010: Service Factories

**Status:** Accepted
**Date:** 2026-05-13

## Context

`prism_shell::services::ServiceRegistry` registers services as
concrete `impl ShellService` instances:

```rust
let mut reg = ServiceRegistry::new();
reg.add(ShellBaseService);
reg.add_scoped(ServiceScope::App, BuilderService);
// ...
```

Each `add*` call eagerly constructs the service, stores it as
`Arc<dyn ShellService>`, walks its `.commands()` once at registration
time, and merges those commands into the `CommandTable`. Lookups via
`registry.get(id)` and `fan_out(event, ctx)` see a fully-constructed
service from frame one.

This eager model worked through the first wave of `docs/dev/dsl-self-bootstrap.md`.
Three subsequent pressures make it limiting:

1. **Luau-defined services.** The Loop 4 work landed
   `LuauScriptedService` — a `ShellService` shim driven by a script
   handler key. Today the shim is constructed at boot, before the
   Luau runtime exists, and its body returns `EventOutcome::Pass`
   unconditionally. When the runtime lands, the shim will need to
   resolve its script handle from a Luau context that doesn't exist
   at registration time. Eager construction forces awkward
   `Option<LuauHandle>` plumbing or an "uninitialized" state every
   call site has to guard against.

2. **Hot-reload.** Phase 9 of `docs/dev/dioxus-inspiration.md`
   wraps the render walk in `subsecond::call` so a swapped-in
   `lower_ui` body patches in place without rebuilding the shell.
   Services don't share that seam — a code change in
   `BuilderService::on_event` still requires a process restart.
   Factories give us a re-instantiation hook: if the factory is
   stored alongside the service, hot-swap can drop the old service
   and rebuild via the factory without touching `Shell::new`.

3. **Per-app service variants.** Two apps might register two
   different services with the same id (e.g. `persistence` —
   Lattice writes to a CRDT, Flux writes to a flat-file VFS). The
   eager registration path panics on duplicate id, which is correct
   for built-ins but blocks app overrides. A factory keyed by
   `(app_id, service_id)` lets the registry resolve the right
   instance per active app.

### Prior art surveyed

- **VS Code `ExtensionContext`** — extensions register
  contributions (commands, languages, providers) as factories.
  VS Code instantiates them lazily when a feature first needs them.
  Proven model.
- **`tower::Service` / `tower::ServiceBuilder`** — Tower's middleware
  stack is built from factories that return concrete `Service`
  impls. The factory pattern carries through every middleware
  composition.
- **Axum `Handler` registration** — handlers are zero-sized
  function items that produce a `Service` lazily on each request.
  Effectively factory-per-request.
- **Bevy's `App::add_systems`** — systems are functions registered
  by reference; Bevy calls them each frame. Mirrors the
  "registration is a description of what to run, not the value
  itself" pattern.

The cluster of designs all share one shape: **registration is
a description; instantiation is separate.** That's the shape we're
adopting.

## Decision

Add a **factory** registration path alongside the existing eager
`add_scoped`. Factories produce `Arc<dyn ShellService>` on demand;
eager `add_scoped` becomes a thin wrapper that pre-runs the factory
at registration time.

### New types

```rust
/// Carrier handed to a service factory at construction time. Holds
/// references to the resources the factory may need to capture (the
/// active app's id, shared registries, etc.) without forcing the
/// factory closure to take ten parameters.
pub struct ServiceContext<'a> {
    pub app_id: Option<&'a str>,
    // future fields land here — registries, the Luau handle, etc.
}

/// Build a service. Called once per registration (eager) or once per
/// activation (lazy). Returns a fully-constructed
/// `Arc<dyn ShellService>` ready to participate in `fan_out`.
pub type ServiceFactory =
    Box<dyn Fn(&ServiceContext<'_>) -> Arc<dyn ShellService> + Send + Sync>;
```

### New API

```rust
impl ServiceRegistry {
    /// Register a factory for a service with explicit scope.
    /// Runs the factory eagerly today; the lazy-instantiation
    /// variant is a follow-up gated on hot-reload.
    pub fn add_factory_scoped(
        &mut self,
        scope: ServiceScope,
        factory: ServiceFactory,
    );

    /// Re-run every factory whose scope matches `App`. Used by
    /// hot-reload and per-app-swap. Drops the prior instances; the
    /// `CommandTable` is rebuilt from the new instances.
    pub fn rebuild_app_services(&mut self, ctx: &ServiceContext<'_>);
}
```

### Existing API stays unchanged

```rust
impl ServiceRegistry {
    pub fn add<S: ShellService + 'static>(&mut self, service: S);
    pub fn add_scoped<S: ShellService + 'static>(
        &mut self,
        scope: ServiceScope,
        service: S,
    );
}
```

Both delegate to `add_factory_scoped` internally with a closure that
captures the pre-built `Arc` and returns clones. Existing call sites
in `register_shell_services` keep working byte-for-byte.

### Storage shape

```rust
struct RegisteredService {
    scope: ServiceScope,
    factory: ServiceFactory,
    instance: Arc<dyn ShellService>,
    command_ids: Vec<&'static str>,
}
```

Both `factory` and `instance` are stored. The factory is kept for
`rebuild_app_services` and future hot-reload; the instance is what
`fan_out` and `get` operate on day-to-day.

### When the factory runs

| Path | Behaviour today |
|---|---|
| `add` / `add_scoped` | Factory runs immediately during registration. |
| `add_factory_scoped` | Factory runs immediately during registration. |
| `rebuild_app_services` | Re-runs every `App`-scoped factory with the new context. |

Lazy instantiation (factory runs on first event) is an explicit
non-goal for this ADR. It introduces a subtle "first event after
boot is slower" wrinkle and complicates `commands()` aggregation —
the factory has to run before `register_shell_services` returns so
the command table is fully populated for `InputService`. Eager
instantiation matches today's contract; lazy is a future ADR.

### Integration with `ShellAppRegistrar`

The Loop 4 `install_services` helper currently calls
`services.add_scoped(ServiceScope::App, LuauScriptedService::new(spec))`.
After this ADR lands it can switch to:

```rust
services.add_factory_scoped(
    ServiceScope::App,
    Box::new(move |ctx| {
        Arc::new(LuauScriptedService::new_with_context(spec.clone(), ctx))
    }),
);
```

The factory captures `spec` (a small `Clone` struct) and reads the
active-app id from `ctx`. When the user switches apps, the registry
re-runs the factory with the new context and the Luau handler resolves
against the new app's script set.

### Migration

Phase 0 (this ADR's implementation):

1. Introduce `ServiceContext` (empty body today, just the `app_id`
   field).
2. Introduce `ServiceFactory` type alias.
3. Refactor `RegisteredService` to hold both `factory` and `instance`.
4. Add `add_factory_scoped` and `rebuild_app_services`.
5. Refactor `add_scoped` to delegate (no caller change).
6. Add unit tests covering the factory path + the rebuild path.

Phase 1 (follow-on, not in this ADR):

- ~~Convert `install_services` to use factories so Luau-backed
  services pick up the active-app context.~~ Landed —
  `install_services` calls `add_factory_scoped` with a closure that
  stamps the `ServiceContext::app_id` onto each
  `LuauScriptedService::new_with_context` invocation.
- ~~Wire the active-app cursor (`WorkspaceSlot::set_active_app`) to
  call `rebuild_app_services` on change.~~ Landed —
  `ShellInner::switch_active_app` now drives every App-scoped
  factory rebuild before marking `FRAME_DIRTY_SENTINEL`. Both the
  public `Shell::switch_active_app` entry and the launchpad
  pointer-down handler share this path.
- Add lazy instantiation gate on hot-reload (Phase 9 of
  `dioxus-inspiration.md`) — still open. Today every factory runs
  eagerly at registration; the seam exists but the lazy flip
  pre-supposes the subsecond hot-reload pipeline that's still
  feature-gated off.

## Rationale

- **Additive.** Every existing call site keeps working. The eager
  path becomes a wrapper around the factory path; no behaviour
  changes for the 14 built-in services.
- **Symmetric to `ComponentRegistry` / `ModifierRegistry`.** Both
  already register through a function (`BlockSpec::new(...).lower(fn)`).
  Services were the odd one out — concrete instances rather than
  descriptions of how to build them. This ADR brings them in line.
- **Hot-reload-ready.** Factories store the "how to build" so a
  later hot-reload pass can re-run them without re-entering
  `Shell::new`. Even before hot-reload lands, the seam is in place.
- **Per-app overrides.** Different apps can register different
  factories for the same id; `rebuild_app_services` swaps them
  when the active-app cursor moves.

## Consequences

- `RegisteredService` gains a `factory` field. Memory footprint
  grows by one `Box<Fn>` per service (14 today, single-digit per app).
- `add` / `add_scoped` get one extra closure-allocation per call,
  paid once at boot. Negligible.
- `ServiceContext` is a struct with one field today; adding more
  fields later is non-breaking because callers pass it through
  by reference.
- The `command_ids` cleanup in `activate_app_services` continues
  to work; the new code path doesn't change the drop semantics.
- Tests gain one new unit-test file for the factory + rebuild paths.

## Out of scope (intentional)

- **Lazy instantiation.** Factories run eagerly today. Lazy is a
  follow-up gated on the hot-reload story.
- **Command-table rebuild incrementality.** `rebuild_app_services`
  drops every `App`-scoped service and re-runs every factory.
  Incremental rebuild (only re-run the factory whose script
  changed) is a future optimisation.
- **Per-frame factories.** `tower::Service` factories run per
  request. Prism services run per frame at most, and that's still
  not what factories here capture — they run once per active-app
  cursor change. Per-frame factories aren't in scope.
- **Async factories.** Factories are sync. If a service needs an
  async resource (a database connection, an HTTP client), the
  factory should construct a synchronous handle that internally
  manages async work via `tokio::spawn`. Async factories are a
  separate ADR.
