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
    #[cfg(feature = "luau")]
    {
        assert!(caps.contains(&"luau.exec".to_string()));
    }
    #[cfg(feature = "build")]
    {
        assert!(caps.contains(&"build.run_step".to_string()));
    }
    #[cfg(feature = "watcher")]
    {
        assert!(caps.contains(&"watcher.watch".to_string()));
    }
    #[cfg(feature = "vfs")]
    {
        assert!(caps.contains(&"vfs.put".to_string()));
        assert!(caps.contains(&"vfs.get".to_string()));
        assert!(caps.contains(&"vfs.stats".to_string()));
    }
    #[cfg(feature = "crypto")]
    {
        assert!(caps.contains(&"crypto.keypair".to_string()));
        assert!(caps.contains(&"crypto.encrypt".to_string()));
        assert!(caps.contains(&"crypto.decrypt".to_string()));
    }
}

#[test]
fn installed_modules_reports_install_order() {
    // We exercise every built-in shortcut the *current* feature set
    // exposes, in install order, then assert the kernel reports the
    // same ordering. Each feature gate lets the test run against mobile
    // / embedded / wasm / full without having to fork it per target.
    #[allow(unused_mut)]
    let mut builder = DaemonBuilder::new();
    let mut expected: Vec<String> = Vec::new();

    #[cfg(feature = "crdt")]
    {
        builder = builder.with_crdt();
        expected.push("prism.crdt".to_string());
    }
    #[cfg(feature = "luau")]
    {
        builder = builder.with_luau();
        expected.push("prism.luau".to_string());
    }
    #[cfg(feature = "build")]
    {
        builder = builder.with_build();
        expected.push("prism.build".to_string());
    }
    #[cfg(feature = "watcher")]
    {
        builder = builder.with_watcher();
        expected.push("prism.watcher".to_string());
    }
    #[cfg(feature = "vfs")]
    {
        builder = builder.with_vfs();
        expected.push("prism.vfs".to_string());
    }
    #[cfg(feature = "crypto")]
    {
        builder = builder.with_crypto();
        expected.push("prism.crypto".to_string());
    }

    let kernel = builder.build().unwrap();
    assert_eq!(kernel.installed_modules(), expected.as_slice());
}

// ── VFS end-to-end through kernel.invoke ────────────────────────────────

#[cfg(feature = "vfs")]
#[test]
fn vfs_blob_store_roundtrips_through_kernel_invoke() {
    let kernel = DaemonBuilder::new().with_vfs().build().unwrap();

    let put = kernel
        .invoke(
            "vfs.put",
            json!({ "bytes": b"prism vfs roundtrip".to_vec() }),
        )
        .unwrap();
    let hash = put["hash"].as_str().unwrap().to_string();
    assert_eq!(hash.len(), 64);

    let has = kernel
        .invoke("vfs.has", json!({ "hash": hash.clone() }))
        .unwrap();
    assert_eq!(has["present"], true);
    assert_eq!(has["size"], 19);

    let got = kernel
        .invoke("vfs.get", json!({ "hash": hash.clone() }))
        .unwrap();
    let bytes: Vec<u8> = serde_json::from_value(got["bytes"].clone()).unwrap();
    assert_eq!(bytes, b"prism vfs roundtrip");

    let del = kernel
        .invoke("vfs.delete", json!({ "hash": hash }))
        .unwrap();
    assert_eq!(del["deleted"], true);
}

// ── Crypto end-to-end through kernel.invoke ─────────────────────────────

#[cfg(feature = "crypto")]
#[test]
fn crypto_keypair_ecdh_and_aead_flow_through_kernel() {
    let kernel = DaemonBuilder::new().with_crypto().build().unwrap();

    let alice = kernel.invoke("crypto.keypair", json!({})).unwrap();
    let bob = kernel.invoke("crypto.keypair", json!({})).unwrap();

    let shared_ab = kernel
        .invoke(
            "crypto.shared_secret",
            json!({
                "secret_key": alice["secret_key"],
                "peer_public_key": bob["public_key"],
            }),
        )
        .unwrap();
    let shared_ba = kernel
        .invoke(
            "crypto.shared_secret",
            json!({
                "secret_key": bob["secret_key"],
                "peer_public_key": alice["public_key"],
            }),
        )
        .unwrap();
    assert_eq!(shared_ab["shared_secret"], shared_ba["shared_secret"]);

    let ct = kernel
        .invoke(
            "crypto.encrypt",
            json!({
                "key": shared_ab["shared_secret"],
                "plaintext": "68656c6c6f20776f726c64", // "hello world"
            }),
        )
        .unwrap();

    let pt = kernel
        .invoke(
            "crypto.decrypt",
            json!({
                "key": shared_ba["shared_secret"],
                "ciphertext": ct["ciphertext"],
                "nonce": ct["nonce"],
            }),
        )
        .unwrap();
    assert_eq!(pt["plaintext"], "68656c6c6f20776f726c64");
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
