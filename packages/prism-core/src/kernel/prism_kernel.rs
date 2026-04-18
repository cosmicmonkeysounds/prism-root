//! `kernel::prism_kernel` — the canonical Layer-1 orchestration struct.
//!
//! Port of `packages/prism-studio/src/kernel/studio-kernel.ts` at 8426588,
//! reduced to the parts that survived the Slint migration. The legacy TS
//! kernel wired ObjectRegistry + CollectionStore + PrismBus + AtomStore +
//! LiveView + Puck + React lens registries into a single 2431-line class.
//! The Rust version is narrower by design: Slint owns the UI tree, so
//! everything Puck / React-specific stays out of `prism-core` and lives
//! in `prism-shell` or a host crate. What's left is the bag of
//! framework-free primitives every app composes:
//!
//! - `object_registry` — entity + edge type definitions.
//! - `plugin_registry` — plugin contributions fan-out.
//! - `config_registry` + `config_model` + `feature_flags` — the layered
//!   settings stack (scope cascade, watchers, feature gates).
//! - `notification_store` — runtime toast/alert queue.
//! - `activity_store` — append-only per-object audit trail.
//! - `builder_manager` — self-replicating profile / build-plan manager.
//! - `process_queue` — pluggable actor runtime queue.
//! - `ai_provider_registry` — AI provider routing.
//!
//! Intentional omissions:
//! - `CollectionStore` is gated on the `crdt` feature and owned by the
//!   host because CRDT availability is a deployment decision.
//! - `AutomationEngine` is constructed by hosts that plug in their own
//!   `AutomationStore` + `ActionHandler` set — `PrismKernel` does not
//!   force a particular wiring.
//! - Domain engines (`FluxRegistry`, `TimelineEngine`, `graph_analysis`)
//!   are stateless or per-document, so the host constructs them on
//!   demand rather than keeping a single instance on the kernel.
//! - The `network::*` subtree is deliberately left off the kernel
//!   surface — relay / presence / session managers compose differently
//!   per host (desktop vs relay vs daemon) and the TS studio-kernel
//!   already kept network as an opt-in via dedicated options.
//!
//! `PrismKernel` is inherently single-threaded: `ConfigModel`,
//! `FeatureFlags`, and `ActivityStore` all use interior mutability
//! without `Send + Sync`, which matches the TS main-thread invariant.
//! Multi-threaded hosts wrap `PrismKernel` in their own synchronisation
//! layer or run one instance per worker.

use std::rc::Rc;
use std::sync::Arc;

use super::actor::ProcessQueue;
use super::builder::{
    create_dry_run_executor, AppProfile, BuildExecutor, BuilderManager, BuilderManagerOptions,
};
use super::config::{ConfigModel, ConfigRegistry, FeatureFlags};
use super::initializer::{install_initializers, Disposer, KernelInitializer};
use super::intelligence::AiProviderRegistry;
use super::plugin::PluginRegistry;
use super::plugin_bundles::{create_builtin_bundles, install_plugin_bundles, PluginInstallContext};
use crate::foundation::object_model::ObjectRegistry;
use crate::interaction::activity::ActivityStore;
use crate::interaction::notification::NotificationStore;

// ── Options ─────────────────────────────────────────────────────────────────

/// Construction-time knobs for [`PrismKernel::new`].
///
/// Everything is optional — the defaults produce a fully usable kernel
/// with the six built-in plugin bundles installed and a dry-run build
/// executor. Hosts that need a daemon-backed build executor or extra
/// app profiles override the relevant fields.
pub struct PrismKernelOptions {
    /// Custom `ConfigRegistry`. `None` yields the built-in registry
    /// (17 settings + `ai-features` / `sync` feature flags).
    pub config_registry: Option<Rc<ConfigRegistry>>,
    /// Install the six built-in plugin bundles (work / finance / crm /
    /// life / assets / platform) onto the object and plugin registries
    /// at boot time. Defaults to `true`.
    pub install_builtin_bundles: bool,
    /// Override the build executor the `BuilderManager` dispatches
    /// through. `None` → `create_dry_run_executor()`. Hosts that talk
    /// to the Prism Daemon pass a `CallbackExecutor` wrapping their
    /// IPC layer.
    pub builder_executor: Option<Arc<dyn BuildExecutor>>,
    /// Extra (non-built-in) app profiles to seed the `BuilderManager`
    /// with. The six built-in profiles (studio / flux / lattice /
    /// cadence / grip / relay) are always installed.
    pub builder_profiles: Vec<AppProfile>,
}

impl Default for PrismKernelOptions {
    fn default() -> Self {
        Self {
            config_registry: None,
            install_builtin_bundles: true,
            builder_executor: None,
            builder_profiles: Vec::new(),
        }
    }
}

// ── Kernel ──────────────────────────────────────────────────────────────────

pub struct PrismKernel {
    pub object_registry: ObjectRegistry,
    pub plugin_registry: PluginRegistry,
    pub config_registry: Rc<ConfigRegistry>,
    pub config_model: ConfigModel,
    pub feature_flags: FeatureFlags,
    pub notification_store: NotificationStore,
    pub activity_store: ActivityStore,
    pub builder_manager: BuilderManager,
    pub process_queue: ProcessQueue,
    pub ai_provider_registry: AiProviderRegistry,
}

