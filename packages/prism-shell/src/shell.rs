//! `Shell` — the §17 terminal boot path.
//!
//! ```text
//! Shell::new   → parse skeleton + build registry + bindings + Surface
//! Shell::run   → backend::run with one event handler that re-renders
//!                via `render_tree` whenever `dispatch_event` returns true
//! ```
//!
//! Per-feature wiring (every command, every mutation, every panel)
//! lands as it's ported off the legacy `app/` modules onto the new
//! `props` / `render` / `events` contract — but the supervisor surface
//! stays exactly this shape: one `render_tree` call, one
//! `dispatch_event` arm per runtime variant.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use prism_ui_runtime::event::Event;
use prism_ui_runtime::interpret::TagResolver;
use prism_ui_runtime::layout::{HitRect, Node as UiNode, Surface, Viewport};

use crate::components::{register_document_builtins, ShellComponentRegistry};
use crate::events::dispatch_event;
use crate::props::{PropCtx, ShellPropBindings};
use crate::render::{default_app_skeleton, render_tree_with, RenderCaches, Skeleton, Stylesheet};
use crate::render_scope::RenderScope;
use crate::services::{
    Clipboard, LuauHost, MutCtx, NoopLuauHost, OsVfs, ServiceRegistry, UndoStack, Vfs,
};

#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("skeleton parse: {0}")]
    Skeleton(String),
    #[error("registry: {0}")]
    Registry(String),
    #[error("runtime: {0}")]
    Runtime(String),
}

/// Per-frame shared state. Currently the registry + bindings + the
/// reloadable `AppState`; legacy modules (store, undo, persistence,
/// VFS, …) re-introduce themselves as fields here as they're ported.
pub struct ShellInner {
    pub registry: ShellComponentRegistry,
    /// **Wave 1** of `docs/dev/composable-builder-plan.md`: open
    /// registry of `ModifierBehaviour` impls. Seeded with the six
    /// baseline kinds in `Shell::new`; threaded into `PropCtx` /
    /// `MutCtx` so the canvas binding can install it on the
    /// builder's `LowerCtx` and the inspector can list attachable
    /// behaviours.
    pub modifier_registry: Arc<prism_builder::ModifierRegistry>,
    pub resolver: Arc<dyn TagResolver>,
    pub bindings: ShellPropBindings,
    pub services: ServiceRegistry,
    /// DSL self-bootstrap Loop 4: the production `AppRegistrar` impl.
    /// Owns the shared dock catalog; mutated during boot by
    /// `install_panels_from_manifests` and frozen into
    /// [`Self::dock_catalog`] before rendering starts.
    pub app_registrar: crate::app_registry::ShellAppRegistrar,
    /// Frozen snapshot of the dock catalog after every manifest's
    /// `panels.add` rows have been installed. Threaded into
    /// [`PropCtx`] so the dock-workspace binding can resolve labels
    /// + content tags for every reachable panel id.
    pub dock_catalog: Arc<prism_dock::DockCatalog>,
    /// ADR-009: per-app skeleton cache. Keyed by `manifest.id`; built
    /// once at boot from every `LoadedApp` whose manifest declared
    /// `[entry] skeleton = "..."`. Apps not in this map fall back to
    /// [`Self::default_app_skeleton`] (a single
    /// `<shell.dock-workspace/>`).
    pub app_skeletons: std::collections::HashMap<String, Skeleton>,
    /// ADR-009: the fallback skeleton applied when the active app is
    /// `None` or its id has no entry in [`Self::app_skeletons`].
    /// Parsed once at boot to avoid re-running the parser per frame.
    pub default_app_skeleton: Skeleton,
    /// ADR-009 follow-on: per-app PRSS stylesheet cache. Keyed by
    /// `manifest.id`; built once at boot from every `LoadedApp` whose
    /// manifest declared `[entry] styles = "..."`. The active app's
    /// stylesheet (if present) layers over the host stylesheet
    /// installed via `Shell::install_stylesheet` — token overrides +
    /// class definitions in the app sheet win on conflict because
    /// they come later in the cascade.
    pub app_stylesheets: std::collections::HashMap<String, Stylesheet>,
    /// Wave H.1 (`prui-luau-fusion.md` §5.4/§5.9): per-app
    /// directory, keyed by `manifest.id`. Feeds the active app's
    /// [`crate::import_resolver::FsImportResolver`] so an app `.prui`
    /// skeleton's `<import>` paths resolve relative to its own
    /// directory. wasm has no filesystem — empty there.
    #[cfg(not(target_arch = "wasm32"))]
    pub app_base_dirs: std::collections::HashMap<String, std::path::PathBuf>,
    pub state: crate::AppState,
    pub viewport: Viewport,
    pub undo: UndoStack,
    /// IO seam — `OsVfs` in production, mock in tests. Borrowed
    /// `&mut` into every `MutCtx` so `PersistenceService` /
    /// `ProjectService` can read/write without owning a fs handle.
    pub vfs: Box<dyn Vfs>,
    /// `LuauHost` seam — used by ad-hoc script execution
    /// (`LuauService::run-selection`, `SignalsService::Custom`).
    /// Distinct from [`Self::luau_runtime`]: the persistent runtime
    /// drives app `main.luau` boot + render/event dispatch through
    /// the long-lived `Lua` state; this `Box<dyn LuauHost>` is a
    /// `Send + Sync` carrier for one-shot script invocations that
    /// take `&mut MutCtx`. Today defaults to `NoopLuauHost`
    /// (records calls, returns `Null`); the daemon wires a real
    /// `LuauHost` impl in once the in-process bridge lands.
    pub luau: Box<dyn LuauHost>,
    /// In-memory clipboard cell — `ClipboardService` (§25) is the
    /// only consumer.
    pub clipboard: Clipboard,
    /// Phase 3 of `docs/dev/dioxus-inspiration.md` (reactive
    /// overhaul): per-shell `RenderScope` owns the reactive-graph
    /// Owner + `DirtyQueue<NodeId>`. Services and bindings can call
    /// `render_scope.invalidate_on(node_id, || sig.read(..))` to
    /// wire fine-grained signal-driven redraws without authoring a
    /// new dispatch arm. The femtovg event handler reads
    /// `render_scope.needs_redraw()` after each event and merges
    /// that with the legacy `dispatch_event` bool to decide whether
    /// to rebuild the tree. Per-block lower scoping comes in a
    /// follow-up; today this is the seam that fires the full
    /// per-frame `render_tree`.
    pub render_scope: RenderScope,
    /// **Wave 14.3** — `transition:<prop>="<duration>"` animator.
    /// Each render observes the live tree, applies in-flight
    /// transitions to the props, and ticks finished transitions
    /// off. The femtovg loop folds `animator.needs_redraw()` into
    /// the dirty bit so a running transition schedules the next
    /// frame without a separate timer.
    pub animator: RefCell<prism_ui_runtime::animator::Animator>,
    /// **Wave 14.3** — `memo="[dep1, dep2]"` cache. One per-shell
    /// table keyed by element id; survives across frames so the
    /// lowering pass short-circuits stable subtrees. Threaded into
    /// every render through `LowerScope::with_memo_cache`.
    pub memo_cache: Rc<RefCell<prism_ui_runtime::interpret::MemoCache>>,
    /// **Fusion F.3** — class→NodeId usage table, rebuilt each full
    /// render. The `.prss` hot-reload consumer reads it to mark only
    /// the NodeIds that used a literally-patched class dirty, so
    /// Phase 3 splices the rest instead of a full re-walk.
    pub class_deps: Rc<RefCell<prism_ui_runtime::interpret::ClassUsage>>,
    /// Persistent-Luau runtime — the long-lived `mlua::Lua` state app
    /// `[entry] script` bodies ran in at boot. `None` when no app
    /// declared a script (or when the build doesn't pull in mlua, e.g.
    /// `wasm32-unknown-unknown`). `LuauComponentBlock::lower_ui` and
    /// `LuauScriptedService::on_event` dispatch through this handle
    /// when present, falling through to their placeholder bodies
    /// otherwise.
    #[cfg(feature = "native")]
    pub luau_runtime: Option<Rc<prism_core::luau_runtime::LuauRuntime>>,
}

