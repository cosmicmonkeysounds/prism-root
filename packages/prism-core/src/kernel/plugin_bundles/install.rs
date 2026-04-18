//! `plugin_bundles::install` — `PluginBundle` trait + shared install
//! context.
//!
//! Port of `kernel/plugin-bundles/plugin-install.ts`. Each bundle
//! self-registers entity defs, edge defs, and a `PrismPlugin`
//! contribution set against a shared [`PluginInstallContext`].
//!
//! The TS `install()` returns a `() => void` uninstall closure. Rust's
//! ownership model makes the closure-over-mut-references pattern
//! awkward, so the port narrows the contract: `install` borrows the
//! registries, mutates them, and returns nothing. The kernel owns the
//! registries for its lifetime; targeted unload goes through
//! `PluginRegistry::unregister(id)` directly.

use crate::foundation::object_model::ObjectRegistry;
use crate::kernel::plugin::PluginRegistry;

pub struct PluginInstallContext<'a> {
    pub object_registry: &'a mut ObjectRegistry,
    pub plugin_registry: &'a mut PluginRegistry,
}

pub trait PluginBundle: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn install(&self, ctx: &mut PluginInstallContext<'_>);
}

pub fn install_plugin_bundles(
    bundles: &[Box<dyn PluginBundle>],
    ctx: &mut PluginInstallContext<'_>,
) {
    for bundle in bundles {
        bundle.install(ctx);
    }
}
