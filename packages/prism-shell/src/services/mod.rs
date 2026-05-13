//! `ShellService` registry — the single seam through which every
//! key, every command, and every mutation reaches the shell.
//!
//! The §17 / §22 read-side contract gave one declarative table per
//! seam (component registry, resolver tag table, shell block
//! registry, prop-bindings table). §24 adds the **write side** in
//! the same shape: one registration table
//! ([`register_shell_services`]), one declarative trait
//! ([`ShellService`]), one fan-out router. Adding a feature is one
//! `impl ShellService` plus one row in the table; adding a command
//! is one row in some service's [`ShellService::commands`] vec.
//!
//! See `docs/dev/clay-migration-plan.md` §24 for the architectural
//! contract.
//!
//! ## The four contracts
//!
//! - [`ShellService`]   — every feature implements this.
//! - [`MutCtx`]         — sister to `PropCtx`, but `&mut`. The
//!   single carrier into every event handler and command body.
//! - [`EventOutcome`]   — three states: `Pass`/`Handled`/`HandledQuiet`.
//! - [`ServiceRegistry`] — fan-out in declared order; first
//!   `Handled` wins. Holds the [`CommandTable`] commands contributed
//!   to at registration time.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use prism_ui_runtime::event::Event;
use prism_ui_runtime::layout::Viewport;

use crate::AppState;

pub mod base;
pub mod builder;
pub mod clipboard;
pub mod field_focus;
pub mod help;
pub mod input;
pub mod luau;
pub mod menu;
pub mod palette;
pub mod persistence;
pub mod project;
pub mod search;
pub mod selection;
pub mod signals;
pub mod undo;
pub mod vfs;

pub use base::ShellBaseService;
pub use builder::BuilderService;
pub use clipboard::{Clipboard, ClipboardService};
pub use field_focus::FieldFocusService;
pub use help::HelpService;
pub use input::{InputScheme, InputService};
pub use luau::{LuauHost, LuauService, NoopLuauHost};
pub use menu::MenuService;
pub use palette::CommandPaletteService;
pub use persistence::PersistenceService;
pub use project::ProjectService;
pub use search::SearchService;
pub use selection::SelectionService;
pub use signals::SignalsService;
pub use undo::{UndoRedoService, UndoStack};
pub use vfs::{OsVfs, Vfs, VfsError};

// ── trait + outcome ────────────────────────────────────────────────

/// Every shell feature implements this. Three pure methods, no
/// inheritance, no associated types — the whole behavioural surface.
pub trait ShellService: Send + Sync {
    /// Stable, kebab-case identifier used by lookup-style access
    /// (`registry.get("luau")`). Cross-service reach is rare —
    /// `SignalsService` calling `LuauService` is the canonical case.
    fn id(&self) -> &'static str;

    /// React to one event. Default: `Pass` (the registry tries the
    /// next service). Returning `Handled` short-circuits the fan-out;
    /// returning `HandledQuiet` short-circuits without requesting a
    /// redraw (e.g. focus-tick).
    fn on_event(
        &self,
        _event: &Event,
        _ctx: &mut MutCtx<'_>,
        _cmds: &CommandTable,
    ) -> EventOutcome {
        EventOutcome::Pass
    }

    /// Commands this service contributes. Indexed by id at
    /// registration; collisions are a registration-time error
    /// surfaced as a panic (programmer error, never user-visible).
    fn commands(&self) -> Vec<CommandSpec> {
        Vec::new()
    }
}

/// What a service signals back to the router.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventOutcome {
    /// Service ignored this event; try the next one.
    Pass,
    /// Service consumed it; stop fan-out, redraw next frame.
    Handled,
    /// Service consumed it; stop fan-out, no redraw needed.
    HandledQuiet,
}

// ── borrow-pack ────────────────────────────────────────────────────

