//! `initializer` — self-registering startup hooks.
//!
//! Mirrors the `PluginBundle` / lens-bundle pattern: each initializer knows
//! how to install itself onto a kernel (seed data, register templates,
//! register action handlers, etc.) without the host having to call N
//! specific functions in the right order.
//!
//! The kernel runs initializers **after** all core state has been
//! constructed and its handle exists, so initializers can call any kernel
//! method freely.
//!
//! Generic over `TKernel` so each app specialises the context with its
//! own kernel type (Studio's `StudioKernel`, Flux's `FluxKernel`, …).
//! Port of `kernel/initializer/kernel-initializer.ts` at 8426588.

use std::sync::Arc;

/// Context handed to [`KernelInitializer::install`].
pub struct KernelInitializerContext<'k, TKernel> {
    pub kernel: &'k TKernel,
}

/// A disposer returned by a successful `install` call. Runs from
/// `kernel.dispose()` in reverse install order.
pub type Disposer = Box<dyn FnOnce() + Send + 'static>;

/// A self-installing startup hook.
pub trait KernelInitializer<TKernel>: Send + Sync {
    /// Unique id for debugging + deduplication.
    fn id(&self) -> &str;
    /// Human-readable name.
    fn name(&self) -> &str;
    /// Install this initializer onto the kernel. Returns a disposer that
    /// runs from `kernel.dispose()`.
    fn install(&self, ctx: KernelInitializerContext<'_, TKernel>) -> Disposer;
}

/// A disposer that does nothing. Useful for one-shot initializers.
pub fn noop_disposer() -> Disposer {
    Box::new(|| {})
}

/// Install multiple initializers in order. Returns a single composite
/// disposer that runs individual disposers in reverse install order.
pub fn install_initializers<TKernel>(
    initializers: &[Arc<dyn KernelInitializer<TKernel>>],
    kernel: &TKernel,
) -> Disposer {
    let mut disposers: Vec<Disposer> = Vec::with_capacity(initializers.len());
    for init in initializers {
        let ctx = KernelInitializerContext { kernel };
        disposers.push(init.install(ctx));
    }
    Box::new(move || {
        while let Some(dispose) = disposers.pop() {
            dispose();
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeKernel {
        installs: Arc<Mutex<Vec<String>>>,
        disposes: Arc<Mutex<Vec<String>>>,
    }

    struct Recorder {
        id: String,
        name: String,
    }

    impl KernelInitializer<FakeKernel> for Recorder {
        fn id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            &self.name
        }
        fn install(&self, ctx: KernelInitializerContext<'_, FakeKernel>) -> Disposer {
            ctx.kernel.installs.lock().unwrap().push(self.id.clone());
            let disposes = ctx.kernel.disposes.clone();
            let id = self.id.clone();
            Box::new(move || {
                disposes.lock().unwrap().push(id);
            })
        }
    }

    #[test]
    fn installs_in_order_and_disposes_in_reverse() {
        let kernel = FakeKernel::default();
        let inits: Vec<Arc<dyn KernelInitializer<FakeKernel>>> = vec![
            Arc::new(Recorder {
                id: "one".into(),
                name: "one".into(),
            }),
            Arc::new(Recorder {
                id: "two".into(),
                name: "two".into(),
            }),
            Arc::new(Recorder {
                id: "three".into(),
                name: "three".into(),
            }),
        ];

        let dispose = install_initializers(&inits, &kernel);
        assert_eq!(
            kernel.installs.lock().unwrap().clone(),
            vec!["one", "two", "three"]
        );
        dispose();
        assert_eq!(
            kernel.disposes.lock().unwrap().clone(),
            vec!["three", "two", "one"]
        );
    }

    #[test]
    fn noop_disposer_runs_cleanly() {
        let d = noop_disposer();
        d();
    }

    #[test]
    fn ids_and_names_are_accessible() {
        let r = Recorder {
            id: "seed-templates".into(),
            name: "Seed built-in templates".into(),
        };
        let r_trait: &dyn KernelInitializer<FakeKernel> = &r;
        assert_eq!(r_trait.id(), "seed-templates");
        assert_eq!(r_trait.name(), "Seed built-in templates");
    }
}