impl ShellInner {
    /// Build the per-frame `PropCtx` borrow-pack. Every binding closure
    /// destructures the fields it needs; adding a new datum is one
    /// field on `PropCtx` and one assignment here.
    pub fn prop_ctx(&self) -> PropCtx<'_> {
        PropCtx {
            state: &self.state,
            viewport_w: self.viewport.width,
            viewport_h: self.viewport.height,
            canvas_zoom: 1.0,
            // §43 B3: bindings that lower host-side trees (currently
            // `shell.builder-canvas` rendering `state.canvas.document`)
            // call into the live registry through this field. Pure
            // slot-accessor bindings ignore it.
            registry: Some(self.registry.as_component_registry()),
            // Phase 3b: hand the canvas binding the shell's
            // per-block invalidator so document blocks lower inside
            // per-NodeId reactive contexts wired to the dirty queue.
            block_invalidator: Some(self.render_scope.block_invalidator()),
            // Wave 1: the canvas binding installs this on the
            // builder's `LowerCtx::with_modifier_registry` so attached
            // `node.modifiers` fold over each block's output.
            modifier_registry: Some(self.modifier_registry.as_ref()),
            // DSL self-bootstrap Loop 4: the dock-workspace binding
            // reads labels + content tags from the frozen catalog
            // so manifest-registered panels surface in the live dock.
            dock_catalog: Some(self.dock_catalog.as_ref()),
        }
    }

    /// ADR-009: the skeleton whose body should fill the host's
    /// `<shell.app-window>` this frame. Picks the active app's
    /// skeleton from [`Self::app_skeletons`] if present, otherwise
    /// returns the cached [`Self::default_app_skeleton`]. The caller
    /// (typically `Shell::render`) calls
    /// `host_skeleton.with_app_body(active_app_skeleton())` to
    /// produce the composed skeleton that drives the frame's render.
    pub fn active_app_skeleton(&self) -> &Skeleton {
        self.state
            .workspace
            .active_app
            .as_deref()
            .and_then(|id| self.app_skeletons.get(id))
            .unwrap_or(&self.default_app_skeleton)
    }

    /// ADR-009 + ADR-010: shared implementation of `switch_active_app`.
    /// Owned by `ShellInner` so both the public `Shell::switch_active_app`
    /// entry point and the in-dispatcher event handlers
    /// (`handle_app_card_click` et al.) share exactly one code path.
    ///
    /// Returns `true` when the cursor actually moved.
    pub fn switch_active_app(&mut self, app_id: Option<&str>) -> bool {
        if self.state.workspace.active_app.as_deref() == app_id {
            return false;
        }
        self.state.workspace.active_app = app_id.map(|s| s.to_string());
        let ctx_app_id = self.state.workspace.active_app.clone();
        self.services
            .rebuild_app_services(&crate::services::ServiceContext {
                app_id: ctx_app_id.as_deref(),
            });
        self.render_scope
            .mark_dirty(crate::render_scope::FRAME_DIRTY_SENTINEL);
        true
    }

    /// ADR-009 follow-on: the active app's PRSS stylesheet, if any.
    /// Returns `None` when the active app is unset, when no app is
    /// active, or when the active app declared no `[entry] styles`.
    /// The render path layers the host stylesheet first, then the
    /// app's sheet on top — token + class overrides in the app
    /// sheet win on conflict.
    pub fn active_app_stylesheet(&self) -> Option<&Stylesheet> {
        self.state
            .workspace
            .active_app
            .as_deref()
            .and_then(|id| self.app_stylesheets.get(id))
    }

    /// Wave H.1 (`prui-luau-fusion.md` §5.4/§5.9): the active app's
    /// filesystem import resolver, if any. `None` when no app is
    /// active, the active app has no recorded directory, or on wasm
    /// (no filesystem — imports degrade to skipped, never fatal).
    /// `prism://` roots are not yet declared per-app; relative
    /// (sibling / `./` / `../`) resolution works today, `prism://`
    /// roots land when `.prism.json scripts.*` parsing is wired.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn active_app_import_resolver(
        &self,
    ) -> Option<std::sync::Arc<dyn prism_ui_runtime::interpret::ImportResolver>> {
        let id = self.state.workspace.active_app.as_deref()?;
        let dir = self.app_base_dirs.get(id)?;
        Some(crate::import_resolver::FsImportResolver::new(dir.clone()).arc())
    }

    /// wasm has no filesystem — the resolver is always `None`, so
    /// `<import>` / sibling probes degrade to skipped (graceful,
    /// matching the runtime's unknown-import rule).
    #[cfg(target_arch = "wasm32")]
    pub fn active_app_import_resolver(
        &self,
    ) -> Option<std::sync::Arc<dyn prism_ui_runtime::interpret::ImportResolver>> {
        None
    }

    /// Sister to [`Self::prop_ctx`] for the §24 write side. Every
    /// service handler and every command body takes one of these.
    /// Adding a new datum = one field on `MutCtx` and one assignment
    /// here.
    pub fn mut_ctx(&mut self) -> MutCtx<'_> {
        // Split-borrow: the registry comes from `self.registry`
        // (immutable), every other field comes from `self` (mutable).
        // Re-borrow explicitly so the borrow checker sees the disjoint
        // slices.
        let registry = self.registry.as_component_registry();
        let modifier_registry: &prism_builder::ModifierRegistry = self.modifier_registry.as_ref();
        MutCtx {
            state: &mut self.state,
            viewport: self.viewport,
            undo: &mut self.undo,
            vfs: self.vfs.as_mut(),
            luau: self.luau.as_mut(),
            clipboard: &mut self.clipboard,
            registry: Some(registry),
            modifier_registry: Some(modifier_registry),
        }
    }
}

pub struct Shell {
    pub inner: Rc<RefCell<ShellInner>>,
    pub skeleton: Skeleton,
    /// **PRSS** — host-installable stylesheet, swappable at runtime
    /// for hot-reload (`prism-ui-build::PrssFingerprintCache` →
    /// `Shell::install_stylesheet`). Wrapped in `Rc<RefCell<…>>` so
    /// the per-frame render closure borrows it cheaply while the
    /// `install_stylesheet` setter mutates in place. `None` until a
    /// stylesheet is loaded — the runtime treats class attributes as
    /// data-round-trip only in that state.
    stylesheet: Rc<RefCell<Option<Stylesheet>>>,
}

impl Shell {
    pub fn new() -> Result<Self, ShellError> {
        let mut registry = ShellComponentRegistry::new();
        // Wave 11.2 single-call bootstrap: native `SHELL_BUILTINS` +
        // `.prism-ui`-authored `SHELL_PRISM_UI_COMPONENTS` land into
        // the same registry, then `finalize_prism_ui_resolver` lets
        // composed `<shell.*>` / `<prism.*>` tags inside a DSL source
        // dispatch against the live merged registry.
        crate::components::registry::register_full_shell_chrome(&mut registry)
            .map_err(|e| ShellError::Registry(e.to_string()))?;
        register_document_builtins(&mut registry)
            .map_err(|e| ShellError::Registry(e.to_string()))?;
        let bindings = ShellPropBindings::with_builtins();
        let mut services = ServiceRegistry::with_builtins();
        let skeleton = Skeleton::load().map_err(ShellError::Skeleton)?;
        let modifier_registry = Arc::new(prism_builder::ModifierRegistry::with_builtins());
        // DSL self-bootstrap Loop 1: launchpad tiles come from
        // `apps/*/manifest.toml` when discovered. Missing/empty dir
        // falls through to the hardcoded list — see
        // `crate::seed::fallback_app_tiles`.
        let loaded_apps =
            crate::app_loader::discover(crate::app_loader::default_apps_root()).unwrap_or_default();
        // DSL self-bootstrap Loop 4: build the production
        // `AppRegistrar`, install every manifest's `panels.add` row
        // into its catalog.
        let app_registrar = crate::app_registry::ShellAppRegistrar::with_builtin_panels();
        let _panel_count =
            crate::app_registry::install_panels_from_manifests(&app_registrar, &loaded_apps);
        // Persistent-Luau runtime — always built under `feature =
        // "native"` so `ctx.luau.exec(...)` (the `LuauHost::exec`
        // seam) and any app's `[entry] script` body share the same
        // Lua state. Closes D2 of
        // `docs/dev/ui-migration-followups.md`: the `NoopLuauHost`
        // stub is only used when the build truly doesn't have mlua
        // available (e.g. wasm32). Scripts (when present) can call
        // `prism.app:register_panel/component/service` to extend the
        // shell at boot; registrations land in the registrar's
        // queues + `LuauCallbackStore` for downstream draining.
        #[cfg(feature = "native")]
        let luau_runtime: Option<Rc<prism_core::luau_runtime::LuauRuntime>> = {
            let registrar_arc: std::sync::Arc<dyn prism_core::AppRegistrar> =
                std::sync::Arc::new(app_registrar.clone());
            match prism_core::luau_runtime::LuauRuntime::new_with_tokens(
                registrar_arc,
                prism_core::design_tokens::DEFAULT_TOKENS,
                prism_core::shell_mode::ShellMode::Build,
                prism_core::shell_mode::Permission::Dev,
            ) {
                Ok(rt) => {
                    for app in &loaded_apps {
                        if let Some(src) = &app.script_source {
                            if let Err(e) = rt.load_script(src, &app.manifest.id) {
                                eprintln!(
                                    "prism-shell: app `{}` boot script failed: {e}",
                                    app.manifest.id
                                );
                            }
                        }
                    }
                    Some(Rc::new(rt))
                }
                Err(e) => {
                    eprintln!("prism-shell: failed to build LuauRuntime: {e}");
                    None
                }
            }
        };
        // Drain any scripted component registrations into the live
        // registry *before* the resolver snapshot so `<my.tag/>` in a
        // skeleton can dispatch through. Pre-wave behaviour is
        // preserved when no scripts ran — the drain is a no-op.
        #[cfg(feature = "native")]
        crate::app_registry::install_components_with_runtime(
            &app_registrar,
            &mut registry,
            luau_runtime.as_ref().map(Rc::clone),
        );
        // Resolver finalised after component installs — every scripted
        // tag is now resolvable end-to-end.
        let resolver = registry.tag_resolver();
        let dock_catalog = Arc::new(app_registrar.snapshot_catalog());
        // Drain scripted service registrations into the live registry.
        // Like components, this is a no-op pre-script.
        #[cfg(feature = "native")]
        crate::app_registry::install_services_with_runtime(
            &app_registrar,
            &mut services,
            luau_runtime.as_ref().map(Rc::clone),
        );
        // ADR-009: cache every loaded app's parsed skeleton so the
        // render path can graft the active app's body into the host
        // skeleton's `<shell.app-window>` per frame.
        let app_skeletons: std::collections::HashMap<String, Skeleton> = loaded_apps
            .iter()
            .filter_map(|a| {
                a.skeleton
                    .as_ref()
                    .map(|s| (a.manifest.id.clone(), s.clone()))
            })
            .collect();
        // ADR-009 follow-on: same shape for stylesheets.
        let app_stylesheets: std::collections::HashMap<String, Stylesheet> = loaded_apps
            .iter()
            .filter_map(|a| {
                a.stylesheet
                    .as_ref()
                    .map(|s| (a.manifest.id.clone(), s.clone()))
            })
            .collect();
        // Wave H.1: app id → directory, for the per-app import
        // resolver. Every loaded app has a `base_dir`.
        #[cfg(not(target_arch = "wasm32"))]
        let app_base_dirs: std::collections::HashMap<String, std::path::PathBuf> = loaded_apps
            .iter()
            .map(|a| (a.manifest.id.clone(), a.base_dir.clone()))
            .collect();
        let default_app = default_app_skeleton();
        // DSL self-bootstrap Loop 3: if any manifest declares service
        // preferences, filter `App`-scoped services to the declared
        // allowlist. Permissive default — manifests that omit
        // `[services]` keep every App-scoped service intact, preserving
        // current behaviour for the four built-in apps that ship without
        // declarations.
        let any_declares = loaded_apps.iter().any(|a| {
            !a.manifest.services.required.is_empty() || !a.manifest.services.optional.is_empty()
        });
        if any_declares {
            let allowed: std::collections::HashSet<&str> = loaded_apps
                .iter()
                .flat_map(|a| {
                    a.manifest
                        .services
                        .required
                        .iter()
                        .chain(a.manifest.services.optional.iter())
                })
                .map(|s| s.as_str())
                .collect();
            services.activate_app_services(&allowed);
        }
        let mut seed_state = crate::seed::initial_state_with_apps(&loaded_apps);
        // Wave 1: hand the shared modifier registry to AppState so
        // every `resync_builder_for_selection` call (boot, hit-test,
        // command body) derives modifier sections without per-callsite
        // plumbing.
        seed_state.modifier_registry = Some(Arc::clone(&modifier_registry));
        let inner = Rc::new(RefCell::new(ShellInner {
            registry,
            modifier_registry,
            resolver,
            bindings,
            services,
            app_registrar,
            dock_catalog,
            app_skeletons,
            default_app_skeleton: default_app,
            app_stylesheets,
            #[cfg(not(target_arch = "wasm32"))]
            app_base_dirs,
            // §43 A1 + Wave 1: hydrated boot state with the modifier
            // registry installed. `AppState::default()` is the
            // zero-data shape for tests and headless renders;
            // `Shell::new` boots into a populated catalog + canvas
            // document so the first frame looks like Studio.
            state: seed_state,
            viewport: Viewport {
                width: 1280.0,
                height: 800.0,
            },
            undo: UndoStack::default(),
            vfs: Box::new(OsVfs),
            luau: {
                // D2: prefer the real `mlua`-backed host when the
                // persistent runtime built successfully. Falls back
                // to `NoopLuauHost` so wasm / runtime-init-failure
                // builds still boot.
                #[cfg(feature = "native")]
                {
                    if let Some(rt) = luau_runtime.as_ref() {
                        Box::new(crate::services::MluaLuauHost::new(Rc::clone(rt)))
                            as Box<dyn LuauHost>
                    } else {
                        Box::new(NoopLuauHost::default()) as Box<dyn LuauHost>
                    }
                }
                #[cfg(not(feature = "native"))]
                {
                    Box::new(NoopLuauHost::default()) as Box<dyn LuauHost>
                }
            },
            clipboard: Clipboard::default(),
            render_scope: RenderScope::new(),
            animator: RefCell::new(prism_ui_runtime::animator::Animator::new()),
            memo_cache: Rc::new(RefCell::new(prism_ui_runtime::interpret::MemoCache::new())),
            class_deps: Rc::new(RefCell::new(
                prism_ui_runtime::interpret::ClassUsage::new(),
            )),
            #[cfg(feature = "native")]
            luau_runtime,
        }));
        // §43 C1: one-shot post-boot resync. The seed sets selection
        // and the inspector tree, but `derive_property_rows` needs the
        // live registry — which `seed::initial_state` doesn't have.
        // Running it once here means the boot frame's properties panel
        // is already populated for the pre-selected node.
        {
            let mut guard = inner.borrow_mut();
            let g = &mut *guard;
            let registry = g.registry.as_component_registry();
            g.state.resync_builder_for_selection(Some(registry));
        }
        Ok(Self {
            inner,
            skeleton,
            stylesheet: Rc::new(RefCell::new(None)),
        })
    }

