//! The daemon kernel — the fully-assembled runtime that transport adapters
//! talk to.
//!
//! Conceptually: `DaemonKernel : prism-daemon :: StudioKernel : prism-studio`.
//! Everything a transport needs (command invocation, direct service access
//! for hot paths, clean shutdown) hangs off this struct.
//!
//! Instantiated via [`DaemonBuilder`](crate::builder::DaemonBuilder), never
//! directly.

use crate::initializer::InitializerHandle;
use crate::permission::Permission;
use crate::registry::{CommandError, CommandRegistry};
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex};

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

/// The fully-assembled daemon runtime.
///
/// Cheaply cloneable — every field is either an `Arc` or lives behind one,
/// so hosts can hand the same kernel out to multiple transport adapters
/// (e.g. local IPC + a debug stdio loop) without extra synchronization.
#[derive(Clone)]
pub struct DaemonKernel {
    registry: Arc<CommandRegistry>,

    /// Permission tier stamped onto the kernel at boot. Every call to
    /// [`DaemonKernel::invoke`] is checked against the registered command's
    /// minimum via [`CommandRegistry::invoke_with_permission`]. Flipped
    /// via [`crate::builder::DaemonBuilder::with_permission`]; defaults to
    /// [`Permission::Dev`] so embedded and test callers keep full access.
    permission: Permission,

    /// IDs of every module installed at boot, in install order. Exposed
    /// for debugging and for `daemon.capabilities`-style introspection.
    module_ids: Arc<Vec<String>>,

    /// Initializer handles — held so they stay alive as long as the kernel
    /// does, and torn down in reverse order on `dispose()`.
    initializer_handles: Arc<Mutex<Vec<InitializerHandle>>>,

    /// Optional direct handle to the CRDT service. Hot paths (like the
    /// Studio shell's managed state) can reach it without going through
    /// JSON (de)serialization — but they don't have to.
    #[cfg(feature = "crdt")]
    doc_manager: Option<Arc<DocManager>>,

    /// Optional direct handle to the filesystem watcher manager. Same
    /// rationale as `doc_manager`.
    #[cfg(feature = "watcher")]
    watcher_manager: Option<Arc<WatcherManager>>,

    /// Optional direct handle to the content-addressed blob store.
    /// Same rationale as `doc_manager` — lets hot paths (drag-and-drop
    /// of large assets, streaming uploads) avoid a JSON roundtrip.
    #[cfg(feature = "vfs")]
    vfs_manager: Option<Arc<VfsManager>>,

    /// Optional direct handle to the actors pool. Hosts that want to
    /// drive actor lifecycles without JSON (e.g. streaming audio frames
    /// to a Whisper sidecar) can reach in through this handle.
    #[cfg(feature = "actors")]
    actors_manager: Option<Arc<ActorsManager>>,

    /// Optional direct handle to the whisper.cpp model pool. Lets hot
    /// audio paths skip the JSON sample-array roundtrip when feeding
    /// PCM frames into a transcription state.
    #[cfg(feature = "whisper")]
    whisper_manager: Option<Arc<WhisperManager>>,

    /// Optional direct handle to the WebRTC conferencing manager. Lets
    /// hot media paths reach the underlying peer connection / data
    /// channels without funneling every byte through `kernel.invoke`.
    #[cfg(feature = "conferencing")]
    conferencing_manager: Option<Arc<ConferencingManager>>,
}

