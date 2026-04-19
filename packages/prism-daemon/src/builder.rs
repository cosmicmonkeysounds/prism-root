//! Fluent builder for [`DaemonKernel`].
//!
//! ```ignore
//! use prism_daemon::DaemonBuilder;
//!
//! let kernel = DaemonBuilder::new()
//!     .with_crdt()
//!     .with_luau()
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
use crate::permission::Permission;
use crate::registry::{CommandError, CommandRegistry};
use std::sync::Arc;

#[cfg(feature = "crdt")]
use crate::doc_manager::DocManager;

#[cfg(feature = "watcher")]
use crate::modules::watcher_module::WatcherManager;

#[cfg(feature = "vfs")]
use crate::modules::vfs_module::VfsManager;

#[cfg(feature = "actors")]
use crate::modules::actors_module::ActorsManager;

#[cfg(feature = "whisper")]
use crate::modules::whisper_module::WhisperManager;

#[cfg(feature = "conferencing")]
use crate::modules::conferencing_module::ConferencingManager;

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

    /// Caller tier stamped onto the kernel at boot. Every `kernel.invoke`
    /// is checked against the registered command's minimum. Defaults to
    /// [`Permission::Dev`] so embedded and test callers keep full access;
    /// published `user`-tier binaries must opt in explicitly via
    /// [`DaemonBuilder::with_permission`] or the stdio binary's
    /// `--permission=user` flag.
    pub(crate) permission: Permission,

    /// Optional CRDT service. Modules that need it should insert via
    /// [`DaemonBuilder::set_doc_manager`] or re-use an existing one.
    #[cfg(feature = "crdt")]
    pub(crate) doc_manager: Option<Arc<DocManager>>,

    /// Optional watcher manager. Same rationale as `doc_manager`.
    #[cfg(feature = "watcher")]
    pub(crate) watcher_manager: Option<Arc<WatcherManager>>,

    /// Optional content-addressed blob store. Same rationale as
    /// `doc_manager` — hosts can inject a pre-rooted manager so the
    /// store lives in the app's data directory instead of the OS temp
    /// dir that the module defaults to.
    #[cfg(feature = "vfs")]
    pub(crate) vfs_manager: Option<Arc<VfsManager>>,

    /// Optional shared actors pool. Hosts rarely need to inject their
    /// own — it only matters if multiple transport adapters must share
    /// the exact same pool (e.g. HTTP + IPC pointed at the same
    /// kernel).
    #[cfg(feature = "actors")]
    pub(crate) actors_manager: Option<Arc<ActorsManager>>,

    /// Optional shared whisper.cpp model pool. Hosts may pre-load a
    /// flagship model from the asset directory before any module
    /// installs so the first transcription doesn't pay the model load
    /// cost on the request thread.
    #[cfg(feature = "whisper")]
    pub(crate) whisper_manager: Option<Arc<WhisperManager>>,

    /// Optional shared conferencing/WebRTC manager. Same rationale as
    /// `actors_manager` — only matters when multiple transport adapters
    /// must drive the same pool of peer connections.
    #[cfg(feature = "conferencing")]
    pub(crate) conferencing_manager: Option<Arc<ConferencingManager>>,
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
            permission: Permission::default(),
            #[cfg(feature = "crdt")]
            doc_manager: None,
            #[cfg(feature = "watcher")]
            watcher_manager: None,
            #[cfg(feature = "vfs")]
            vfs_manager: None,
            #[cfg(feature = "actors")]
            actors_manager: None,
            #[cfg(feature = "whisper")]
            whisper_manager: None,
            #[cfg(feature = "conferencing")]
            conferencing_manager: None,
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

    /// Set/override the VFS blob store. Hosts should inject a manager
    /// rooted at the app's data directory on startup — otherwise the
    /// `vfs` module will lazily create one under the OS temp dir.
    #[cfg(feature = "vfs")]
    pub fn set_vfs_manager(&mut self, vm: Arc<VfsManager>) -> &mut Self {
        self.vfs_manager = Some(vm);
        self
    }

    /// Current VFS blob store slot.
    #[cfg(feature = "vfs")]
    pub fn vfs_manager_slot(&mut self) -> &mut Option<Arc<VfsManager>> {
        &mut self.vfs_manager
    }

    /// Set/override the shared actors pool.
    #[cfg(feature = "actors")]
    pub fn set_actors_manager(&mut self, am: Arc<ActorsManager>) -> &mut Self {
        self.actors_manager = Some(am);
        self
    }

    /// Current actors pool slot.
    #[cfg(feature = "actors")]
    pub fn actors_manager_slot(&mut self) -> &mut Option<Arc<ActorsManager>> {
        &mut self.actors_manager
    }

    /// Set/override the shared whisper.cpp model pool.
    #[cfg(feature = "whisper")]
    pub fn set_whisper_manager(&mut self, wm: Arc<WhisperManager>) -> &mut Self {
        self.whisper_manager = Some(wm);
        self
    }

    /// Current whisper.cpp model pool slot.
    #[cfg(feature = "whisper")]
    pub fn whisper_manager_slot(&mut self) -> &mut Option<Arc<WhisperManager>> {
        &mut self.whisper_manager
    }

    /// Set/override the shared WebRTC conferencing manager.
    #[cfg(feature = "conferencing")]
    pub fn set_conferencing_manager(&mut self, cm: Arc<ConferencingManager>) -> &mut Self {
        self.conferencing_manager = Some(cm);
        self
    }

    /// Current WebRTC conferencing manager slot.
    #[cfg(feature = "conferencing")]
    pub fn conferencing_manager_slot(&mut self) -> &mut Option<Arc<ConferencingManager>> {
        &mut self.conferencing_manager
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

    /// Stamp the kernel with a caller tier. Published `user`-tier shells
    /// (Flux / Lattice / Musica builds hitting an end user) should flip
    /// this to [`Permission::User`]; developer tooling stays on the
    /// default [`Permission::Dev`]. The check runs on every
    /// `kernel.invoke`; a user-tier kernel can only reach commands that
    /// opted in via [`CommandRegistry::register_user`] / `register_with_permission`.
    pub fn with_permission(mut self, permission: Permission) -> Self {
        self.permission = permission;
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
    #[cfg(feature = "luau")]
    pub fn with_luau(self) -> Self {
        self.with_module(crate::modules::luau_module::LuauModule)
    }

    /// Install the Luau debugger module (`luau.debug.*` — launch,
    /// breakpoints, stepping, variable inspection, in-session eval).
    #[cfg(feature = "luau")]
    pub fn with_debug(self) -> Self {
        self.with_module(crate::modules::debug_module::DebugModule)
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

    /// Install the VFS content-addressed blob store (`vfs.put`,
    /// `vfs.get`, `vfs.has`, `vfs.delete`, `vfs.list`, `vfs.stats`).
    #[cfg(feature = "vfs")]
    pub fn with_vfs(self) -> Self {
        self.with_module(crate::modules::vfs_module::VfsModule)
    }

    /// Install the crypto module (X25519 keypairs + XChaCha20-Poly1305
    /// AEAD): `crypto.keypair`, `crypto.shared_secret`, `crypto.encrypt`,
    /// `crypto.decrypt`, `crypto.random_bytes`.
    #[cfg(feature = "crypto")]
    pub fn with_crypto(self) -> Self {
        self.with_module(crate::modules::crypto_module::CryptoModule)
    }

    /// Install the actors module (sandboxed Luau actor pool):
    /// `actors.spawn`, `actors.send`, `actors.recv`, `actors.status`,
    /// `actors.list`, `actors.stop`.
    #[cfg(feature = "actors")]
    pub fn with_actors(self) -> Self {
        self.with_module(crate::modules::actors_module::ActorsModule)
    }

    /// Install the whisper.cpp STT module: `whisper.load_model`,
    /// `whisper.unload_model`, `whisper.list_models`,
    /// `whisper.transcribe_pcm`, `whisper.transcribe_file`. Desktop-only —
    /// the underlying `whisper-rs` build script needs CMake on PATH.
    #[cfg(feature = "whisper")]
    pub fn with_whisper(self) -> Self {
        self.with_module(crate::modules::whisper_module::WhisperModule)
    }

    /// Install the WebRTC conferencing module:
    /// `conferencing.create_peer`, `conferencing.create_data_channel`,
    /// `conferencing.create_offer`, `conferencing.create_answer`,
    /// `conferencing.set_local_description`,
    /// `conferencing.set_remote_description`,
    /// `conferencing.local_description`,
    /// `conferencing.add_ice_candidate`, `conferencing.send_data`,
    /// `conferencing.recv_data`, `conferencing.peer_state`,
    /// `conferencing.list_peers`, `conferencing.close_peer`. Desktop-only.
    #[cfg(feature = "conferencing")]
    pub fn with_conferencing(self) -> Self {
        self.with_module(crate::modules::conferencing_module::ConferencingModule)
    }

    /// Install the admin module (`daemon.admin`). Always available — no
    /// feature gate. Returns a normalised admin snapshot (health, uptime,
    /// metrics, services) matching `@prism/admin-kit`'s `AdminSnapshot`.
    pub fn with_admin(self) -> Self {
        self.with_module(crate::modules::admin_module::AdminModule)
    }

    /// Install every built-in capability the current feature set allows.
    /// Equivalent to `createStudioKernel({ lensBundles: createBuiltinLensBundles() })`.
    pub fn with_defaults(mut self) -> Self {
        #[cfg(feature = "crdt")]
        {
            self = self.with_crdt();
        }
        #[cfg(feature = "luau")]
        {
            self = self.with_luau();
            self = self.with_debug();
        }
        #[cfg(feature = "build")]
        {
            self = self.with_build();
        }
        #[cfg(feature = "watcher")]
        {
            self = self.with_watcher();
        }
        #[cfg(feature = "vfs")]
        {
            self = self.with_vfs();
        }
        #[cfg(feature = "crypto")]
        {
            self = self.with_crypto();
        }
        #[cfg(feature = "actors")]
        {
            self = self.with_actors();
        }
        // Admin is always last so it sees every module that was installed.
        self = self.with_admin();
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
            permission,
            #[cfg(feature = "crdt")]
            doc_manager,
            #[cfg(feature = "watcher")]
            watcher_manager,
            #[cfg(feature = "vfs")]
            vfs_manager,
            #[cfg(feature = "actors")]
            actors_manager,
            #[cfg(feature = "whisper")]
            whisper_manager,
            #[cfg(feature = "conferencing")]
            conferencing_manager,
        } = self;

        let kernel = DaemonKernel::new(
            registry,
            permission,
            module_ids,
            Vec::new(),
            #[cfg(feature = "crdt")]
            doc_manager,
            #[cfg(feature = "watcher")]
            watcher_manager,
            #[cfg(feature = "vfs")]
            vfs_manager,
            #[cfg(feature = "actors")]
            actors_manager,
            #[cfg(feature = "whisper")]
            whisper_manager,
            #[cfg(feature = "conferencing")]
            conferencing_manager,
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
