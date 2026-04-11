//! Fluent builder for [`DaemonKernel`].
//!
//! ```ignore
//! use prism_daemon::DaemonBuilder;
//!
//! let kernel = DaemonBuilder::new()
//!     .with_crdt()
//!     .with_lua()
//!     .with_build()
//!     .with_watcher()
//!     .build()
//!     .expect("kernel assembly failed");
//! ```
//!
//! This is intentionally shaped exactly like Studio's
//! `createStudioKernel({ lensBundles, initializers })`: each `with_*` call
//! is a module install, `with_initializer` is a post-boot hook, and
//! `build` is the finalization step that produces the kernel all transport
//! adapters will call into.

use crate::initializer::{BoxedInitializer, DaemonInitializer, InitializerHandle};
use crate::kernel::DaemonKernel;
use crate::module::{BoxedModule, DaemonModule};
use crate::registry::{CommandError, CommandRegistry};
use std::sync::Arc;

#[cfg(feature = "crdt")]
use crate::doc_manager::DocManager;

#[cfg(feature = "watcher")]
use crate::modules::watcher_module::WatcherManager;

/// Assembles a [`DaemonKernel`] by plugging modules + initializers into a
/// shared [`CommandRegistry`].
pub struct DaemonBuilder {
    pub(crate) registry: Arc<CommandRegistry>,

    /// Ordered list of installed modules. We keep them alive on the
    /// kernel so future "module list" / "uninstall" APIs have state to
    /// work with.
    pub(crate) modules: Vec<BoxedModule>,

    /// Installed module IDs in install order.
    pub(crate) module_ids: Vec<String>,

    /// Queued initializers, run in order immediately after build.
    pub(crate) initializers: Vec<BoxedInitializer>,

    /// Optional CRDT service. Modules that need it should insert via
    /// [`DaemonBuilder::set_doc_manager`] or re-use an existing one.
    #[cfg(feature = "crdt")]
    pub(crate) doc_manager: Option<Arc<DocManager>>,

    /// Optional watcher manager. Same rationale as `doc_manager`.
    #[cfg(feature = "watcher")]
    pub(crate) watcher_manager: Option<Arc<WatcherManager>>,
}

impl Default for DaemonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DaemonBuilder {
    /// Start a new empty builder. Nothing is installed — you opt in to
    /// every capability explicitly so mobile/embedded builds can stay
    /// minimal.
    pub fn new() -> Self {
        Self {
            registry: Arc::new(CommandRegistry::new()),
            modules: Vec::new(),
            module_ids: Vec::new(),
            initializers: Vec::new(),
            #[cfg(feature = "crdt")]
            doc_manager: None,
            #[cfg(feature = "watcher")]
            watcher_manager: None,
        }
    }

    // ── Shared service accessors ─────────────────────────────────────────

    /// Borrow the command registry. Modules call this inside their
    /// `install()` to register handlers.
    pub fn registry(&self) -> &Arc<CommandRegistry> {
        &self.registry
    }

    /// Set/override the CRDT service. The built-in crdt module calls this
    /// if no service has been attached yet. Exposed so hosts can inject a
    /// preconfigured [`DocManager`] (e.g. one already hydrated from disk).
    #[cfg(feature = "crdt")]
    pub fn set_doc_manager(&mut self, dm: Arc<DocManager>) -> &mut Self {
        self.doc_manager = Some(dm);
        self
    }

    /// Current CRDT service handle, if any. Built-in modules can use
    /// `get_or_insert_with` on this to lazily provision one.
    #[cfg(feature = "crdt")]
    pub fn doc_manager_slot(&mut self) -> &mut Option<Arc<DocManager>> {
        &mut self.doc_manager
    }

    /// Set/override the watcher manager.
    #[cfg(feature = "watcher")]
    pub fn set_watcher_manager(&mut self, wm: Arc<WatcherManager>) -> &mut Self {
        self.watcher_manager = Some(wm);
        self
    }

    /// Current watcher manager slot.
    #[cfg(feature = "watcher")]
    pub fn watcher_manager_slot(&mut self) -> &mut Option<Arc<WatcherManager>> {
        &mut self.watcher_manager
    }

    // ── Generic extension points ─────────────────────────────────────────

