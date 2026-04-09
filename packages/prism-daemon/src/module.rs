//! Daemon modules — the self-registering units that provide the daemon's
//! capabilities.
//!
//! A module is to the daemon what a [`LensBundle`] or [`PluginBundle`] is
//! to Studio: a stand-alone unit that, given a builder, contributes
//! services + command handlers without the builder needing to know which
//! concrete modules it will carry. Modules are composed at boot via the
//! [`DaemonBuilder`] and torn down in reverse order on `dispose()`.
//!
//! The trait is intentionally thin: `install` owns the *only* moment a
//! module can mutate the builder, so wiring stays explicit and traceable.
//!
//! [`LensBundle`]: https://docs — see @prism/core/lens
//! [`PluginBundle`]: https://docs — see @prism/core/layer1
//! [`DaemonBuilder`]: crate::builder::DaemonBuilder

use crate::builder::DaemonBuilder;
use crate::registry::CommandError;

/// Anything that can be plugged into a [`DaemonBuilder`] at boot.
///
/// Implementations typically:
///   1. Allocate any long-lived service state (wrapped in `Arc`).
///   2. Register one or more commands against the builder's
///      [`CommandRegistry`](crate::registry::CommandRegistry), capturing
///      that state in the handler closures.
///   3. Optionally stash the service on the builder so other modules /
///      initializers can access it later.
pub trait DaemonModule: Send + Sync {
    /// Stable, unique id for this module — used for logging and for the
    /// `modules installed` list exposed by the kernel.
    fn id(&self) -> &str;

    /// Install the module into the builder. Called exactly once, before
    /// the kernel is finalized.
    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError>;
}

/// Boxed module handle stored on the builder / kernel. We box because the
/// module set is heterogeneous and determined at runtime.
pub type BoxedModule = Box<dyn DaemonModule>;