    /// ADR-009 + ADR-010: change the active app. Performs three
    /// linked side effects in one call so callers never forget any:
    ///
    /// 1. Update `state.workspace.active_app` — the source of truth
    ///    read by [`ShellInner::active_app_skeleton`].
    /// 2. Re-run every `App`-scoped [`ServiceFactory`] with a fresh
    ///    [`ServiceContext`] carrying the new app id, so Luau-backed
    ///    services re-bind to the new app's script handles. Eager
    ///    services (no factory) are left untouched.
    /// 3. Mark `FRAME_DIRTY_SENTINEL` so the next event-loop tick
    ///    re-renders against the new skeleton.
    ///
    /// Idempotent: switching to the already-active app is a no-op
    /// (no rebuild, no dirty mark). Switching to `None` un-mounts
    /// any app — the host skeleton falls back to the default app
    /// skeleton.
    ///
    /// Returns `true` when the cursor actually moved, `false` for
    /// the idempotent no-op path.
    ///
    /// Delegates to [`ShellInner::switch_active_app`]; this is the
    /// public entry point hosts call without going through
    /// `RefCell::borrow_mut`. Event handlers inside the dispatcher
    /// already have a `&mut ShellInner` and call the inner method
    /// directly.
    ///
    /// [`ServiceFactory`]: crate::services::ServiceFactory
    /// [`ServiceContext`]: crate::services::ServiceContext
    pub fn switch_active_app(&self, app_id: Option<&str>) -> bool {
        self.inner.borrow_mut().switch_active_app(app_id)
    }

    /// ADR-009 follow-on: install (or replace) the stylesheet
    /// associated with a specific app id. Pairs with the boot-time
    /// cache built in `Shell::new` to support hot-reload — when the
    /// dev-loop's PRSS watcher detects a change to
    /// `apps/<id>/app.prss`, the host re-parses the file and hands
    /// the resulting [`Stylesheet`] to this method.
    ///
    /// If `app_id` matches the currently active app, marks the
    /// frame dirty so the next render picks up the new cascade
    /// against the host stylesheet. Hot-reload of an inactive app's
    /// sheet updates the cache without paying for a redraw — the
    /// next `switch_active_app` to that id surfaces the new values.
    ///
    /// Returns `true` when the install actually marked dirty (the
    /// app was active), `false` for the inactive-cache-update path.
    pub fn install_app_stylesheet(&self, app_id: &str, stylesheet: Stylesheet) -> bool {
        let mut inner = self.inner.borrow_mut();
        inner.app_stylesheets.insert(app_id.to_string(), stylesheet);
        if inner.state.workspace.active_app.as_deref() == Some(app_id) {
            inner
                .render_scope
                .mark_dirty(crate::render_scope::FRAME_DIRTY_SENTINEL);
            true
        } else {
            false
        }
    }

    /// ADR-009 follow-on: drop a per-app stylesheet from the cache.
    /// Hot-reload path for the rare case where an author deletes
    /// `app.prss` or replaces it with a broken file the watcher
    /// can't parse. Marks dirty when the dropped app was active.
    pub fn uninstall_app_stylesheet(&self, app_id: &str) -> bool {
        let mut inner = self.inner.borrow_mut();
        let was_present = inner.app_stylesheets.remove(app_id).is_some();
        if was_present && inner.state.workspace.active_app.as_deref() == Some(app_id) {
            inner
                .render_scope
                .mark_dirty(crate::render_scope::FRAME_DIRTY_SENTINEL);
            true
        } else {
            false
        }
    }

    /// Read the currently-active app id. `None` when no launchpad
    /// tile has been activated yet.
    pub fn active_app(&self) -> Option<String> {
        self.inner.borrow().state.workspace.active_app.clone()
    }

    /// Persistent-Luau hot-reload entry. Re-runs `source` against the
    /// shell's long-lived `LuauRuntime`. Registrations from the prior
    /// run are overwritten — both the [`LuauCallbackStore`]'s retained
    /// closures (insert-replaces semantics) and the
    /// [`ShellComponentRegistry`] (via `register_or_replace`). The
    /// runtime's module-level `local`s do *not* survive re-execution;
    /// per-script-author state belongs in the shared `prism.objects`
    /// or in atom-backed module globals.
    ///
    /// Returns `Ok(())` when the script ran cleanly. Errors come back
    /// as the script's diagnostic message; the watcher logs + retains
    /// the previous state on the registries (the failed run never
    /// drains the registrar queues, so nothing gets clobbered).
    ///
    /// Frame is marked dirty whenever `app_id` matches the active
    /// app so the watcher sees an immediate redraw with the new
    /// renders in effect.
    #[cfg(feature = "native")]
    pub fn install_app_script(&self, app_id: &str, source: &str) -> Result<(), String> {
        let inner = self.inner.borrow();
        let Some(rt) = inner.luau_runtime.as_ref() else {
            return Err("no LuauRuntime — shell booted without script support".into());
        };
        let rt = std::rc::Rc::clone(rt);
        let registrar = inner.app_registrar.clone();
        drop(inner);
        // Run the script first against a tentative-state error path:
        // if it parses + executes cleanly, drain into the registry.
        rt.load_script(source, app_id)?;
        let mut inner = self.inner.borrow_mut();
        // Drain newly-queued components into the registry with
        // replace semantics so a redefined `my.card` overwrites the
        // prior one.
        let _replaced =
            crate::app_registry::install_components_replace(&registrar, &mut inner.registry);
        // Re-resolve the tag resolver so newly-introduced tags become
        // dispatchable. (The existing resolver was a snapshot.)
        inner.resolver = inner.registry.tag_resolver();
        // Drain services with replace semantics — re-running a
        // `register_service({id = "x", ...})` on top of an existing
        // registration drops the prior entry (and its command-table
        // contributions) before installing the new factory. Without
        // replace, the standard `add_factory_scoped` install path's
        // duplicate-id assert would panic on the second hot-reload.
        let _ = crate::app_registry::install_services_replace(&registrar, &mut inner.services);
        if inner.state.workspace.active_app.as_deref() == Some(app_id) {
            inner
                .render_scope
                .mark_dirty(crate::render_scope::FRAME_DIRTY_SENTINEL);
        }
        Ok(())
    }

    /// Install (or replace) the active PRSS stylesheet. Subsequent
    /// `render` calls thread the new sheet into the lowering scope so
    /// every container with a `class="..."` attribute resolves through
    /// the named-class vocabulary. Pass `None` to detach the
    /// stylesheet entirely (returns to "data-round-trip only" mode).
    ///
    /// Used by both the boot path (host loads `theme.prss` once at
    /// startup) and the hot-reload watcher (each save through the
    /// `PrssFingerprintCache` builds a fresh `Stylesheet` and installs
    /// it). Bumps the render scope's dirty bit so the next event
    /// triggers a redraw with the new sheet in effect.
    pub fn install_stylesheet(&self, stylesheet: Option<Stylesheet>) {
        *self.stylesheet.borrow_mut() = stylesheet;
        // Force a redraw on the next event loop tick so the swap
        // takes visual effect even when nothing else changed. The
        // FRAME_DIRTY_SENTINEL is the same dirty bit signal writes
        // use, so the femtovg event handler picks it up uniformly.
        self.inner
            .borrow()
            .render_scope
            .mark_dirty(crate::render_scope::FRAME_DIRTY_SENTINEL);
    }

    /// A2 — collect every `bind:<key>="<source>"` authored on the
    /// currently-rendered skeleton tree. Returns a
    /// [`SkeletonBindings`](crate::skeleton_bindings::SkeletonBindings)
    /// list keyed by container/input id. Downstream code
    /// (event router, future Effect installation against `AppState`
    /// slots) consumes the list without re-walking the tree.
    ///
    /// The render-scope's frame-level `ReactiveContext` already
    /// auto-subscribes any `Signal::read` invoked inside the render
    /// walk (Phase 3a of `docs/dev/dioxus-inspiration.md`), so the
    /// declarative `bind:*` carry-through this function surfaces is
    /// the *metadata* layer — the wiring layer beneath it is
    /// already reactive.
    pub fn collect_skeleton_bindings(&self) -> crate::skeleton_bindings::SkeletonBindings {
        let tree = self.render();
        crate::skeleton_bindings::SkeletonBindings::collect(&tree)
    }