    /// Plug in any [`DaemonModule`]. Runs the module's `install` hook
    /// immediately so later modules see its contributions in the registry.
    pub fn with_module<M: DaemonModule + 'static>(mut self, module: M) -> Self {
        if let Err(e) = self.install_module(Box::new(module)) {
            panic!("failed to install daemon module: {e}");
        }
        self
    }

    /// Fallible variant of [`with_module`].
    pub fn try_with_module<M: DaemonModule + 'static>(
        mut self,
        module: M,
    ) -> Result<Self, CommandError> {
        self.install_module(Box::new(module))?;
        Ok(self)
    }

    /// Queue a post-boot initializer.
    pub fn with_initializer<I: DaemonInitializer + 'static>(mut self, init: I) -> Self {
        self.initializers.push(Box::new(init));
        self
    }

    fn install_module(&mut self, module: BoxedModule) -> Result<(), CommandError> {
        let id = module.id().to_string();
        module.install(self)?;
        self.module_ids.push(id);
        self.modules.push(module);
        Ok(())
    }

    // ── Built-in capability shortcuts ────────────────────────────────────
    //
    // These are sugar over `with_module(CrdtModule)` etc. and are only
    // compiled in when the corresponding feature is active. Mobile/embedded
    // builds that don't pull in `build` won't see `.with_build()` — that's
    // deliberate: the absence is the compile-time proof that the capability
    // is not in this binary.

    /// Install the CRDT module (Loro-backed docs + `crdt.*` commands).
    #[cfg(feature = "crdt")]
    pub fn with_crdt(self) -> Self {
        self.with_module(crate::modules::crdt_module::CrdtModule)
    }

    /// Install the Luau scripting module (`luau.exec`).
    #[cfg(feature = "lua")]
    pub fn with_lua(self) -> Self {
        self.with_module(crate::modules::luau_module::LuauModule)
    }

    /// Install the build-step executor (`build.run_step`). Not safe on
    /// iOS (process spawning is banned by the OS) — leave this off for
    /// mobile builds.
    #[cfg(feature = "build")]
    pub fn with_build(self) -> Self {
        self.with_module(crate::modules::build_module::BuildModule)
    }

    /// Install the filesystem watcher module (`watcher.watch`,
    /// `watcher.poll`, `watcher.stop`).
    #[cfg(feature = "watcher")]
    pub fn with_watcher(self) -> Self {
        self.with_module(crate::modules::watcher_module::WatcherModule)
    }

    /// Install every built-in capability the current feature set allows.
    /// Equivalent to `createStudioKernel({ lensBundles: createBuiltinLensBundles() })`.
    pub fn with_defaults(mut self) -> Self {
        #[cfg(feature = "crdt")]
        {
            self = self.with_crdt();
        }
        #[cfg(feature = "lua")]
        {
            self = self.with_lua();
        }
        #[cfg(feature = "build")]
        {
            self = self.with_build();
        }
        #[cfg(feature = "watcher")]
        {
            self = self.with_watcher();
        }
        self
    }

    // ── Finalize ─────────────────────────────────────────────────────────

    /// Assemble the kernel, then run every queued initializer in order.
    ///
    /// If any initializer fails, the kernel that's been built so far is
    /// still returned to the caller via the `Err` variant's context —
    /// actually no, we just propagate the error. The kernel is not
    /// partially constructed from the caller's point of view.
    pub fn build(self) -> Result<DaemonKernel, CommandError> {
        let DaemonBuilder {
            registry,
            modules: _modules,
            module_ids,
            initializers,
            #[cfg(feature = "crdt")]
            doc_manager,
            #[cfg(feature = "watcher")]
            watcher_manager,
        } = self;

        let kernel = DaemonKernel::new(
            registry,
            module_ids,
            Vec::new(),
            #[cfg(feature = "crdt")]
            doc_manager,
            #[cfg(feature = "watcher")]
            watcher_manager,
        );

        // Run initializers. They may call kernel.invoke(...) freely.
        let mut handles: Vec<InitializerHandle> = Vec::with_capacity(initializers.len());
        for init in initializers {
            match init.install(&kernel) {
                Ok(handle) => handles.push(handle),
                Err(e) => {
                    // Tear down already-installed initializers in reverse.
                    while let Some(h) = handles.pop() {
                        h.run_uninstall();
                    }
                    return Err(e);
                }
            }
        }

        kernel.install_initializer_handles(handles);
        Ok(kernel)
    }
}
