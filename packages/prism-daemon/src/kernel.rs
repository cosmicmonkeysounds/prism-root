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
use crate::registry::{CommandError, CommandRegistry};
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex};

#[cfg(feature = "crdt")]
use crate::doc_manager::DocManager;

#[cfg(feature = "watcher")]
use crate::modules::watcher_module::WatcherManager;

/// The fully-assembled daemon runtime.
///
/// Cheaply cloneable — every field is either an `Arc` or lives behind one,
/// so hosts can hand the same kernel out to multiple transport adapters
/// (e.g. Tauri IPC + a debug stdio loop) without extra synchronization.
#[derive(Clone)]
pub struct DaemonKernel {
    registry: Arc<CommandRegistry>,

    /// IDs of every module installed at boot, in install order. Exposed
    /// for debugging and for `daemon.capabilities`-style introspection.
    module_ids: Arc<Vec<String>>,

    /// Initializer handles — held so they stay alive as long as the kernel
    /// does, and torn down in reverse order on `dispose()`.
    initializer_handles: Arc<Mutex<Vec<InitializerHandle>>>,

    /// Optional direct handle to the CRDT service. Hot paths (like the
    /// Tauri shell's managed state) can reach it without going through
    /// JSON (de)serialization — but they don't have to.
    #[cfg(feature = "crdt")]
    doc_manager: Option<Arc<DocManager>>,

    /// Optional direct handle to the filesystem watcher manager. Same
    /// rationale as `doc_manager`.
    #[cfg(feature = "watcher")]
    watcher_manager: Option<Arc<WatcherManager>>,
}

impl DaemonKernel {
    pub(crate) fn new(
        registry: Arc<CommandRegistry>,
        module_ids: Vec<String>,
        initializer_handles: Vec<InitializerHandle>,
        #[cfg(feature = "crdt")] doc_manager: Option<Arc<DocManager>>,
        #[cfg(feature = "watcher")] watcher_manager: Option<Arc<WatcherManager>>,
    ) -> Self {
        Self {
            registry,
            module_ids: Arc::new(module_ids),
            initializer_handles: Arc::new(Mutex::new(initializer_handles)),
            #[cfg(feature = "crdt")]
            doc_manager,
            #[cfg(feature = "watcher")]
            watcher_manager,
        }
    }

    /// The single transport-agnostic entry point: run a command by name.
    /// Transport adapters (Tauri `#[command]`, HTTP handlers, FFI bridges)
    /// all funnel through this method.
    pub fn invoke(&self, name: &str, payload: JsonValue) -> Result<JsonValue, CommandError> {
        self.registry.invoke(name, payload)
    }

    /// The underlying command registry. Modules can hand this around when
    /// they need to register additional handlers after boot (e.g. a
    /// dynamic plugin loaded from Lua).
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