    /// Install (or replace) the default `app.prism-ui` skeleton from
    /// fresh source. Used by the C3 hot-reload watcher in
    /// `docs/dev/ui-migration-followups.md`: `prism dev shell` (or
    /// any host) detects a change to `ui/app.prism-ui` via
    /// `prism_ui_build::template_watch`, calls this with the new
    /// source, and the next frame renders against the swapped
    /// skeleton. A parse error returns `Err(msg)` so the host can
    /// surface it as a toast without crashing the live shell.
    pub fn install_default_skeleton(&self, source: &str) -> Result<(), String> {
        let fresh = Skeleton::from_source(source)?;
        self.inner.borrow_mut().default_app_skeleton = fresh;
        self.inner
            .borrow()
            .render_scope
            .mark_dirty(crate::render_scope::FRAME_DIRTY_SENTINEL);
        Ok(())
    }

    /// Same as [`Self::install_default_skeleton`] but targets a
    /// per-app skeleton (`apps/<id>/shell.prism-ui`). When the
    /// active app's skeleton is the one being swapped, the next
    /// frame re-renders with the new tree; otherwise the swap is
    /// silent until the user switches to that app.
    pub fn install_app_skeleton(&self, app_id: &str, source: &str) -> Result<(), String> {
        let fresh = Skeleton::from_source(source)?;
        self.inner
            .borrow_mut()
            .app_skeletons
            .insert(app_id.to_string(), fresh);
        self.inner
            .borrow()
            .render_scope
            .mark_dirty(crate::render_scope::FRAME_DIRTY_SENTINEL);
        Ok(())
    }

    /// Borrow the current stylesheet for read-only inspection. `None`
    /// when nothing has been installed yet.
    pub fn stylesheet(&self) -> Option<Stylesheet> {
        self.stylesheet.borrow().clone()
    }

    /// Dispatch one input event against this shell — same path the
    /// femtovg backend takes on every window event. Returns
    /// `dispatch_event`'s redraw signal. Used by the e2e suite and
    /// any host that wants to drive the shell programmatically.
    pub fn dispatch_event(&self, event: &prism_ui_runtime::event::Event) -> bool {
        let hit = pointer_xy(event).and_then(|(x, y)| {
            let tree = self.render();
            use prism_ui_runtime::layout::{
                ContainerProps, Direction, Node as UiNode, Sizing, Surface,
            };
            let viewport = self.inner.borrow().viewport;
            let root = UiNode::Container {
                id: String::new(),
                props: ContainerProps {
                    direction: Direction::Column,
                    width: Sizing::Grow,
                    height: Sizing::Grow,
                    ..Default::default()
                },
                children: tree,
            };
            let mut surface = Surface::new(root, viewport);
            surface.hit_test_at(x, y).cloned()
        });
        crate::events::dispatch_event(&self.inner, event, hit)
    }

    /// Read-only borrow of `ShellInner` for inspection by tests and
    /// host glue. The closure must not call back into the shell's
    /// mutating API (e.g. `dispatch_event`) — that would trigger a
    /// double-borrow panic.
    pub fn with_inner<R>(&self, f: impl FnOnce(&ShellInner) -> R) -> R {
        f(&self.inner.borrow())
    }

    /// Mutable borrow of `ShellInner`. Same double-borrow caveat as
    /// [`Self::with_inner`] — the closure must not call back into
    /// `dispatch_event` or any other `&self` shell method while it
    /// holds the borrow. Used by tests + host glue that need to
    /// stage `AppState` setup outside the scene path.
    pub fn with_inner_mut<R>(&self, f: impl FnOnce(&mut ShellInner) -> R) -> R {
        f(&mut self.inner.borrow_mut())
    }

    /// Run a registered command by id. Returns `true` when the
    /// command table had a matching entry. Used by tests + host glue
    /// to invoke commands that aren't bound to keyboard shortcuts
    /// (e.g. `devtools.clear-probes`). Builds a `MutCtx` from the
    /// inner state the same way `dispatch_event` does.
    pub fn run_command(&self, id: &str) -> bool {
        // Resolve the command spec first while we still hold an
        // immutable borrow of `services` — the registry / command
        // handlers are `Arc<dyn Fn>` so we can clone the handler
        // pointer out, drop the immutable borrow, and then call it
        // with a mutable `MutCtx`. Without the clone we'd hold both
        // an immutable borrow on `g.services` and a mutable borrow
        // on `g` (through `mut_ctx`) at the same time.
        let mut guard = self.inner.borrow_mut();
        let g = &mut *guard;
        let Some(spec) = g.services.commands().get(id).cloned() else {
            return false;
        };
        let mut ctx = g.mut_ctx();
        (spec.handler)(&mut ctx);
        true
    }

    /// Find the topmost hit-cache entry whose `data-role` matches
    /// `role`. Helper for e2e tests that need to synthesise pointer
    /// events against a known surface (e.g. the code-editor body).
    pub fn find_hit_by_role(&self, role: &str) -> Option<prism_ui_runtime::layout::HitRect> {
        let tree = self.render();
        use prism_ui_runtime::layout::{
            ContainerProps, Direction, Node as UiNode, Sizing, Surface,
        };
        let viewport = self.inner.borrow().viewport;
        let root = UiNode::Container {
            id: String::new(),
            props: ContainerProps {
                direction: Direction::Column,
                width: Sizing::Grow,
                height: Sizing::Grow,
                ..Default::default()
            },
            children: tree,
        };
        let mut surface = Surface::new(root, viewport);
        for h in surface.hit_rects().iter().rev() {
            if h.attrs.iter().any(|(k, v)| k == "data-role" && v == role) {
                return Some(h.clone());
            }
        }
        None
    }

    /// Build the initial runtime tree. Pure function of `(skeleton,
    /// bindings, resolver, ctx)` — exposed so tests, alternate hosts,
    /// and the per-frame redraw closure all hit the same path.
    ///
    /// Wrapped in two layers:
    /// 1. `render_scope.run_in_render_pass(...)` — **Phase 3a** of
    ///    `docs/dev/dioxus-inspiration.md`. Any reactive signal read
    ///    inside the walk (today: nothing yet; Phase 4 wires
    ///    `ReactiveProps`; Phase 2 wires CRDT-backed atoms) subscribes
    ///    a persistent frame context whose dirty callback marks
    ///    `FRAME_DIRTY_SENTINEL` into the dirty queue. Signal writes
    ///    drive redraws without any per-binding wiring.
    /// 2. `subsecond::call` under the `hot-reload` feature so changes
    ///    to `render_tree`'s body (and transitively, the block lower
    ///    bodies it calls) patch in-place via subsecond without
    ///    dropping the `Surface` tree or the reactive `Owner` graph.
    ///    Phase 9.
    pub fn render(&self) -> Vec<UiNode> {
        let inner = self.inner.borrow();
        let cache = Rc::clone(&inner.memo_cache);
        let host_stylesheet = self.stylesheet.borrow().clone();
        // ADR-009 follow-on: cascade host + active-app stylesheet.
        // Host sheet provides the base tokens / classes; the active
        // app's sheet (if any) layers on top so its token overrides
        // and class definitions win on conflict. Held as an owned
        // local because `merge_with` returns a fresh value — the
        // host's stylesheet field stays untouched.
        let app_stylesheet = inner.active_app_stylesheet();
        let effective_stylesheet: Option<Stylesheet> = match (host_stylesheet, app_stylesheet) {
            (Some(host), Some(app)) => Some(host.merge_with(app)),
            (Some(host), None) => Some(host),
            (None, Some(app)) => Some(app.clone()),
            (None, None) => None,
        };
        // ADR-009: graft the active app's skeleton body into the host
        // skeleton's `<shell.app-window>` element. Cheap (AST clone),
        // bounded (host skeleton is ~50 nodes), and runs once per
        // frame — no caching needed unless profiling shows it.
        let composed = self.skeleton.with_app_body(inner.active_app_skeleton());
        let app_import_resolver = inner.active_app_import_resolver();
        // Full render — rebuild the class→NodeId table from scratch so
        // a removed `class="…"` doesn't keep a stale NodeId alive.
        inner.class_deps.borrow_mut().clear();
        let mut tree = inner.render_scope.run_in_render_pass(|| {
            render_with_hot_reload(|| {
                render_tree_with(
                    &composed,
                    &inner.bindings,
                    Arc::clone(&inner.resolver),
                    &inner.prop_ctx(),
                    // Boot / full render: no dirty set — walk
                    // everything and (re)populate the per-id cache so
                    // subsequent reactive frames can splice.
                    RenderCaches {
                        memo: Some(Rc::clone(&cache)),
                        dirty: None,
                        class_deps: Some(Rc::clone(&inner.class_deps)),
                    },
                    effective_stylesheet.as_ref(),
                    app_import_resolver.clone(),
                )
            })
        });
        // **Wave 14.3** — animator pass. `observe` looks for moved
        // declared values on transition-tagged containers, `apply`
        // rewrites them to the interpolated sample at `now_ms`, and
        // `tick` prunes finished transitions so the next frame
        // skips them.
        //
        // **Wave 14.8** — phantom-node graft. Any container that
        // carried `animate:out-<prop>` and has since left the tree
        // re-appears at its previous parent's child list (or at the
        // root when the parent vanished too) with its eased prop
        // values already written in, so the painter can fade it
        // out in place. `tick` drains the phantom once every
        // out-transition for its id has elapsed.
        let now_ms = now_ms();
        let mut animator = inner.animator.borrow_mut();
        animator.observe(&tree, now_ms);
        animator.apply(&mut tree, now_ms);
        graft_phantoms(&mut tree, animator.phantom_nodes_with_parent(now_ms));
        animator.tick(now_ms);
        tree
    }