/// Mutation borrow-pack — the `&mut` mirror of `PropCtx`. Every
/// event handler and every command body takes one of these. Adding
/// a new datum is one field here; existing services ignore it.
pub struct MutCtx<'a> {
    pub state: &'a mut AppState,
    pub viewport: Viewport,
    pub undo: &'a mut UndoStack,
    /// Filesystem seam — `OsVfs` in production, in-memory mock in
    /// tests. `PersistenceService` / `ProjectService` are the only
    /// services that read this; everyone else ignores the field.
    pub vfs: &'a mut dyn Vfs,
    /// Luau runtime seam — `NoopLuauHost` when the `mlua` feature is
    /// off, the real mlua-backed host when it's on. `SignalsService`
    /// reaches Luau through this field, *not* through
    /// `ServiceRegistry::get("luau")` — shared resources go on
    /// [`MutCtx`], not behind cross-service trait calls.
    pub luau: &'a mut dyn LuauHost,
    /// One-cell internal clipboard. `ClipboardService` (§25) is the
    /// only consumer; system-clipboard plug-ins (`arboard`) attach
    /// at the service body, not on this field.
    pub clipboard: &'a mut Clipboard,
    /// §43 C1 read-only seam: command bodies that re-derive the
    /// builder slot's selection-driven panels (inspector tree,
    /// property rows) reach the live `ComponentRegistry` through
    /// this field. The mirror of `PropCtx.registry` on the write
    /// side. `None` keeps headless tests and command-table-only
    /// callers compiling — derivations no-op when the registry
    /// isn't available.
    pub registry: Option<&'a prism_builder::ComponentRegistry>,
    /// **Wave 1** read-only seam: command bodies that attach /
    /// detach / toggle / reorder modifiers reach the registry
    /// through this field. Mirror of `PropCtx.modifier_registry`.
    pub modifier_registry: Option<&'a prism_builder::ModifierRegistry>,
}

// ── command spec + table ──────────────────────────────────────────

/// One executable command. Declared next to its owning service via
/// [`ShellService::commands`].
#[derive(Clone)]
pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub category: &'static str,
    pub shortcut: Option<&'static str>,
    pub handler: Arc<dyn Fn(&mut MutCtx<'_>) + Send + Sync>,
}

impl std::fmt::Debug for CommandSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandSpec")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("category", &self.category)
            .field("shortcut", &self.shortcut)
            .finish_non_exhaustive()
    }
}

/// The single source of truth for "what commands exist and how to
/// run them." `InputService` resolves a key combo to an id and asks
/// this table to dispatch; `CommandPaletteService` will read its
/// rows for fuzzy filtering. Owns no state — the table is
/// constructed once at registration and treated as immutable
/// thereafter.
#[derive(Default)]
pub struct CommandTable {
    map: HashMap<&'static str, CommandSpec>,
}

impl CommandTable {
    pub fn run(&self, id: &str, ctx: &mut MutCtx<'_>) -> bool {
        match self.map.get(id) {
            Some(spec) => {
                (spec.handler)(ctx);
                true
            }
            None => false,
        }
    }

    pub fn get(&self, id: &str) -> Option<&CommandSpec> {
        self.map.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.map.keys().copied()
    }

    /// Iterate `(id, label)` pairs for every registered command. The
    /// single aggregator the command palette's fuzzy filter consumes
    /// (§25); no service rebuilds this list.
    pub fn rows(&self) -> Vec<(&'static str, &'static str)> {
        self.map.values().map(|s| (s.id, s.label)).collect()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

// ── registry ──────────────────────────────────────────────────────

/// Activation scope for a registered service. See
/// `docs/dev/dsl-self-bootstrap.md` Loop 3.
///
/// - [`Universal`](Self::Universal) — always present. Shell chrome
///   needs the service regardless of which app is mounted (Input,
///   UndoRedo, Palette, etc.).
/// - [`App`](Self::App) — gated. Only active when at least one loaded
///   app's manifest declares the service id in `services.required` or
///   `services.optional`. Filtered through
///   [`ServiceRegistry::activate_app_services`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ServiceScope {
    Universal,
    App,
}

struct RegisteredService {
    scope: ServiceScope,
    service: Arc<dyn ShellService>,
    command_ids: Vec<&'static str>,
}

/// One row per service, fan-out in declared order.
#[derive(Default)]
pub struct ServiceRegistry {
    services: Vec<RegisteredService>,
    by_id: HashMap<&'static str, Arc<dyn ShellService>>,
    commands: CommandTable,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// The same shape `ShellPropBindings::with_builtins()` exposes
    /// for the read side — one constructor that lands every
    /// shipped service. Test code can build an empty registry via
    /// [`Self::new`] and add only the services under test.
    pub fn with_builtins() -> Self {
        let mut reg = Self::new();
        register_shell_services(&mut reg);
        reg
    }

