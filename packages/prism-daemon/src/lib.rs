//! Prism Daemon — the local physics engine.
//!
//! The daemon is a **transport-agnostic Rust kernel** for Prism's local
//! capabilities (CRDT merging, Luau scripting, filesystem watching, build-
//! plan execution, hardware bridges, …). Every capability lives behind a
//! [`DaemonModule`] that self-registers JSON-in/JSON-out handlers on a
//! shared [`CommandRegistry`]. Modules are plugged in via the fluent
//! [`DaemonBuilder`], producing a [`DaemonKernel`] that hosts talk to
//! through a single entry point: [`DaemonKernel::invoke`].
//!
//! This is the same self-replicating paradigm Studio uses (bundle +
//! initializer + kernel registry), ported to Rust so the exact same
//! assembly works whether the kernel is embedded behind Tauri on desktop,
//! compiled into a Capacitor native shell on iOS/Android, run as a
//! headless daemon on a server, or wrapped by a UniFFI bridge for any
//! other host.
//!
//! ### Quick start
//!
//! ```ignore
//! use prism_daemon::DaemonBuilder;
//! use serde_json::json;
//!
//! let kernel = DaemonBuilder::new()
//!     .with_defaults() // every feature the current build enabled
//!     .build()
//!     .unwrap();
//!
//! kernel.invoke(
//!     "crdt.write",
//!     json!({ "docId": "notes", "key": "title", "value": "Hello" }),
//! ).unwrap();
//! ```
//!
//! ### Feature flags
//!
//! | Feature   | Pulls in                  | Commands registered |
//! |-----------|---------------------------|---------------------|
//! | `crdt`    | `loro`                    | `crdt.{write,read,export,import}` |
//! | `luau`    | `mlua` (luau + vendored)  | `luau.exec` |
//! | `build`   | —                         | `build.run_step` |
//! | `watcher` | `notify`                  | `watcher.{watch,poll,stop}` |
//! | `vfs`     | `sha2 + hex`              | `vfs.{put,get,has,delete,list,stats}` |
//! | `crypto`  | `chacha20poly1305 + x25519-dalek + rand_core + hex` | `crypto.{keypair,derive_public,shared_secret,encrypt,decrypt,random_bytes}` |
//! | `cli`     | `tokio`                   | (enables the `prism-daemond` binary) |
//! | `mobile`  | `crdt + luau + vfs + crypto` | (enables the C-ABI adapter in [`wasm`] for iOS/Android staticlibs) |
//! | `wasm`    | `crdt + luau + vfs + crypto` | (enables the C-ABI adapter in [`wasm`] for emscripten) |
//!
//! `default = ["full"]`. Mobile shells override with `mobile` (crdt, luau,
//! vfs, and crypto — no process spawning, no filesystem watcher). Embedded
//! targets can pick `embedded` (crdt only). Browser shells pick `wasm` and
//! cross-compile to `wasm32-unknown-emscripten`.

#![deny(clippy::all)]

pub mod builder;
pub mod initializer;
pub mod kernel;
pub mod module;
pub mod modules;
pub mod registry;

#[cfg(any(
    feature = "transport-http",
    feature = "transport-grpc",
    feature = "transport-uniffi"
))]
pub mod transport;

// UniFFI requires its `UniFfiTag` marker to be visible from the crate
// root, so the scaffolding macro lives here even though every other
// piece of the bridge is in `transport::uniffi_bridge`.
#[cfg(feature = "transport-uniffi")]
uniffi::setup_scaffolding!();

#[cfg(feature = "crdt")]
pub mod doc_manager;

// The C ABI adapter (`prism_daemon_{create,destroy,invoke,free_string}`) is
// the single entry point used by every non-Rust host: the browser via
// emscripten (`wasm` feature) AND the Capacitor native shells on iOS/Android
// (`mobile` feature, consuming `libprism_daemon.a` as a staticlib through a
// hand-written Swift/Kotlin plugin). Desktop (Tauri) speaks Rust directly
// and doesn't need the C ABI. The module is still named `wasm` for
// historical reasons — it was introduced for the browser build — but the
// cfg reflects that both host families use it.
#[cfg(any(feature = "wasm", feature = "mobile"))]
pub mod wasm;

// ── Re-exports — the public surface hosts import from. ─────────────────

pub use builder::DaemonBuilder;
pub use initializer::{DaemonInitializer, InitializerHandle};
pub use kernel::DaemonKernel;
pub use module::DaemonModule;
pub use registry::{CommandError, CommandHandler, CommandRegistry};

#[cfg(feature = "crdt")]
pub use doc_manager::DocManager;

#[cfg(feature = "vfs")]
pub use modules::vfs_module::{VfsEntry, VfsManager, VfsStats};

#[cfg(feature = "actors")]
pub use modules::actors_module::{ActorKind, ActorMessage, ActorStatus, ActorsManager};

// ── Daemon-level error surface ─────────────────────────────────────────
//
// Kept at the crate root because the built-in CRDT service (and any host
// that accesses `DocManager` directly, like the Tauri shell) speaks this
// error shape.

/// Daemon-level errors.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("Document not found: {0}")]
    DocNotFound(String),

    #[error("Lock poisoned")]
    LockPoisoned,

    #[error("Loro error: {0}")]
    Loro(String),

    #[error("Luau error: {0}")]
    Luau(String),
}
