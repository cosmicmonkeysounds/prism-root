//! End-to-end integration tests that drive the kernel through its public
//! surface only — no reach-ins. Mirrors the "build the whole Studio kernel
//! in a test" style used on the TS side.

use prism_daemon::{
    CommandError, DaemonBuilder, DaemonInitializer, DaemonKernel, DaemonModule, InitializerHandle,
};
use serde_json::{json, Value as JsonValue};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// ── Full-defaults assembly ──────────────────────────────────────────────

#[test]
fn with_defaults_installs_every_feature_module() {
    let kernel = DaemonBuilder::new().with_defaults().build().unwrap();

    let caps = kernel.capabilities();

    #[cfg(feature = "crdt")]
    {
        assert!(caps.contains(&"crdt.write".to_string()));
        assert!(caps.contains(&"crdt.read".to_string()));
    }
    #[cfg(feature = "lua")]
    {
        assert!(caps.contains(&"lua.exec".to_string()));
    }
    #[cfg(feature = "build")]
    {
        assert!(caps.contains(&"build.run_step".to_string()));
    }
    #[cfg(feature = "watcher")]
    {
        assert!(caps.contains(&"watcher.watch".to_string()));
    }
}

#[test]
fn installed_modules_reports_install_order() {
    let kernel = DaemonBuilder::new()
        .with_crdt()
        .with_lua()
        .with_build()
        .with_watcher()
        .build()
        .unwrap();

    let modules = kernel.installed_modules();
    assert_eq!(
        modules,
        &[
            "prism.crdt".to_string(),
            "prism.lua".to_string(),
            "prism.build".to_string(),
            "prism.watcher".to_string(),
        ]
    );
}

#[test]
fn kernel_is_cloneable_and_all_clones_share_state() {
    let a = DaemonBuilder::new().with_crdt().build().unwrap();
    let b = a.clone();

    a.invoke(
        "crdt.write",
        json!({ "docId": "shared", "key": "k", "value": "v" }),
    )
    .unwrap();

    let out = b
        .invoke("crdt.read", json!({ "docId": "shared", "key": "k" }))
        .unwrap();
    assert_eq!(out["value"], JsonValue::String("\"v\"".to_string()));
}

// ── Custom modules ──────────────────────────────────────────────────────

struct CounterModule {
    count: Arc<AtomicUsize>,
}

impl DaemonModule for CounterModule {
    fn id(&self) -> &str {
        "test.counter"
    }

    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        let counter = self.count.clone();
        builder.registry().register("counter.bump", move |_| {
            let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(json!({ "count": n }))
        })?;

        let counter = self.count.clone();
        builder.registry().register("counter.get", move |_| {
            Ok(json!({ "count": counter.load(Ordering::SeqCst) }))
        })?;
        Ok(())
    }
}

#[test]
fn custom_module_registers_and_runs_through_kernel_invoke() {
    let count = Arc::new(AtomicUsize::new(0));
    let kernel = DaemonBuilder::new()
        .with_module(CounterModule {
            count: count.clone(),
        })
        .build()
        .unwrap();

    assert_eq!(
        kernel.invoke("counter.get", JsonValue::Null).unwrap()["count"],
        JsonValue::from(0)
    );

    kernel.invoke("counter.bump", JsonValue::Null).unwrap();
    kernel.invoke("counter.bump", JsonValue::Null).unwrap();
    kernel.invoke("counter.bump", JsonValue::Null).unwrap();

    assert_eq!(count.load(Ordering::SeqCst), 3);
    assert_eq!(
        kernel.invoke("counter.get", JsonValue::Null).unwrap()["count"],
        JsonValue::from(3)
    );
}

// ── Initializers run post-boot and tear down in reverse ────────────────

struct OrderingInitializer {
    id: &'static str,
    log: Arc<Mutex<Vec<String>>>,
}

impl DaemonInitializer for OrderingInitializer {
    fn id(&self) -> &str {
        self.id
    }

    fn install(&self, _kernel: &DaemonKernel) -> Result<InitializerHandle, CommandError> {
        self.log
            .lock()
            .unwrap()
            .push(format!("install:{}", self.id));
        let log = self.log.clone();
        let id = self.id.to_string();
        Ok(InitializerHandle::new(move || {
            log.lock().unwrap().push(format!("uninstall:{}", id));
        }))
    }
}

#[test]
fn initializers_install_in_order_and_uninstall_reverse() {
    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let kernel = DaemonBuilder::new()
        .with_initializer(OrderingInitializer {
            id: "first",
            log: log.clone(),
        })
        .with_initializer(OrderingInitializer {
            id: "second",
            log: log.clone(),
        })
        .with_initializer(OrderingInitializer {
            id: "third",
            log: log.clone(),
        })
        .build()
        .unwrap();

    assert_eq!(
        *log.lock().unwrap(),
        vec![
            "install:first".to_string(),
            "install:second".to_string(),
            "install:third".to_string(),
        ]
    );

    kernel.dispose();

    assert_eq!(
        *log.lock().unwrap(),
        vec![
            "install:first".to_string(),
            "install:second".to_string(),
            "install:third".to_string(),
            "uninstall:third".to_string(),
            "uninstall:second".to_string(),
            "uninstall:first".to_string(),
        ]
    );
}

#[test]
fn initializers_can_call_kernel_invoke() {
    struct SeedInitializer;
    impl DaemonInitializer for SeedInitializer {
        fn id(&self) -> &str {
            "test.seed"
        }
        fn install(&self, kernel: &DaemonKernel) -> Result<InitializerHandle, CommandError> {
            kernel.invoke(
                "crdt.write",
                json!({ "docId": "seed", "key": "hello", "value": "world" }),
            )?;
            Ok(InitializerHandle::noop())
        }
    }

    let kernel = DaemonBuilder::new()
        .with_crdt()
        .with_initializer(SeedInitializer)
        .build()
        .unwrap();

    let out = kernel
        .invoke("crdt.read", json!({ "docId": "seed", "key": "hello" }))
        .unwrap();
    assert_eq!(out["value"], JsonValue::String("\"world\"".to_string()));
}

// ── daemon-level introspection ─────────────────────────────────────────

#[test]
fn empty_kernel_has_no_capabilities() {
    let kernel = DaemonBuilder::new().build().unwrap();
    assert!(kernel.capabilities().is_empty());
    assert!(kernel.installed_modules().is_empty());

    // Invoking anything yields NotFound.
    let err = kernel.invoke("crdt.read", json!({})).unwrap_err();
    assert!(matches!(err, CommandError::NotFound(_)));
}