    /// Register a universal service (the default scope). Equivalent to
    /// `add_scoped(ServiceScope::Universal, service)`.
    pub fn add<S: ShellService + 'static>(&mut self, service: S) {
        self.add_scoped(ServiceScope::Universal, service);
    }

    /// Register a service with an explicit [`ServiceScope`]. Apps that
    /// push their own services (Luau-defined / manifest-declared)
    /// register through this method with `ServiceScope::App` so
    /// [`Self::activate_app_services`] can filter them.
    pub fn add_scoped<S: ShellService + 'static>(&mut self, scope: ServiceScope, service: S) {
        let arc: Arc<dyn ShellService> = Arc::new(service);
        let id = arc.id();
        let mut command_ids = Vec::new();
        for spec in arc.commands() {
            assert!(
                !self.commands.map.contains_key(spec.id),
                "duplicate command id `{}` (declared by service `{}`)",
                spec.id,
                id
            );
            command_ids.push(spec.id);
            self.commands.map.insert(spec.id, spec);
        }
        assert!(!self.by_id.contains_key(id), "duplicate service id `{id}`");
        self.by_id.insert(id, Arc::clone(&arc));
        self.services.push(RegisteredService {
            scope,
            service: arc,
            command_ids,
        });
    }

    /// Filter `App`-scoped services against an allowlist. `Universal`
    /// services are untouched. Each dropped service's commands are
    /// removed from the [`CommandTable`] as well, so the palette never
    /// surfaces a stub that won't run.
    ///
    /// Pass the union of every loaded `AppManifest`'s
    /// `services.required` + `services.optional` ids.
    pub fn activate_app_services(&mut self, allowed: &HashSet<&str>) {
        let mut keep_flags = Vec::with_capacity(self.services.len());
        for r in &self.services {
            let keep =
                matches!(r.scope, ServiceScope::Universal) || allowed.contains(r.service.id());
            keep_flags.push(keep);
        }
        let mut i = 0;
        self.services.retain(|r| {
            let keep = keep_flags[i];
            i += 1;
            if !keep {
                self.by_id.remove(r.service.id());
                for cmd_id in &r.command_ids {
                    self.commands.map.remove(cmd_id);
                }
            }
            keep
        });
    }

    /// Cross-service reach. Used sparingly — `SignalsService` calling
    /// `LuauService::exec_handler` is the canonical case (the only
    /// reason the registry is also a lookup table).
    pub fn get(&self, id: &str) -> Option<Arc<dyn ShellService>> {
        self.by_id.get(id).cloned()
    }

    pub fn commands(&self) -> &CommandTable {
        &self.commands
    }

    /// Walk services in declared order, short-circuit on the first
    /// `Handled` / `HandledQuiet`. The router calls this for every
    /// event variant that isn't a §22 pointer arm.
    pub fn fan_out(&self, event: &Event, ctx: &mut MutCtx<'_>) -> EventOutcome {
        for r in &self.services {
            match r.service.on_event(event, ctx, &self.commands) {
                EventOutcome::Pass => continue,
                outcome => return outcome,
            }
        }
        EventOutcome::Pass
    }

    pub fn service_ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.services.iter().map(|r| r.service.id())
    }

    /// Scope of a registered service, by id. Returns `None` if the id
    /// isn't registered.
    pub fn scope_of(&self, id: &str) -> Option<ServiceScope> {
        self.services
            .iter()
            .find(|r| r.service.id() == id)
            .map(|r| r.scope)
    }

    /// Number of currently registered services.
    pub fn len(&self) -> usize {
        self.services.len()
    }

    /// Whether the registry has zero services.
    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
    }
}

// ── registration table ────────────────────────────────────────────

