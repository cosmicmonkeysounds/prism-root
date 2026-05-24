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
pub mod code_editor;
pub mod devtools;
pub mod editor_files;
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
pub mod text_input;
pub mod undo;
pub mod vfs;

pub use base::ShellBaseService;
pub use builder::BuilderService;
pub use clipboard::{Clipboard, ClipboardService};
pub use code_editor::CodeEditorService;
pub use devtools::DevToolsService;
pub use editor_files::EditorFilesService;
pub use field_focus::FieldFocusService;
pub use help::HelpService;
pub use input::{InputScheme, InputService};
#[cfg(feature = "web")]
pub use luau::JsLuauHost;
#[cfg(feature = "native")]
pub use luau::MluaLuauHost;
pub use luau::{LuauHost, LuauService, NoopLuauHost};
pub use menu::MenuService;
pub use palette::CommandPaletteService;
pub use persistence::PersistenceService;
pub use project::ProjectService;
pub use search::SearchService;
pub use selection::SelectionService;
pub use signals::SignalsService;
pub use undo::{UndoRedoService, UndoStack};
pub use vfs::{FilePickerSpec, OsVfs, Vfs, VfsError};

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

/// ADR-010: carrier handed to a [`ServiceFactory`] at construction
/// time. Holds references to the resources the factory may need to
/// capture (the active app's id, shared registries, etc.) without
/// forcing the factory closure to take ten parameters.
///
/// Today's body carries one field; new fields land here as the
/// factory path grows (Luau handle, app registrar reference, etc.).
#[derive(Clone, Copy, Debug, Default)]
pub struct ServiceContext<'a> {
    /// The active app's id when the factory is constructing a service
    /// for a specific app. `None` for universal services and for the
    /// initial boot pass where no app has been activated yet.
    pub app_id: Option<&'a str>,
}