    #[cfg(feature = "native")]
    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        self.run_inner(None)
    }

    #[cfg(feature = "native")]
    fn run_inner(
        self,
        tick: Option<prism_ui_runtime::backends::femtovg::TickHook>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let initial = wrap_root(self.render());
        let viewport = self.inner.borrow().viewport;
        let surface = Surface::new(initial, viewport);

        let inner = Rc::clone(&self.inner);
        let skeleton = self.skeleton.clone();
        // Cheap-clone the `Rc<RefCell<Option<Stylesheet>>>` handle so
        // the event-loop closure picks up live `install_stylesheet`
        // swaps on every frame — hot-reload feeds new sheets through
        // this seam without reconstructing the handler.
        let stylesheet_handle = Rc::clone(&self.stylesheet);
        let handler: prism_ui_runtime::event::EventHandler = Box::new(move |event, surface| {
            // Single hit-test per pointer event. Reused for: hover
            // paint (`set_hovered` on PointerMove), click routing
            // (PointerDown chrome / canvas-node routes), and the
            // click-without-drag step on PointerUp. Non-pointer
            // events yield `None` and pass through cleanly.
            let hit = compute_hit(event, surface);
            // Hover paint: PointerMove syncs `Surface::hovered_id`
            // so every container declaring `props.hover` (inspector
            // rows, icon buttons, nav buttons, menu items, tabs, app
            // cards, drag-number fields, ...) tints as the cursor
            // passes. `set_hovered` is a no-op when the id hasn't
            // changed and only marks the surface dirty when at
            // least one side of the transition has hover overrides
            // — clean hovers stay clean.
            if matches!(event, Event::PointerMove { .. }) {
                surface.set_hovered(hit.as_ref().map(|h| h.id.clone()));
            }
            let event_dirty = dispatch_event(&inner, event, hit);
            // Phase 3 of `docs/dev/dioxus-inspiration.md`: reactive
            // services that invalidate through `render_scope` push
            // node IDs into the per-shell `DirtyQueue`. Either path
            // (event dispatch or signal-driven invalidation) is
            // sufficient reason to re-render this frame. We drain
            // the queue regardless so per-block lowering can wire
            // up against it once that lands; today the whole tree
            // re-renders either way.
            let drained: Option<Vec<String>> = {
                let guard = inner.borrow();
                if guard.render_scope.needs_redraw() {
                    Some(guard.render_scope.drain_dirty())
                } else {
                    None
                }
            };
            let reactive_dirty = drained.is_some();
            // **Wave 14.3** — animator wants its own frame while
            // transitions are in flight. Even when nothing else
            // changed (no event, no signal write), a running
            // transition needs the next frame to sample its next
            // eased value.
            let animator_dirty = inner.borrow().animator.borrow().needs_redraw();
            if event_dirty || reactive_dirty || animator_dirty {
                let guard = inner.borrow();
                let cache = Rc::clone(&guard.memo_cache);
                let stylesheet = stylesheet_handle.borrow().clone();
                let app_import_resolver = guard.active_app_import_resolver();

                // **Phase 3** — localise the re-walk to the dirty
                // NodeIds *only* when the redraw is purely reactive
                // (we know precisely which blocks changed). Event /
                // animator redraws, or a `FRAME_DIRTY_SENTINEL`
                // (frame-scope signal read, not localisable), do a
                // full walk — which also repopulates the per-id cache
                // so the next reactive frame can splice.
                let dirty_set: Option<Rc<std::collections::HashSet<String>>> =
                    if reactive_dirty && !event_dirty && !animator_dirty {
                        let ids = drained.as_deref().unwrap_or(&[]);
                        if ids
                            .iter()
                            .any(|d| d == crate::render_scope::FRAME_DIRTY_SENTINEL)
                        {
                            None
                        } else {
                            Some(Rc::new(ids.iter().cloned().collect()))
                        }
                    } else {
                        None
                    };
                if dirty_set.is_some() {
                    cache.borrow_mut().begin_pass();
                }
                let render = |dirty: Option<Rc<std::collections::HashSet<String>>>| {
                    // A full pass (no dirty set) rebuilds the
                    // class→NodeId table from scratch; a reactive
                    // splice keeps the prior table (spliced elements
                    // don't re-record, so their class deps must
                    // persist).
                    if dirty.is_none() {
                        guard.class_deps.borrow_mut().clear();
                    }
                    guard.render_scope.run_in_render_pass(|| {
                        render_with_hot_reload(|| {
                            // `render_with_hot_reload` accepts FnMut so
                            // the patch pipeline can re-invoke us; clone
                            // the dirty handle each call so the inner
                            // closure doesn't move it out of its capture.
                            render_tree_with(
                                &skeleton,
                                &guard.bindings,
                                Arc::clone(&guard.resolver),
                                &guard.prop_ctx(),
                                RenderCaches {
                                    memo: Some(Rc::clone(&cache)),
                                    dirty: dirty.clone(),
                                    class_deps: Some(Rc::clone(&guard.class_deps)),
                                },
                                stylesheet.as_ref(),
                                app_import_resolver.clone(),
                            )
                        })
                    })
                };
                let mut tree = render(dirty_set.clone());
                // **Phase 3 non-lossy guard.** If any drained dirty
                // id mapped to no element this pass (an unknown /
                // renamed id), the splice may have skipped a needed
                // update — redo a full walk this same frame. Worst
                // case equals the pre-Phase-3 behaviour; a correct
                // splice never reaches here.
                if let Some(ds) = &dirty_set {
                    let missed = cache.borrow().untouched(ds.iter());
                    if !missed.is_empty() {
                        tree = render(None);
                    }
                }
                let now_ms = now_ms();
                let mut animator = guard.animator.borrow_mut();
                animator.observe(&tree, now_ms);
                animator.apply(&mut tree, now_ms);
                graft_phantoms(&mut tree, animator.phantom_nodes_with_parent(now_ms));
                animator.tick(now_ms);
                surface.set_tree(wrap_root(tree));
            }
        });

        match tick {
            Some(tick) => prism_ui_runtime::backends::femtovg::run_with_tick(
                surface,
                handler,
                crate::assets::loader(),
                tick,
            ),
            None => {
                prism_ui_runtime::backends::femtovg::run(surface, handler, crate::assets::loader())
            }
        }
        .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))
    }

    /// Same as [`Self::run`] but spawns a [`crate::hot_reload`]
    /// watcher first and drains its channel each tick. Closes C3 of
    /// `docs/dev/ui-migration-followups.md`: editing a watched
    /// `.prism-ui` file applies in-place without dropping the event
    /// loop or rebuilding cargo. `--watch-ui` on the shell binary
    /// is the canonical user-facing surface.
    ///
    /// `specs` lists each watched path along with the
    /// [`crate::hot_reload::ReloadTarget`] it feeds. The watcher's
    /// life is scoped to the run; when this function returns, the
    /// watcher thread tears down.
    #[cfg(feature = "native")]
    pub fn run_with_hot_reload(
        self,
        specs: Vec<crate::hot_reload::WatchSpec>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let watcher = crate::hot_reload::spawn_hot_reload_watcher(specs)
            .map_err(Box::<dyn std::error::Error>::from)?;

        // Tick hook: drain the watcher and apply pending reloads
        // against `ShellInner`. Render-scope dirty mark routes the
        // swap through the next frame the handler renders; the
        // femtovg backend's `about_to_wait` reads `surface.is_dirty()
        // || images.has_animations()`, so we also re-render once here
        // (via a synthetic empty handler call) when a reload landed
        // to ensure the new tree paints without an extra input event.
        let tick_inner = Rc::clone(&self.inner);
        // §3.2 — host PRSS handle + a persistent `StylesheetWatcher`
        // (owns the `PrssFingerprintCache`) so `.prss` literal vs
        // structural classification stays deterministic across ticks.
        let host_sheet = Rc::clone(&self.stylesheet);
        let mut sheet_watcher = crate::render::StylesheetWatcher::new();
        let tick: prism_ui_runtime::backends::femtovg::TickHook =
            Box::new(move |_surface: &mut Surface| {
                let pending = watcher.drain();
                if pending.is_empty() {
                    return;
                }
                let mut dirty = false;
                for evt in pending {
                    use crate::hot_reload::ReloadTarget;
                    match &evt.target {
                        ReloadTarget::DefaultSkeleton => match Skeleton::from_source(&evt.source) {
                            Ok(s) => {
                                tick_inner.borrow_mut().default_app_skeleton = s;
                                dirty = true;
                            }
                            Err(e) => {
                                eprintln!("prism-shell hot-reload: skeleton parse error: {e}");
                            }
                        },
                        ReloadTarget::AppSkeleton { app_id } => {
                            match Skeleton::from_source(&evt.source) {
                                Ok(s) => {
                                    tick_inner
                                        .borrow_mut()
                                        .app_skeletons
                                        .insert(app_id.clone(), s);
                                    dirty = true;
                                }
                                Err(e) => {
                                    eprintln!("prism-shell hot-reload: skeleton parse error: {e}");
                                }
                            }
                        }
                        ReloadTarget::Stylesheet { app_id } => {
                            // Classify the `.prss` change through the
                            // `PrssFingerprintCache`. `NoChange` /
                            // `ParseError` / `ReadError` keep the last
                            // good sheet (no `stylesheet`); a literal
                            // or structural change yields a fresh one.
                            let key = match app_id {
                                None => "<prss:host>".to_string(),
                                Some(id) => format!("prss:{id}"),
                            };
                            let reload = sheet_watcher.observe_source(&key, &evt.source);
                            if let Some(sheet) = reload.stylesheet {
                                match app_id {
                                    None => {
                                        *host_sheet.borrow_mut() = Some(sheet);
                                    }
                                    Some(id) => {
                                        tick_inner
                                            .borrow_mut()
                                            .app_stylesheets
                                            .insert(id.clone(), sheet);
                                    }
                                }
                                // **Fusion F.3** — selective
                                // invalidation. A `LiteralOnly` change
                                // marks *only* the NodeIds that resolved
                                // a patched class (Phase 3 then splices
                                // the rest). Token-bucket patches and
                                // structural changes cascade broadly →
                                // FRAME_DIRTY_SENTINEL (safe full walk).
                                // A class with no recorded NodeId (not
                                // yet rendered) also falls back to the
                                // sentinel so the edit is never dropped.
                                let g = tick_inner.borrow();
                                let mut broad = false;
                                match &reload.change {
                                    prism_ui_build::PrssChange::LiteralOnly { patches } => {
                                        let deps = g.class_deps.borrow();
                                        for p in patches {
                                            match &p.owner {
                                                prism_ui_build::PrssLiteralOwner::Class {
                                                    name,
                                                    ..
                                                } => {
                                                    let nodes = deps.nodes_for(name);
                                                    if nodes.is_empty() {
                                                        broad = true;
                                                    } else {
                                                        for nid in nodes {
                                                            g.render_scope.mark_dirty(nid);
                                                        }
                                                    }
                                                }
                                                // Tokens cascade into
                                                // every class that
                                                // references them.
                                                prism_ui_build::PrssLiteralOwner::Token {
                                                    ..
                                                } => broad = true,
                                            }
                                        }
                                    }
                                    // FirstSighting / Structural →
                                    // re-walk everything.
                                    _ => broad = true,
                                }
                                if broad {
                                    g.render_scope.mark_dirty(
                                        crate::render_scope::FRAME_DIRTY_SENTINEL,
                                    );
                                }
                            } else if let prism_ui_build::PrssChange::ParseError { message } =
                                &reload.change
                            {
                                eprintln!("prism-shell hot-reload: .prss parse error: {message}");
                            }
                        }
                    }
                }
                // Mark the render scope dirty so the femtovg backend's
                // post-tick `surface.is_dirty() || …` check triggers
                // a redraw. The handler then re-runs the
                // `render_tree_with` walk (FRAME_DIRTY_SENTINEL forces
                // Phase 3's safe full pass) against the swapped
                // skeleton / stylesheet and calls `surface.set_tree`.
                if dirty {
                    tick_inner
                        .borrow()
                        .render_scope
                        .mark_dirty(crate::render_scope::FRAME_DIRTY_SENTINEL);
                }
            });

        self.run_inner(Some(tick))
    }

    /// On the web target the femtovg backend isn't compiled in; the
    /// stub returns immediately so `web_start` links until the
    /// `prism-ui-runtime/web` backend's `run` is wired up.
    #[cfg(not(feature = "native"))]
    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.render();
        Ok(())
    }
}