/// Sister to `register_shell_builtins` (blocks) and
/// `ShellPropBindings::with_builtins` (read bindings). Reads as a
/// flat list of one-line rows; adding a new feature is exactly two
/// edits — one `impl ShellService for FooService` plus one row here.
///
/// Order matters: services that *capture* events (modal overlays,
/// the command palette while open) must come before services that
/// would otherwise consume them. The base service is first so its
/// commands (undo/redo/palette) are in the table when later
/// services' `commands()` runs — but since command lookup happens
/// at run time through [`CommandTable`], the only ordering effect
/// is event fan-out.
pub fn register_shell_services(reg: &mut ServiceRegistry) {
    use ServiceScope::{App, Universal};

    reg.add_scoped(Universal, ShellBaseService);
    reg.add_scoped(Universal, UndoRedoService);
    // B4 — `FieldFocusService` must register ahead of `InputService`
    // and the modal overlays. When the user is typing into a
    // property-row text field, Text events and plain Esc/Enter/
    // Backspace key events should reach the focus session before any
    // global shortcut, palette, or search modal interprets them.
    // Modifier-bearing keys (Ctrl+S etc.) still pass through.
    reg.add_scoped(Universal, FieldFocusService);
    // §25 — `CommandPaletteService` MUST register ahead of `InputService`
    // so its modal-capture `on_event` (returns `Handled` while open)
    // short-circuits Ctrl+S / Ctrl+F / etc. before InputService can
    // resolve them.
    reg.add_scoped(Universal, CommandPaletteService);
    reg.add_scoped(Universal, InputService::with_defaults());
    reg.add_scoped(Universal, SelectionService);
    // DSL self-bootstrap Loop 3: `App`-scoped services are dropped by
    // `activate_app_services` unless the loaded app manifests include
    // them in `services.required` / `services.optional`. Builder /
    // Signals / Luau / Project are tied to the live document — the
    // launchpad never needs them.
    reg.add_scoped(App, BuilderService);
    reg.add_scoped(Universal, ClipboardService);
    // §26 — IO services. `Persistence` + `Search` are universal (every
    // app gets File menu + Ctrl+F); `Project` is app-scoped (only
    // project-aware apps mount the explorer panel).
    reg.add_scoped(Universal, PersistenceService);
    reg.add_scoped(App, ProjectService);
    reg.add_scoped(Universal, SearchService);
    // §27 — cross-service services. `Help` + `Menu` are universal
    // (every app surfaces them); `Signals` + `Luau` are app-scoped
    // (signal-connections / .luau-script-bearing apps).
    reg.add_scoped(Universal, HelpService::default());
    reg.add_scoped(Universal, MenuService);
    reg.add_scoped(App, SignalsService);
    reg.add_scoped(App, LuauService);
}

// ── declarative `cmd!` macro ──────────────────────────────────────

