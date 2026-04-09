//! Daemon initializers — post-boot side-effect hooks.
//!
//! Modules run during `DaemonBuilder::build()` — while the kernel is still
//! being assembled. Initializers run *after* the kernel exists and can
//! freely call `kernel.invoke(...)` to seed state, register peers, bind
//! default keymaps, etc. This mirrors Studio's `StudioInitializer` split:
//! a module is for plumbing; an initializer is for side effects.

use crate::kernel::DaemonKernel;
use crate::registry::CommandError;

/// A post-boot side-effect hook. Receives a reference to the fully-built
/// kernel and returns an optional uninstall closure that's called on
/// `kernel.dispose()`.
pub trait DaemonInitializer: Send + Sync {
    fn id(&self) -> &str;

    /// Runs once, immediately after `DaemonBuilder::build()` finishes.
    fn install(&self, kernel: &DaemonKernel) -> Result<InitializerHandle, CommandError>;
}

/// Opaque handle returned by an initializer. Holds an optional uninstall
/// closure; defaults to a no-op so trivial initializers don't have to
/// bother.
pub struct InitializerHandle {
    uninstall: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl InitializerHandle {
    pub fn noop() -> Self {
        Self { uninstall: None }
    }

    pub fn new<F>(uninstall: F) -> Self
    where
        F: FnOnce() + Send + Sync + 'static,
    {
        Self {
            uninstall: Some(Box::new(uninstall)),
        }
    }

    pub(crate) fn run_uninstall(self) {
        if let Some(f) = self.uninstall {
            f();
        }
    }
}

pub type BoxedInitializer = Box<dyn DaemonInitializer>;