impl DaemonKernel {
    // Each manager handle is its own optional argument so transport
    // adapters can decide which capabilities to install. The list grows
    // every time a new feature lands; bundling them into a struct would
    // just push the same parameter list one layer deeper.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        registry: Arc<CommandRegistry>,
        permission: Permission,
        module_ids: Vec<String>,
        initializer_handles: Vec<InitializerHandle>,
        #[cfg(feature = "crdt")] doc_manager: Option<Arc<DocManager>>,
        #[cfg(feature = "watcher")] watcher_manager: Option<Arc<WatcherManager>>,
        #[cfg(feature = "vfs")] vfs_manager: Option<Arc<VfsManager>>,
        #[cfg(feature = "actors")] actors_manager: Option<Arc<ActorsManager>>,
        #[cfg(feature = "whisper")] whisper_manager: Option<Arc<WhisperManager>>,
        #[cfg(feature = "conferencing")] conferencing_manager: Option<Arc<ConferencingManager>>,
    ) -> Self {
        Self {
            registry,
            permission,
            module_ids: Arc::new(module_ids),
            initializer_handles: Arc::new(Mutex::new(initializer_handles)),
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
        }
    }

    /// The single transport-agnostic entry point: run a command by name.
    /// Every call is gated by the kernel's boot-time [`Permission`]; a
    /// `user`-tier kernel can only reach commands registered with
    /// [`Permission::User`], a `dev`-tier kernel reaches everything.
    /// Transport adapters (IPC handlers, HTTP handlers, FFI bridges)
    /// all funnel through this method.
    pub fn invoke(&self, name: &str, payload: JsonValue) -> Result<JsonValue, CommandError> {
        self.registry
            .invoke_with_permission(name, payload, self.permission)
    }

    /// Run a command while overriding the caller tier. Lets transport
    /// adapters that authenticate each request independently (e.g. an
    /// HTTP server issuing different tokens to dashboard users vs.
    /// developer tools) present a different caller than the kernel's
    /// stamped default.
    pub fn invoke_with_permission(
        &self,
        name: &str,
        payload: JsonValue,
        caller: Permission,
    ) -> Result<JsonValue, CommandError> {
        self.registry.invoke_with_permission(name, payload, caller)
    }

    /// The permission tier this kernel was built with.
    pub fn permission(&self) -> Permission {
        self.permission
    }

    /// The underlying command registry. Modules can hand this around when
    /// they need to register additional handlers after boot (e.g. a
    /// dynamic plugin loaded from Luau).
    pub fn registry(&self) -> &Arc<CommandRegistry> {
        &self.registry
    }

    /// Every registered command name, sorted.
    pub fn capabilities(&self) -> Vec<String> {
        self.registry.list()
    }

    /// IDs of every installed module, in install order.
    pub fn installed_modules(&self) -> &[String] {
        &self.module_ids
    }

    /// Direct CRDT service handle. Only present when built with the
    /// `crdt` feature and after [`DaemonBuilder::with_crdt`] ran.
    #[cfg(feature = "crdt")]
    pub fn doc_manager(&self) -> Option<Arc<DocManager>> {
        self.doc_manager.clone()
    }

    /// Direct filesystem watcher handle. Only present when built with the
    /// `watcher` feature and after [`DaemonBuilder::with_watcher`] ran.
    #[cfg(feature = "watcher")]
    pub fn watcher_manager(&self) -> Option<Arc<WatcherManager>> {
        self.watcher_manager.clone()
    }

    /// Direct content-addressed blob store handle. Only present when
    /// built with the `vfs` feature and after
    /// [`DaemonBuilder::with_vfs`](crate::builder::DaemonBuilder::with_vfs) ran.
    #[cfg(feature = "vfs")]
    pub fn vfs_manager(&self) -> Option<Arc<VfsManager>> {
        self.vfs_manager.clone()
    }

    /// Direct actors pool handle. Only present when built with the
    /// `actors` feature and after
    /// [`DaemonBuilder::with_actors`](crate::builder::DaemonBuilder::with_actors) ran.
    #[cfg(feature = "actors")]
    pub fn actors_manager(&self) -> Option<Arc<ActorsManager>> {
        self.actors_manager.clone()
    }

    /// Direct whisper.cpp model pool handle. Only present when built
    /// with the `whisper` feature and after
    /// [`DaemonBuilder::with_whisper`](crate::builder::DaemonBuilder::with_whisper) ran.
    #[cfg(feature = "whisper")]
    pub fn whisper_manager(&self) -> Option<Arc<WhisperManager>> {
        self.whisper_manager.clone()
    }

    /// Direct conferencing/WebRTC manager handle. Only present when
    /// built with the `conferencing` feature and after
    /// [`DaemonBuilder::with_conferencing`](crate::builder::DaemonBuilder::with_conferencing) ran.
    #[cfg(feature = "conferencing")]
    pub fn conferencing_manager(&self) -> Option<Arc<ConferencingManager>> {
        self.conferencing_manager.clone()
    }

    /// Release every initializer (in reverse order) and drop the registry.
    ///
    /// Safe to call multiple times — subsequent calls are no-ops.
    pub fn dispose(&self) {
        if let Ok(mut handles) = self.initializer_handles.lock() {
            while let Some(handle) = handles.pop() {
                handle.run_uninstall();
            }
        }
    }

    /// Install initializer handles after the kernel has been constructed.
    /// Called exactly once by [`DaemonBuilder::build`](crate::builder::DaemonBuilder::build)
    /// once every initializer has run successfully.
    pub(crate) fn install_initializer_handles(&self, handles: Vec<InitializerHandle>) {
        if let Ok(mut slot) = self.initializer_handles.lock() {
            *slot = handles;
        }
    }
}