/// One-line command declaration. Used inside `commands()` impls so
/// every service's contribution reads as a flat list.
///
/// ```ignore
/// cmd!("undo", "Undo", "Edit", "Ctrl+Z", |ctx| { ctx.undo.undo(ctx.state); })
/// ```
#[macro_export]
macro_rules! cmd {
    ($id:expr, $label:expr, $cat:expr, $handler:expr $(,)?) => {
        $crate::services::CommandSpec {
            id: $id,
            label: $label,
            category: $cat,
            shortcut: None,
            handler: ::std::sync::Arc::new($handler),
        }
    };
    ($id:expr, $label:expr, $cat:expr, $shortcut:expr, $handler:expr $(,)?) => {
        $crate::services::CommandSpec {
            id: $id,
            label: $label,
            category: $cat,
            shortcut: Some($shortcut),
            handler: ::std::sync::Arc::new($handler),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every command declared by every registered service is
    /// dispatchable through the [`CommandTable`]. Forgetting to wire
    /// a command is a compile-or-test failure, not a silent dead
    /// shortcut.
    #[test]
    fn commands_are_unique_and_dispatchable() {
        let reg = ServiceRegistry::with_builtins();
        for id in reg.commands().ids() {
            assert!(
                reg.commands().get(id).is_some(),
                "command `{id}` declared but not dispatchable"
            );
        }
        assert!(
            reg.commands().len() >= 4,
            "builtins must declare at least undo/redo/palette/save"
        );
    }

    #[test]
    fn lookup_returns_registered_services() {
        let reg = ServiceRegistry::with_builtins();
        for id in ["shell.base", "input", "undo-redo"] {
            assert!(reg.get(id).is_some(), "service `{id}` missing");
        }
    }

    #[test]
    fn builtin_scopes_match_design() {
        // DSL self-bootstrap Loop 3: the manifest-driven activation
        // path filters App-scoped services against the loaded app
        // manifests. Universal services are always present.
        let reg = ServiceRegistry::with_builtins();
        assert_eq!(reg.scope_of("builder"), Some(ServiceScope::App));
        assert_eq!(reg.scope_of("signals"), Some(ServiceScope::App));
        assert_eq!(reg.scope_of("luau"), Some(ServiceScope::App));
        assert_eq!(reg.scope_of("project"), Some(ServiceScope::App));
        for universal in [
            "shell.base",
            "undo-redo",
            "field-focus",
            "command-palette",
            "input",
            "selection",
            "clipboard",
            "persistence",
            "search",
            "help",
            "menu",
        ] {
            assert_eq!(
                reg.scope_of(universal),
                Some(ServiceScope::Universal),
                "service `{universal}` should be Universal",
            );
        }
    }

    #[test]
    fn activate_app_services_drops_unlisted_app_scoped() {
        let mut reg = ServiceRegistry::with_builtins();
        let before = reg.len();
        let allowed: HashSet<&str> = ["builder"].into_iter().collect();
        reg.activate_app_services(&allowed);
        // Builder kept (allowed); Signals / Luau / Project dropped.
        assert!(reg.get("builder").is_some());
        assert!(reg.get("signals").is_none());
        assert!(reg.get("luau").is_none());
        assert!(reg.get("project").is_none());
        // Universal services untouched.
        assert!(reg.get("input").is_some());
        assert!(reg.get("undo-redo").is_some());
        assert_eq!(reg.len(), before - 3);
        // Dropped services' commands removed from the table.
        for dead in ["signals.refresh", "luau.run-selection"] {
            assert!(
                reg.commands().get(dead).is_none(),
                "dropped service's command `{dead}` should be unreachable"
            );
        }
    }

    #[test]
    fn activate_app_services_with_empty_allowlist_drops_every_app_scoped() {
        let mut reg = ServiceRegistry::with_builtins();
        let allowed: HashSet<&str> = HashSet::new();
        reg.activate_app_services(&allowed);
        for app in ["builder", "signals", "luau", "project"] {
            assert!(reg.get(app).is_none(), "`{app}` should be dropped");
        }
    }

    #[test]
    fn fan_out_short_circuits_on_handled() {
        // A service that always returns `Handled` for any event must
        // prevent later services from seeing it. We build a tiny
        // fake registry; we don't use builtins here because the
        // builtins legitimately Pass on most synthetic events.
        struct Capture;
        impl ShellService for Capture {
            fn id(&self) -> &'static str {
                "test.capture"
            }
            fn on_event(
                &self,
                _ev: &Event,
                _ctx: &mut MutCtx<'_>,
                _cmds: &CommandTable,
            ) -> EventOutcome {
                EventOutcome::Handled
            }
        }
        struct ShouldNotRun;
        impl ShellService for ShouldNotRun {
            fn id(&self) -> &'static str {
                "test.should-not-run"
            }
            fn on_event(
                &self,
                _ev: &Event,
                _ctx: &mut MutCtx<'_>,
                _cmds: &CommandTable,
            ) -> EventOutcome {
                panic!("should not run after Handled");
            }
        }
        let mut reg = ServiceRegistry::new();
        reg.add(Capture);
        reg.add(ShouldNotRun);
        let mut state = AppState::default();
        let mut undo = UndoStack::default();
        let mut vfs = OsVfs;
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let mut ctx = MutCtx {
            state: &mut state,
            viewport: Viewport {
                width: 1.0,
                height: 1.0,
            },
            undo: &mut undo,
            vfs: &mut vfs,
            luau: &mut luau,
            clipboard: &mut clipboard,
            registry: None,
            modifier_registry: None,
        };
        assert_eq!(
            reg.fan_out(&Event::Wheel { dx: 0.0, dy: 0.0 }, &mut ctx),
            EventOutcome::Handled
        );
    }
}