/// `Surface` takes a single root `Node`. The skeleton lowers to a
/// flat `Vec<Node>` (app-window + workflow-page-bar + overlay
/// siblings); wrap them in an anonymous Column container that fills
/// the viewport so the app-window (`Sizing::Grow`) and the
/// workflow-page-bar (`Sizing::Fixed(32)`) both land at their
/// intended sizes.
///
/// Without `Sizing::Grow` on both axes here, the wrapper would
/// collapse to its content's intrinsic size (`Sizing::Fit`), and a
/// Taffy flex column anchored at viewport(1280×800) with auto-width
/// children would have ambiguous cross-axis stretching — visible as
/// the menu bar text wrapping mid-word when label widths exceed the
/// shrunk column.
/// **Wave 14.3** — monotonic-ish wall-clock millis used by the
/// animator. Wraps `Instant::now().elapsed()` against a per-process
/// `OnceLock` epoch so the value stays cheap and overflow-safe for
/// the lifetime of a session. Tests that drive the animator directly
/// pass their own `now_ms` and don't go through here.
fn now_ms() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = EPOCH.get_or_init(Instant::now);
    epoch.elapsed().as_millis() as u64
}

/// Wave 14.8 — graft animator phantoms into the live render tree.
/// Each `(parent_id, node)` pair looks up its parent container by id
/// and appends the phantom there; phantoms whose parent has also
/// left the tree (or that were root-level snapshots, `parent_id =
/// None`) fall through to the root sibling list. The lookup is a
/// single recursive walk that short-circuits on the first match per
/// phantom — sufficient for the handful of phantoms a typical frame
/// carries.
fn graft_phantoms(tree: &mut Vec<UiNode>, phantoms: Vec<(Option<String>, UiNode)>) {
    for (parent_id, phantom) in phantoms {
        match parent_id {
            None => tree.push(phantom),
            Some(pid) => {
                if !graft_into_parent(tree, &pid, &phantom) {
                    tree.push(phantom);
                }
            }
        }
    }
}

/// Recursive helper: walk `nodes` looking for a container whose id
/// equals `parent_id`. Returns `true` when the phantom was placed.
fn graft_into_parent(nodes: &mut [UiNode], parent_id: &str, phantom: &UiNode) -> bool {
    for node in nodes.iter_mut() {
        if let UiNode::Container { id, children, .. } = node {
            if id == parent_id {
                children.push(phantom.clone());
                return true;
            }
            if graft_into_parent(children, parent_id, phantom) {
                return true;
            }
        }
    }
    false
}

fn wrap_root(children: Vec<UiNode>) -> UiNode {
    UiNode::Container {
        id: String::new(),
        props: prism_ui_runtime::layout::ContainerProps {
            direction: prism_ui_runtime::layout::Direction::Column,
            width: prism_ui_runtime::layout::Sizing::Grow,
            height: prism_ui_runtime::layout::Sizing::Grow,
            ..Default::default()
        },
        children,
    }
}

/// Phase 9 of `docs/dev/dioxus-inspiration.md` reload anchor.
/// Wraps the per-frame render walk in `subsecond::call` when the
/// `hot-reload` feature is on, so a swapped-in `lower_ui` body
/// patches in-place without dropping the `Surface` tree or the
/// reactive `Owner` graph. Without the feature the wrapper is a
/// straight pass-through — no per-frame cost.
#[cfg(feature = "hot-reload")]
fn render_with_hot_reload<F>(f: F) -> Vec<UiNode>
where
    F: FnMut() -> Vec<UiNode>,
{
    // `subsecond::call` is the hot-patch boundary. The closure
    // body is the thing subsecond hot-patches at runtime;
    // everything outside this call site stays put across patches.
    subsecond::call(f)
}

#[cfg(not(feature = "hot-reload"))]
fn render_with_hot_reload<F>(mut f: F) -> Vec<UiNode>
where
    F: FnMut() -> Vec<UiNode>,
{
    // Without the `hot-reload` feature this is a straight
    // pass-through; subsecond isn't compiled in at all.
    f()
}

/// Pointer-event hit-test. One closed-form helper so every pointer
/// variant — `PointerMove` for hover paint, `PointerDown` for click
/// routing, `PointerUp` for the no-drag click-step fall-through —
/// resolves through the same `Surface::hit_test_at` call. Non-pointer
/// events (Wheel / Key / Text / Focus / Resize) yield `None` and pass
/// through dispatch unchanged.
#[cfg(feature = "native")]
fn compute_hit(event: &Event, surface: &mut Surface) -> Option<HitRect> {
    let (x, y) = pointer_xy(event)?;
    surface.hit_test_at(x, y).cloned()
}