/// ADR-010: build a service on demand. Run once per registration
/// (eager) or once per activation (lazy, via
/// [`ServiceRegistry::rebuild_app_services`]). Returns a
/// fully-constructed `Arc<dyn ShellService>` ready to participate in
/// `fan_out`.
pub type ServiceFactory = Box<dyn Fn(&ServiceContext<'_>) -> Arc<dyn ShellService> + Send + Sync>;

struct RegisteredService {
    scope: ServiceScope,
    service: Arc<dyn ShellService>,
    command_ids: Vec<&'static str>,
    /// ADR-010: optional factory that knows how to rebuild this
    /// service from a fresh [`ServiceContext`]. Populated when the
    /// caller used [`ServiceRegistry::add_factory_scoped`]; `None` for
    /// services registered via the legacy eager
    /// [`ServiceRegistry::add`] / [`ServiceRegistry::add_scoped`]
    /// path (those services skip the
    /// [`ServiceRegistry::rebuild_app_services`] pass).
    factory: Option<ServiceFactory>,
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
        // ADR-010: eager registration installs no factory — the
        // service is fixed by construction. `rebuild_app_services`
        // skips entries with `factory: None`.
        self.install(scope, Arc::new(service), None);
    }

    /// ADR-010: register a service via a factory closure. The factory
    /// runs immediately to produce the initial instance (eager
    /// instantiation matches today's contract — see ADR-010 §"When
    /// the factory runs"). The factory is kept alongside the instance
    /// so [`Self::rebuild_app_services`] can re-run it with a fresh
    /// [`ServiceContext`].
    ///
    /// Apps that push Luau-backed services pay for this path so
    /// the runtime can re-bind the script handle when the active
    /// app changes.
    pub fn add_factory_scoped(&mut self, scope: ServiceScope, factory: ServiceFactory) {
        let ctx = ServiceContext::default();
        let instance = factory(&ctx);
        self.install(scope, instance, Some(factory));
    }

    /// Hot-reload variant of [`Self::add_factory_scoped`]. If a service
    /// with the same id is already registered, drop it (along with the
    /// commands it contributed) before installing the new factory.
    /// Used by `Shell::install_app_script` so re-running a `main.luau`
    /// that calls `register_service({id="x", ...})` doesn't panic on
    /// the second-run duplicate-id assertion.
    pub fn add_or_replace_factory_scoped(&mut self, scope: ServiceScope, factory: ServiceFactory) {
        // Build the new instance first so we can grab its id (we need
        // to remove the prior registration before `install` runs its
        // duplicate-id assert).
        let ctx = ServiceContext::default();
        let instance = factory(&ctx);
        let id = instance.id();
        if self.by_id.contains_key(id) {
            // Drop the prior entry + its command-table contributions
            // before re-installing — same teardown shape as
            // `activate_app_services`.
            self.services.retain(|r| {
                if r.service.id() == id {
                    for cmd_id in &r.command_ids {
                        self.commands.map.remove(cmd_id);
                    }
                    self.by_id.remove(r.service.id());
                    false
                } else {
                    true
                }
            });
        }
        self.install(scope, instance, Some(factory));
    }

    /// Internal: install an already-built `Arc<dyn ShellService>` into
    /// the registry, recording the optional factory for future
    /// rebuild passes. Used by both [`Self::add_scoped`] (factory =
    /// `None`) and [`Self::add_factory_scoped`] (factory = `Some`).
    fn install(
        &mut self,
        scope: ServiceScope,
        service: Arc<dyn ShellService>,
        factory: Option<ServiceFactory>,
    ) {
        let id = service.id();
        let mut command_ids = Vec::new();
        for spec in service.commands() {
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
        self.by_id.insert(id, Arc::clone(&service));
        self.services.push(RegisteredService {
            scope,
            service,
            command_ids,
            factory,
        });
    }

    /// ADR-010: re-run every [`ServiceScope::App`] factory with a
    /// fresh [`ServiceContext`]. Drops the prior instance, replaces
    /// it with the factory's output, and rebuilds the command table
    /// entries those services contributed. Used by the active-app
    /// cursor swap (Phase 1 follow-up) and by hot-reload.
    ///
    /// Services registered through the eager
    /// [`Self::add`] / [`Self::add_scoped`] path have no factory and
    /// are left untouched — only factory-backed services rebuild.
    pub fn rebuild_app_services(&mut self, ctx: &ServiceContext<'_>) {
        // Walk the registered services, rebuilding any `App`-scoped
        // entry whose `factory` is `Some`. Collect updates after the
        // loop so the borrow checker doesn't fight us — we need both
        // the existing service's command_ids (to drop) and the new
        // service's commands (to install).
        struct Rebuild {
            idx: usize,
            new_instance: Arc<dyn ShellService>,
            new_command_ids: Vec<&'static str>,
            new_command_specs: Vec<CommandSpec>,
            old_command_ids: Vec<&'static str>,
        }
        let mut updates: Vec<Rebuild> = Vec::new();
        for (idx, r) in self.services.iter().enumerate() {
            if !matches!(r.scope, ServiceScope::App) {
                continue;
            }
            let Some(factory) = r.factory.as_ref() else {
                continue;
            };
            let new_instance = factory(ctx);
            let new_command_specs = new_instance.commands();
            let new_command_ids: Vec<&'static str> =
                new_command_specs.iter().map(|s| s.id).collect();
            updates.push(Rebuild {
                idx,
                new_instance,
                new_command_ids,
                new_command_specs,
                old_command_ids: r.command_ids.clone(),
            });
        }
        // Apply updates: drop old commands, swap instances, install
        // new commands. The `by_id` map keys on the service id, which
        // a factory must not change between rebuilds (that would
        // break cross-service lookups). We assert this invariant.
        for u in updates {
            let r = &mut self.services[u.idx];
            let old_id = r.service.id();
            let new_id = u.new_instance.id();
            assert_eq!(
                old_id, new_id,
                "ADR-010: factory rebuild must preserve service id (was `{old_id}`, now `{new_id}`)"
            );
            for cmd_id in &u.old_command_ids {
                self.commands.map.remove(cmd_id);
            }
            for spec in u.new_command_specs {
                self.commands.map.insert(spec.id, spec);
            }
            self.by_id.insert(new_id, Arc::clone(&u.new_instance));
            r.service = u.new_instance;
            r.command_ids = u.new_command_ids;
        }
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
    // The in-shell code editor (`shell.code-editor`) keyboard router.
    // Registers after `FieldFocusService` so an open property-row
    // text field still wins fan-out — typing into a property cell
    // over a code-editor panel doesn't double up. Routes Text / Key
    // events through `state.canvas.code_buffer.editor` when the
    // editor body has been clicked into focus.
    reg.add_scoped(Universal, CodeEditorService);
    // Editor file commands — Ctrl+N/O/S/Shift+S/W and Ctrl+Tab /
    // Ctrl+Shift+Tab for multi-file tabs. Registers *after*
    // `CodeEditorService` so the editor's own key handling sees
    // every keystroke first; the file service only fires on the
    // chord-style command shortcuts that the editor itself doesn't
    // claim.
    reg.add_scoped(Universal, EditorFilesService);
    // §25 — `CommandPaletteService` + `SearchService` keep their
    // command rows (palette.open / search.next / …) but their event
    // handling moved to declarations on `DeclarativeTextInputService`.
    // The single declarative service handles every modal-overlay
    // text input through one slice of `TextInputDeclaration`s — the
    // §25 modal-capture invariant is now expressed declaratively
    // (`TextInputDeclaration::modal_capture`) rather than per-service.
    reg.add_scoped(Universal, CommandPaletteService);
    reg.add_scoped(Universal, SearchService);
    reg.add_scoped(
        Universal,
        text_input::DeclarativeTextInputService::with_declarations(
            text_input::builtin_declarations(),
        ),
    );
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
    // §27 — cross-service services. `Help` + `Menu` are universal
    // (every app surfaces them); `Signals` + `Luau` are app-scoped
    // (signal-connections / .luau-script-bearing apps).
    reg.add_scoped(Universal, HelpService::default());
    reg.add_scoped(Universal, MenuService);
    reg.add_scoped(App, SignalsService);
    reg.add_scoped(App, LuauService);
    // IDE-mode Phase 4 — DevTools / Inspector. Contributes the
    // `devtools.{show-*, clear-probes}` commands. Event routing for
    // the filter field is declarative (see
    // `text_input::builtin_declarations`); tab clicks route through
    // the pointer-route table in `events.rs`.
    reg.add_scoped(Universal, DevToolsService);
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

    // ── ADR-010 — service factories ──────────────────────────────

    /// Tiny `ShellService` impl whose construction we can count.
    /// The factory closure captures an `AtomicUsize` and increments
    /// it; this struct just satisfies the `ShellService` shape.
    struct Counted {
        id: &'static str,
    }
    impl ShellService for Counted {
        fn id(&self) -> &'static str {
            self.id
        }
    }

    #[test]
    fn add_factory_scoped_runs_factory_immediately_and_lands_in_registry() {
        let mut reg = ServiceRegistry::new();
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let cc = Arc::clone(&call_count);
        reg.add_factory_scoped(
            ServiceScope::App,
            Box::new(move |_ctx| {
                cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Arc::new(Counted { id: "test.factory" })
            }),
        );
        // Factory ran exactly once at registration.
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        // The instance is present in the registry.
        assert!(reg.get("test.factory").is_some());
        // Scope is recorded.
        assert_eq!(reg.scope_of("test.factory"), Some(ServiceScope::App));
    }

    #[test]
    fn rebuild_app_services_reruns_factories_with_new_context() {
        let mut reg = ServiceRegistry::new();
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        // Record every `ctx.app_id` the factory observed, so the
        // test can assert the rebuild path actually flowed a fresh
        // context through.
        let seen_app_ids: Arc<std::sync::Mutex<Vec<Option<String>>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let cc = Arc::clone(&call_count);
        let seen = Arc::clone(&seen_app_ids);
        reg.add_factory_scoped(
            ServiceScope::App,
            Box::new(move |ctx| {
                cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                seen.lock().unwrap().push(ctx.app_id.map(str::to_string));
                Arc::new(Counted { id: "test.context" })
            }),
        );
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(seen_app_ids.lock().unwrap().as_slice(), &[None]);

        // Rebuild with a populated app_id; factory runs again.
        reg.rebuild_app_services(&ServiceContext {
            app_id: Some("lattice"),
        });
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(
            seen_app_ids.lock().unwrap().as_slice(),
            &[None, Some("lattice".to_string())],
        );
        assert!(reg.get("test.context").is_some());
    }

    #[test]
    fn rebuild_app_services_skips_eager_services() {
        // Services registered via the eager `add_scoped` path have no
        // factory; rebuild must leave them untouched.
        struct Fixed;
        impl ShellService for Fixed {
            fn id(&self) -> &'static str {
                "test.fixed"
            }
        }
        let mut reg = ServiceRegistry::new();
        reg.add_scoped(ServiceScope::App, Fixed);
        let before_len = reg.len();
        let before_ptr = Arc::as_ptr(&reg.get("test.fixed").unwrap());
        reg.rebuild_app_services(&ServiceContext::default());
        let after_len = reg.len();
        let after_ptr = Arc::as_ptr(&reg.get("test.fixed").unwrap());
        assert_eq!(before_len, after_len);
        assert!(
            std::ptr::eq(before_ptr, after_ptr),
            "rebuild must leave eager services untouched"
        );
    }

    #[test]
    fn rebuild_app_services_skips_universal_services() {
        // Universal services should never be rebuilt — they're the
        // shell chrome, not app concerns.
        let mut reg = ServiceRegistry::new();
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let cc = Arc::clone(&call_count);
        reg.add_factory_scoped(
            ServiceScope::Universal,
            Box::new(move |_| {
                cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Arc::new(Counted {
                    id: "test.universal",
                })
            }),
        );
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        reg.rebuild_app_services(&ServiceContext::default());
        // Universal-scoped factory only ran once (during initial
        // registration), even though the registry's app-services
        // rebuild was triggered.
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn rebuild_app_services_swaps_command_table_entries() {
        // Services that emit commands need their CommandTable rows
        // to track factory rebuilds. Otherwise the command palette
        // surfaces stale handlers.
        struct V1;
        impl ShellService for V1 {
            fn id(&self) -> &'static str {
                "test.cmds"
            }
            fn commands(&self) -> Vec<CommandSpec> {
                vec![CommandSpec {
                    id: "test.cmd.v1",
                    label: "V1",
                    category: "Test",
                    shortcut: None,
                    handler: Arc::new(|_| {}),
                }]
            }
        }
        struct V2;
        impl ShellService for V2 {
            fn id(&self) -> &'static str {
                "test.cmds"
            }
            fn commands(&self) -> Vec<CommandSpec> {
                vec![CommandSpec {
                    id: "test.cmd.v2",
                    label: "V2",
                    category: "Test",
                    shortcut: None,
                    handler: Arc::new(|_| {}),
                }]
            }
        }
        let mut reg = ServiceRegistry::new();
        let toggle = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let tg = Arc::clone(&toggle);
        reg.add_factory_scoped(
            ServiceScope::App,
            Box::new(move |_| {
                if tg.load(std::sync::atomic::Ordering::SeqCst) {
                    Arc::new(V2)
                } else {
                    Arc::new(V1)
                }
            }),
        );
        // V1 command present, V2 absent.
        assert!(reg.commands().get("test.cmd.v1").is_some());
        assert!(reg.commands().get("test.cmd.v2").is_none());

        // Flip the toggle and rebuild — V1 command drops, V2 surfaces.
        toggle.store(true, std::sync::atomic::Ordering::SeqCst);
        reg.rebuild_app_services(&ServiceContext::default());
        assert!(reg.commands().get("test.cmd.v1").is_none());
        assert!(reg.commands().get("test.cmd.v2").is_some());
    }

    #[test]
    #[should_panic(expected = "factory rebuild must preserve service id")]
    fn factory_changing_service_id_between_rebuilds_panics() {
        // ADR-010 invariant: a factory must return the same id every
        // time it runs. Otherwise the `by_id` lookup table grows
        // ghost entries and cross-service reach breaks.
        let mut reg = ServiceRegistry::new();
        let toggle = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let tg = Arc::clone(&toggle);
        struct A;
        impl ShellService for A {
            fn id(&self) -> &'static str {
                "test.id-a"
            }
        }
        struct B;
        impl ShellService for B {
            fn id(&self) -> &'static str {
                "test.id-b"
            }
        }
        reg.add_factory_scoped(
            ServiceScope::App,
            Box::new(move |_| {
                if tg.load(std::sync::atomic::Ordering::SeqCst) {
                    Arc::new(B) as Arc<dyn ShellService>
                } else {
                    Arc::new(A) as Arc<dyn ShellService>
                }
            }),
        );
        toggle.store(true, std::sync::atomic::Ordering::SeqCst);
        reg.rebuild_app_services(&ServiceContext::default());
    }

    #[test]
    fn eager_add_paths_remain_byte_compatible() {
        // ADR-010 backwards-compat pin: existing `add` / `add_scoped`
        // call sites must keep working identically. Both delegate
        // through `install` with `factory: None`, so the only
        // observable change is that `rebuild_app_services` skips them.
        struct One;
        impl ShellService for One {
            fn id(&self) -> &'static str {
                "test.one"
            }
        }
        struct Two;
        impl ShellService for Two {
            fn id(&self) -> &'static str {
                "test.two"
            }
        }
        let mut reg = ServiceRegistry::new();
        reg.add(One);
        reg.add_scoped(ServiceScope::App, Two);
        assert_eq!(reg.scope_of("test.one"), Some(ServiceScope::Universal));
        assert_eq!(reg.scope_of("test.two"), Some(ServiceScope::App));
        assert!(reg.get("test.one").is_some());
        assert!(reg.get("test.two").is_some());
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