impl PrismKernel {
    /// Construct a kernel with the six built-in plugin bundles installed.
    pub fn new(options: PrismKernelOptions) -> Self {
        let config_registry = options
            .config_registry
            .unwrap_or_else(|| Rc::new(ConfigRegistry::new()));
        let config_model = ConfigModel::new(Rc::clone(&config_registry));
        let feature_flags = FeatureFlags::new(config_model.clone());

        let mut object_registry = ObjectRegistry::new();
        let mut plugin_registry = PluginRegistry::new();
        if options.install_builtin_bundles {
            let bundles = create_builtin_bundles();
            let mut ctx = PluginInstallContext {
                object_registry: &mut object_registry,
                plugin_registry: &mut plugin_registry,
            };
            install_plugin_bundles(&bundles, &mut ctx);
        }

        let builder_manager = BuilderManager::new(BuilderManagerOptions {
            executor: Some(
                options
                    .builder_executor
                    .unwrap_or_else(create_dry_run_executor),
            ),
            profiles: options.builder_profiles,
        });

        Self {
            object_registry,
            plugin_registry,
            config_registry,
            config_model,
            feature_flags,
            notification_store: NotificationStore::new(),
            activity_store: ActivityStore::new(),
            builder_manager,
            process_queue: ProcessQueue::default(),
            ai_provider_registry: AiProviderRegistry::new(),
        }
    }

    /// Install a batch of [`KernelInitializer`]s, returning a composite
    /// [`Disposer`] that tears them down in reverse install order.
    ///
    /// Mirrors the TS `installInitializers(kernel, …)` helper used by
    /// `studio-kernel.ts` to seed default templates, register action
    /// handlers, and wire autosave hooks.
    pub fn install_initializers(
        &self,
        initializers: &[Arc<dyn KernelInitializer<PrismKernel>>],
    ) -> Disposer {
        install_initializers(initializers, self)
    }
}

impl Default for PrismKernel {
    fn default() -> Self {
        Self::new(PrismKernelOptions::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::builder::{BuildStep, BuildTarget, BuiltInProfileId};
    use crate::kernel::initializer::{noop_disposer, KernelInitializerContext};
    use std::sync::Mutex;

    #[test]
    fn default_kernel_installs_six_builtin_plugins() {
        let k = PrismKernel::default();
        assert_eq!(k.plugin_registry.len(), 6);
        assert!(k.plugin_registry.has("prism.plugin.work"));
        assert!(k.plugin_registry.has("prism.plugin.platform"));
        // Plugin bundles seed concrete entity types into the object
        // registry as part of install.
        assert!(k.object_registry.get("work:gig").is_some());
        assert!(k.object_registry.get("life:habit").is_some());
    }

    #[test]
    fn can_skip_builtin_bundles() {
        let k = PrismKernel::new(PrismKernelOptions {
            install_builtin_bundles: false,
            ..Default::default()
        });
        assert_eq!(k.plugin_registry.len(), 0);
        assert!(k.object_registry.get("work:gig").is_none());
    }

    #[test]
    fn builder_manager_has_six_builtin_profiles() {
        let k = PrismKernel::default();
        let profiles = k.builder_manager.list_profiles();
        assert_eq!(profiles.len(), 6);
        assert!(k
            .builder_manager
            .get_profile(BuiltInProfileId::Flux.as_str())
            .is_some());
    }

    #[test]
    fn builder_manager_plans_dry_run_by_default() {
        let k = PrismKernel::default();
        let plan = k
            .builder_manager
            .plan_build(BuiltInProfileId::Flux.as_str(), BuildTarget::Web, true)
            .unwrap();
        assert!(plan.dry_run);
        assert!(plan
            .steps
            .iter()
            .any(|s| matches!(s, BuildStep::EmitFile { .. })));
    }

    #[test]
    fn config_registry_has_builtin_settings() {
        let k = PrismKernel::default();
        // `ui.theme` is one of the 17 built-in settings — verify the
        // registry wired through and the model resolves via the
        // default scope.
        let theme = k.config_model.get("ui.theme");
        assert!(!theme.is_null());
    }

    #[test]
    fn feature_flags_boot_with_builtin_definitions() {
        let k = PrismKernel::default();
        let _ = k.feature_flags.is_enabled("ai-features");
    }

    #[test]
    fn initializers_run_against_the_kernel_handle() {
        struct RecordingInit {
            id: String,
            seen: Arc<Mutex<Vec<usize>>>,
        }
        impl KernelInitializer<PrismKernel> for RecordingInit {
            fn id(&self) -> &str {
                &self.id
            }
            fn name(&self) -> &str {
                &self.id
            }
            fn install(&self, ctx: KernelInitializerContext<'_, PrismKernel>) -> Disposer {
                self.seen
                    .lock()
                    .unwrap()
                    .push(ctx.kernel.plugin_registry.len());
                noop_disposer()
            }
        }

        let seen: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
        let k = PrismKernel::default();
        let inits: Vec<Arc<dyn KernelInitializer<PrismKernel>>> = vec![Arc::new(RecordingInit {
            id: "one".into(),
            seen: seen.clone(),
        })];
        let dispose = k.install_initializers(&inits);
        dispose();
        // The initializer saw the kernel with six installed plugin bundles.
        assert_eq!(seen.lock().unwrap().as_slice(), &[6]);
    }

    #[test]
    fn process_queue_has_no_runtimes_until_host_registers_them() {
        let k = PrismKernel::default();
        assert_eq!(k.process_queue.runtime_names().len(), 0);
    }

    #[test]
    fn ai_provider_registry_starts_empty() {
        let k = PrismKernel::default();
        assert!(k.ai_provider_registry.active().is_none());
    }

    #[test]
    fn notification_and_activity_stores_are_empty() {
        let k = PrismKernel::default();
        assert_eq!(k.notification_store.get_unread_count(None), 0);
        assert_eq!(
            k.activity_store
                .get_events("any", GetEventsOptions::default())
                .len(),
            0
        );
    }

    use crate::interaction::activity::GetEventsOptions;
}