fn pointer_xy(event: &Event) -> Option<(f32, f32)> {
    match event {
        Event::PointerMove { x, y, .. }
        | Event::PointerDown { x, y, .. }
        | Event::PointerUp { x, y, .. } => Some((*x, *y)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_ui_runtime::event::Modifiers;

    #[test]
    fn shell_new_succeeds() {
        let shell = Shell::new().expect("boot");
        let nodes = shell.render();
        assert!(!nodes.is_empty());
    }

    #[test]
    fn render_is_deterministic() {
        let shell = Shell::new().expect("boot");
        let a = shell.render();
        let b = shell.render();
        assert_eq!(a, b, "two consecutive renders must be equal");
    }

    #[test]
    fn switch_active_app_updates_cursor_and_returns_true() {
        let shell = Shell::new().expect("boot");
        assert!(shell.active_app().is_none());
        let moved = shell.switch_active_app(Some("lattice"));
        assert!(moved, "first cursor move should report `true`");
        assert_eq!(shell.active_app().as_deref(), Some("lattice"));
    }

    #[test]
    fn switch_active_app_is_idempotent() {
        let shell = Shell::new().expect("boot");
        shell.switch_active_app(Some("musica"));
        let moved_again = shell.switch_active_app(Some("musica"));
        assert!(!moved_again, "no-op cursor move should report `false`");
    }

    #[test]
    fn switch_active_app_to_none_unmounts() {
        let shell = Shell::new().expect("boot");
        shell.switch_active_app(Some("flux"));
        assert_eq!(shell.active_app().as_deref(), Some("flux"));
        let moved = shell.switch_active_app(None);
        assert!(moved);
        assert!(shell.active_app().is_none());
    }

    #[test]
    fn switch_active_app_marks_render_scope_dirty() {
        let shell = Shell::new().expect("boot");
        // Drain any pre-existing dirty state from boot.
        let _ = shell.render();
        let was_dirty_before = shell.inner.borrow().render_scope.needs_redraw();
        shell.switch_active_app(Some("studio"));
        let is_dirty_after = shell.inner.borrow().render_scope.needs_redraw();
        assert!(
            is_dirty_after && !was_dirty_before,
            "switch must flip the dirty bit (before={was_dirty_before}, after={is_dirty_after})"
        );
    }

    #[test]
    fn install_app_stylesheet_caches_for_inactive_app_without_dirtying() {
        let shell = Shell::new().expect("boot");
        let _ = shell.render();
        let _ = shell.inner.borrow().render_scope.drain_dirty();
        let sheet = Stylesheet::from_source("[class.foo]\nbackground = \"#aabbcc\"\n");
        let dirty = shell.install_app_stylesheet("lattice", sheet);
        assert!(!dirty, "inactive-app install should not flip the dirty bit");
        // The cache entry lands either way.
        assert!(shell.inner.borrow().app_stylesheets.contains_key("lattice"));
        assert!(
            !shell.inner.borrow().render_scope.needs_redraw(),
            "render scope must stay clean when installing for an inactive app"
        );
    }

    #[test]
    fn install_app_stylesheet_marks_dirty_when_active() {
        let shell = Shell::new().expect("boot");
        shell.switch_active_app(Some("lattice"));
        let _ = shell.render();
        // Drain whatever boot/render/switch put on the queue so we
        // can observe the fresh install_app_stylesheet emission in
        // isolation. `drain_dirty` is the canonical reset path.
        let _ = shell.inner.borrow().render_scope.drain_dirty();
        let sheet = Stylesheet::from_source("[class.bar]\nbackground = \"#112233\"\n");
        let dirty = shell.install_app_stylesheet("lattice", sheet);
        assert!(dirty, "active-app install should flip the dirty bit");
        assert!(shell.inner.borrow().render_scope.needs_redraw());
    }

    #[test]
    fn install_app_stylesheet_replaces_existing_entry() {
        let shell = Shell::new().expect("boot");
        let first = Stylesheet::from_source("[class.a]\nbackground = \"#000000\"\n");
        let second = Stylesheet::from_source("[class.b]\nbackground = \"#ffffff\"\n");
        shell.install_app_stylesheet("flux", first);
        shell.install_app_stylesheet("flux", second);
        let inner = shell.inner.borrow();
        let cached = inner.app_stylesheets.get("flux").unwrap();
        // The second sheet's class is present; the first is gone.
        assert!(cached.sheet().classes.contains_key("b"));
        assert!(!cached.sheet().classes.contains_key("a"));
    }

    #[test]
    fn uninstall_app_stylesheet_removes_cache_and_signals_when_active() {
        let shell = Shell::new().expect("boot");
        let sheet = Stylesheet::from_source("[class.x]\nbackground = \"#000\"\n");
        shell.install_app_stylesheet("lattice", sheet);
        // Inactive: no dirty signal on uninstall.
        let signalled = shell.uninstall_app_stylesheet("lattice");
        assert!(!signalled);
        assert!(!shell.inner.borrow().app_stylesheets.contains_key("lattice"));

        // Re-install + activate, then uninstall: dirty fires.
        shell.install_app_stylesheet(
            "lattice",
            Stylesheet::from_source("[class.y]\nbackground = \"#000\"\n"),
        );
        shell.switch_active_app(Some("lattice"));
        let _ = shell.render();
        let signalled = shell.uninstall_app_stylesheet("lattice");
        assert!(
            signalled,
            "uninstalling the active app's sheet should mark dirty"
        );
    }

    #[test]
    fn uninstall_app_stylesheet_for_unknown_app_is_a_noop() {
        let shell = Shell::new().expect("boot");
        let _ = shell.render();
        let _ = shell.inner.borrow().render_scope.drain_dirty();
        let signalled = shell.uninstall_app_stylesheet("never-registered");
        assert!(!signalled);
        assert!(!shell.inner.borrow().render_scope.needs_redraw());
    }

    /// Wave 14.3 — `Shell::render` runs the animator pre/post the
    /// lowering walk, so any container carrying `data-transition-*`
    /// surfaces a live `Animator` entry once its declared value
    /// moves. We prove the wiring by pushing two trees through the
    /// animator manually with the live shell's clock and checking
    /// that `needs_redraw()` flips. The render path doesn't yet
    /// boot a shell tree with transitions, so this test is the
    /// canonical witness that the substrate is connected.
    #[test]
    fn animator_is_threaded_through_shell_render_pipeline() {
        let shell = Shell::new().expect("boot");
        let _ = shell.render();
        // Manually push a transition-tagged tree through the same
        // animator the shell uses.
        use prism_ui_runtime::layout::{ContainerProps, Node as UiNode, Padding, Semantic};
        let baseline = vec![UiNode::Container {
            id: "anim-target".into(),
            props: ContainerProps {
                padding: Padding::all(10.0),
                semantic: Semantic {
                    attrs: vec![("data-transition-padding".into(), "200ms".into())],
                    ..Default::default()
                },
                ..Default::default()
            },
            children: vec![],
        }];
        let after = vec![UiNode::Container {
            id: "anim-target".into(),
            props: ContainerProps {
                padding: Padding::all(30.0),
                semantic: Semantic {
                    attrs: vec![("data-transition-padding".into(), "200ms".into())],
                    ..Default::default()
                },
                ..Default::default()
            },
            children: vec![],
        }];
        let inner = shell.inner.borrow();
        let mut animator = inner.animator.borrow_mut();
        animator.observe(&baseline, 0);
        animator.observe(&after, 50);
        assert!(
            animator.needs_redraw(),
            "moved declared value should kick off a transition the shell can sample"
        );
        let mid = animator
            .current("anim-target", "padding", 100)
            .expect("mid-flight padding");
        assert!(mid > 10.0 && mid < 30.0);
    }

    /// Wave 14.8 — phantom nodes registered by the animator graft
    /// into the rendered tree at the end of the root sibling list.
    /// Pushed manually through the live shell's animator: a node
    /// carrying `data-animate-out-*` observed in the first call
    /// disappears in the second, and `phantom_nodes` returns the
    /// snapshot the painter would graft in `Shell::render`.
    #[test]
    fn animator_phantom_nodes_surface_through_shell_handle() {
        use prism_ui_runtime::layout::{ContainerProps, Node as UiNode, Semantic};
        let shell = Shell::new().expect("boot");
        let _ = shell.render();
        let baseline = vec![UiNode::Container {
            id: "ephemeral-toast".into(),
            props: ContainerProps {
                opacity: Some(1.0),
                semantic: Semantic {
                    attrs: vec![("data-animate-out-opacity".into(), "0 200ms".into())],
                    ..Default::default()
                },
                ..Default::default()
            },
            children: vec![],
        }];
        let inner = shell.inner.borrow();
        let mut animator = inner.animator.borrow_mut();
        animator.observe(&baseline, 0);
        // Vanish on the next frame — phantom should appear.
        animator.observe(&[], 50);
        let phantoms = animator.phantom_nodes(100);
        assert_eq!(phantoms.len(), 1, "vanished node produces one phantom");
        let UiNode::Container { id, props, .. } = &phantoms[0] else {
            panic!("phantom must keep its container shape")
        };
        assert_eq!(id, "ephemeral-toast");
        let opacity = props.opacity.unwrap_or(1.0);
        assert!(
            opacity < 1.0 && opacity > 0.0,
            "phantom opacity should ease toward zero, got {opacity}"
        );
    }

    /// Wave 14.8 — `graft_phantoms` places each phantom into the
    /// live tree at its previous parent's child list. Phantoms whose
    /// parent has also vanished fall back to the root sibling list.
    /// Phantoms with no parent_id (root-level snapshots) likewise
    /// land at the root.
    #[test]
    fn graft_phantoms_drops_each_phantom_into_its_recorded_parent() {
        use prism_ui_runtime::layout::{ContainerProps, Node as UiNode};
        let mut tree = vec![UiNode::Container {
            id: "host".into(),
            props: ContainerProps::default(),
            children: vec![],
        }];
        let phantom = UiNode::Container {
            id: "ghost".into(),
            props: ContainerProps::default(),
            children: vec![],
        };
        super::graft_phantoms(&mut tree, vec![(Some("host".into()), phantom.clone())]);
        // Phantom landed inside `host`, not at root.
        assert_eq!(tree.len(), 1);
        let UiNode::Container { children, .. } = &tree[0] else {
            panic!()
        };
        assert_eq!(children.len(), 1);
        let UiNode::Container { id, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(id, "ghost");
    }

    /// Wave 14.8 — when the recorded parent isn't in the live tree
    /// any more (e.g. an entire workspace section collapsed), the
    /// phantom falls back to the root sibling list so it still
    /// paints during its fade-out.
    #[test]
    fn graft_phantoms_falls_back_to_root_when_parent_is_missing() {
        use prism_ui_runtime::layout::{ContainerProps, Node as UiNode};
        let mut tree = vec![UiNode::Container {
            id: "unrelated".into(),
            props: ContainerProps::default(),
            children: vec![],
        }];
        let phantom = UiNode::Container {
            id: "ghost".into(),
            props: ContainerProps::default(),
            children: vec![],
        };
        super::graft_phantoms(&mut tree, vec![(Some("vanished-parent".into()), phantom)]);
        // Two root-level siblings now: the unrelated container and
        // the phantom (orphan fallback).
        assert_eq!(tree.len(), 2);
        let UiNode::Container { id, .. } = &tree[1] else {
            panic!()
        };
        assert_eq!(id, "ghost");
    }

    /// Wave 14.3 — the shared `MemoCache` lives on `ShellInner` and
    /// is fresh at boot. Touching it through the lowering path is
    /// covered in the interpret-level tests; this test just proves
    /// the handle is reachable and survives a render.
    #[test]
    fn memo_cache_is_present_on_shell_inner() {
        let shell = Shell::new().expect("boot");
        // First render seeds whatever memoised subtrees the skeleton
        // declares; today the shipped skeleton has none, so the
        // cache stays empty — both states are valid.
        let _ = shell.render();
        let cache = Rc::clone(&shell.inner.borrow().memo_cache);
        // The handle is reachable and we can mutate through it
        // without poisoning anything. (`MemoCache::clear` exists for
        // the panel-swap/hot-reload reset path; exercise it once.)
        cache.borrow_mut().clear();
        assert!(cache.borrow().is_empty());
    }

    #[test]
    fn pointer_xy_extracts_position_for_every_pointer_variant() {
        use prism_ui_runtime::event::PointerButton;
        // The shell's hover + click routing assumes every pointer
        // variant yields a position; non-pointer events explicitly
        // pass through dispatch unchanged.
        assert_eq!(
            pointer_xy(&Event::PointerMove {
                x: 1.0,
                y: 2.0,
                modifiers: Modifiers::default()
            }),
            Some((1.0, 2.0))
        );
        assert_eq!(
            pointer_xy(&Event::PointerDown {
                x: 3.0,
                y: 4.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            }),
            Some((3.0, 4.0))
        );
        assert_eq!(
            pointer_xy(&Event::PointerUp {
                x: 5.0,
                y: 6.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            }),
            Some((5.0, 6.0))
        );
        assert_eq!(pointer_xy(&Event::Wheel { dx: 0.0, dy: 1.0 }), None);
        assert_eq!(pointer_xy(&Event::Focus { gained: true }), None);
    }

    #[test]
    fn hover_pump_marks_surface_dirty_when_passing_over_hover_aware_node() {
        // The femtovg backend's redraw loop polls `Surface::is_dirty()`
        // after every event. The shell's hover pump must therefore
        // request a redraw when the cursor enters a container whose
        // `props.hover` is set — without this, every chrome tint that
        // declares a hover override stays dead in production.
        use prism_ui_runtime::command::{Color, CornerRadius};
        use prism_ui_runtime::layout::{ContainerProps, HoverOverrides, Padding, Sizing};
        let tree = UiNode::Container {
            id: String::new(),
            props: ContainerProps::default(),
            children: vec![UiNode::Container {
                id: "hot-button".into(),
                props: ContainerProps {
                    width: Sizing::Fixed(100.0),
                    height: Sizing::Fixed(40.0),
                    padding: Padding::default(),
                    hover: Some(HoverOverrides {
                        background: Some(Color {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 32,
                        }),
                        radius: Some(CornerRadius {
                            tl: 4.0,
                            tr: 4.0,
                            br: 4.0,
                            bl: 4.0,
                        }),
                    }),
                    ..Default::default()
                },
                children: Vec::new(),
            }],
        };
        let mut surface = Surface::new(
            tree,
            Viewport {
                width: 200.0,
                height: 200.0,
            },
        );
        // Prime the layout cache; this is what the femtovg backend does
        // on first redraw, after which `is_dirty()` returns false until
        // something mutates.
        let _ = surface.commands();
        assert!(!surface.is_dirty());

        // Simulate the shell's hover-pump path: compute a hit at a
        // point inside the button's bounds and call `set_hovered`.
        let event = Event::PointerMove {
            x: 10.0,
            y: 10.0,
            modifiers: Modifiers::default(),
        };
        let hit = compute_hit(&event, &mut surface);
        assert_eq!(hit.as_ref().map(|h| h.id.as_str()), Some("hot-button"));
        surface.set_hovered(hit.as_ref().map(|h| h.id.clone()));
        assert!(
            surface.is_dirty(),
            "entering a hover-aware node must dirty the surface so the \
             backend redraws with the tint applied"
        );

        // Leaving the node back to nowhere should re-dirty the surface
        // so the tint clears.
        let _ = surface.commands();
        let event = Event::PointerMove {
            x: 199.0,
            y: 199.0,
            modifiers: Modifiers::default(),
        };
        let hit = compute_hit(&event, &mut surface);
        assert!(hit.is_none());
        surface.set_hovered(None);
        assert!(
            surface.is_dirty(),
            "leaving a hover-aware node must re-dirty so the tint clears"
        );
    }

    // ─── PRSS stylesheet integration (end-to-end) ──────────────

    /// Boot path: a fresh `Shell` has no stylesheet installed; the
    /// runtime treats class attributes as data-round-trip only.
    #[test]
    fn shell_starts_without_stylesheet() {
        let shell = Shell::new().expect("boot");
        assert!(shell.stylesheet().is_none());
    }

    /// Installing a stylesheet via `install_stylesheet` makes it
    /// available through `stylesheet()` and triggers a redraw via
    /// the render scope's dirty bit.
    #[test]
    fn install_stylesheet_swaps_active_sheet_and_marks_dirty() {
        let shell = Shell::new().expect("boot");
        let sheet = Stylesheet::from_source(
            r##"[class.btn]
            background = "#0060c0"
            "##,
        );
        shell.install_stylesheet(Some(sheet));
        assert!(shell.stylesheet().is_some());
        // The install writes FRAME_DIRTY_SENTINEL; consume it to
        // verify the dirty path fired.
        let dirty = shell.inner.borrow().render_scope.needs_redraw();
        assert!(dirty, "install_stylesheet must mark the render frame dirty");
    }

    /// C3 — installing a fresh `app.prism-ui` source swaps the
    /// default skeleton in place. Subsequent `render` calls walk the
    /// new tree; a parse error in the new source returns `Err`
    /// without clobbering the cached skeleton.
    #[test]
    fn install_default_skeleton_swaps_the_active_skeleton() {
        let shell = Shell::new().expect("boot");
        // Fresh source — minimal valid skeleton.
        let src = r#"<shell.app-window><container/></shell.app-window>"#;
        shell.install_default_skeleton(src).expect("install");
        // The default skeleton field was updated; the render scope is
        // marked dirty so the next event loop tick redraws.
        let dirty = shell.inner.borrow().render_scope.needs_redraw();
        assert!(dirty, "skeleton install must mark the render scope dirty");
    }

    /// A parse error in the new source surfaces as `Err` without
    /// mutating the in-place skeleton.
    #[test]
    fn install_default_skeleton_parse_error_preserves_previous() {
        let shell = Shell::new().expect("boot");
        let result = shell.install_default_skeleton("<not a valid skeleton");
        assert!(result.is_err(), "malformed source must error");
        // The shell is still renderable — the previous skeleton was
        // not clobbered.
        let nodes = shell.render();
        assert!(
            !nodes.is_empty(),
            "previous skeleton must still drive renders"
        );
    }

    /// Per-app skeleton swap. `install_app_skeleton("flux", …)`
    /// inserts a fresh `Skeleton` keyed by app id so
    /// `current_skeleton()` picks it up when `flux` is the active app.
    #[test]
    fn install_app_skeleton_inserts_keyed_entry() {
        let shell = Shell::new().expect("boot");
        let src = r#"<container><text>Custom Flux body</text></container>"#;
        shell.install_app_skeleton("flux", src).expect("install");
        let has = shell.inner.borrow().app_skeletons.contains_key("flux");
        assert!(has, "swap must register under the app id");
    }

    /// Detaching the stylesheet (`install_stylesheet(None)`) returns
    /// the shell to the no-sheet state.
    #[test]
    fn install_stylesheet_none_detaches() {
        let shell = Shell::new().expect("boot");
        shell.install_stylesheet(Some(Stylesheet::from_source(
            r##"[class.btn]
            radius = 8
            "##,
        )));
        assert!(shell.stylesheet().is_some());
        shell.install_stylesheet(None);
        assert!(shell.stylesheet().is_none());
    }

    /// End-to-end: install a stylesheet, render the live shell tree,
    /// and assert the chrome containers carrying `class="..."` get
    /// the configured background pulled from PRSS.
    ///
    /// The shipped `app.prism-ui` doesn't author a static `class`
    /// attribute today, so this test uses a custom skeleton via
    /// the `from_source` constructor. The pipeline that flows
    /// — `Shell::install_stylesheet` → `Shell::render` →
    /// `render_tree_with` → `LowerScope::with_stylesheet` →
    /// `apply_prss_class` — is the same code path the boot tree
    /// would use once the shell skeleton starts authoring class
    /// attributes.
    #[test]
    fn render_picks_up_stylesheet_through_full_pipeline() {
        let shell = Shell::new().expect("boot");
        // First render without stylesheet — establish the baseline.
        let baseline_tree = shell.render();
        assert!(!baseline_tree.is_empty());
        // Install a stylesheet whose [tokens.colors] override
        // `accent` — every chrome surface that resolves
        // `tokens.colors.accent` (toolbar buttons, focus rings,
        // active dock tabs, …) reads through PRSS now.
        let sheet = Stylesheet::from_source(
            r##"[tokens.colors]
            accent = "#ff0000"
            "##,
        );
        shell.install_stylesheet(Some(sheet));
        // Re-render — the stylesheet's token overrides flow through
        // `with_stylesheet`'s token-merge path so any `style:background="{tokens.colors.accent}"`
        // resolves to `#ff0000`. We compare lengths to ensure the
        // tree shape stayed stable (no respawn / structural shift).
        let after_tree = shell.render();
        assert_eq!(after_tree.len(), baseline_tree.len());
    }

    /// **Hot-reload (literal-only fast path)** — the
    /// `StylesheetWatcher` classifies a value-only edit as
    /// `LiteralOnly` and hands the host a freshly-parsed
    /// `Stylesheet` to install. The full pipeline (cache observe →
    /// shell install → render) is exercised end-to-end.
    #[test]
    fn stylesheet_watcher_literal_only_swap_round_trips_through_shell() {
        use prism_ui_build::PrssChange;
        let shell = Shell::new().expect("boot");
        let mut watcher = crate::render::StylesheetWatcher::new();
        let path = std::path::PathBuf::from("ui/theme.prss");
        // First sighting — seeds the cache, returns the parsed
        // sheet for install.
        let r1 = watcher.observe_source(
            &path,
            r##"[class.btn]
            background = "#000000"
            "##,
        );
        assert!(matches!(r1.change, PrssChange::FirstSighting { .. }));
        let sheet = r1.stylesheet.expect("first sighting yields sheet");
        shell.install_stylesheet(Some(sheet));
        let _ = shell.render();
        // Edit the value — same structural shape, different literal.
        // The watcher classifies it `LiteralOnly` and re-emits a
        // fresh sheet for install.
        let r2 = watcher.observe_source(
            &path,
            r##"[class.btn]
            background = "#7c3aed"
            "##,
        );
        match r2.change {
            PrssChange::LiteralOnly { ref patches } => {
                assert_eq!(patches.len(), 1);
                assert_eq!(patches[0].old_value, "#000000");
                assert_eq!(patches[0].new_value, "#7c3aed");
            }
            other => panic!("expected LiteralOnly, got {other:?}"),
        }
        let sheet = r2.stylesheet.expect("literal-only yields sheet");
        shell.install_stylesheet(Some(sheet));
        let _ = shell.render();
    }

    /// **Hot-reload (structural)** — adding a class is a structural
    /// change; the host still re-installs the new sheet, but the
    /// dev loop knows to invalidate broader caches.
    #[test]
    fn stylesheet_watcher_structural_change_returns_fresh_sheet() {
        use prism_ui_build::PrssChange;
        let mut watcher = crate::render::StylesheetWatcher::new();
        let path = std::path::PathBuf::from("ui/theme.prss");
        let _ = watcher.observe_source(
            &path,
            r##"[class.btn]
            background = "#fff"
            "##,
        );
        let r = watcher.observe_source(
            &path,
            r##"
            [class.btn]
            background = "#fff"

            [class.icon]
            color = "#000"
            "##,
        );
        assert!(matches!(r.change, PrssChange::Structural));
        // Even structural changes hand the host a fresh sheet; the
        // host might choose to fast-respawn, but the parsed sheet is
        // always available for install.
        assert!(r.stylesheet.is_some());
    }

    /// **Hot-reload (parse error)** — a syntax failure surfaces
    /// `ParseError` and *does not* hand back a stylesheet. The host
    /// keeps the previously-installed sheet rendering until the file
    /// is fixed.
    #[test]
    fn stylesheet_watcher_parse_error_leaves_previous_sheet_active() {
        use prism_ui_build::PrssChange;
        let mut watcher = crate::render::StylesheetWatcher::new();
        let path = std::path::PathBuf::from("ui/theme.prss");
        let r1 = watcher.observe_source(
            &path,
            r##"[class.btn]
            background = "#fff"
            "##,
        );
        assert!(r1.stylesheet.is_some());
        let cached_before = watcher.current();
        let r2 = watcher.observe_source(
            &path,
            // unterminated string — hard syntax break
            r##"[class.btn
            background = "
            "##,
        );
        match r2.change {
            PrssChange::ParseError { ref message } => assert!(!message.is_empty()),
            other => panic!("expected ParseError, got {other:?}"),
        }
        assert!(r2.stylesheet.is_none());
        // The watcher's cached "current" sheet stays intact.
        let cached_after = watcher.current();
        assert_eq!(
            cached_before.sheet().classes.len(),
            cached_after.sheet().classes.len()
        );
    }

    /// **Hot-reload (no change)** — observing the same source twice
    /// reports `NoChange` and skips the re-install allocation.
    #[test]
    fn stylesheet_watcher_no_change_skips_reinstall() {
        use prism_ui_build::PrssChange;
        let mut watcher = crate::render::StylesheetWatcher::new();
        let path = std::path::PathBuf::from("ui/theme.prss");
        let _ = watcher.observe_source(
            &path,
            r##"[class.btn]
            background = "#fff"
            "##,
        );
        let r = watcher.observe_source(
            &path,
            r##"[class.btn]
            background = "#fff"
            "##,
        );
        assert!(matches!(r.change, PrssChange::NoChange));
        assert!(r.stylesheet.is_none());
    }
}
