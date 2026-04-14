//! Transport adapters — thin wrappers over [`crate::DaemonKernel::invoke`].
//!
//! The kernel itself is transport-agnostic: every capability lives behind a
//! single `invoke(name, payload)` entry point that takes JSON in and returns
//! JSON out. The adapters in this module wrap that entry point in whatever
//! shape a particular host expects:
//!
//! | Adapter           | Feature              | Use case                                            |
//! |-------------------|----------------------|-----------------------------------------------------|
//! | [`http_axum`]     | `transport-http`     | Headless desktop / server / dev tooling             |
//! | [`grpc_tonic`]    | `transport-grpc`     | Polyglot service mesh, language-agnostic clients    |
//! | [`uniffi_bridge`] | `transport-uniffi`   | Direct Swift/Kotlin bindings on iOS/Android         |
//! | [`ipc_local`]     | `transport-ipc`      | Tauri 2 no-webview desktop shell ↔ daemon sidecar   |
//!
//! Each adapter is a single file and is *strictly* a wrapper: no business
//! logic, no mutation of the registry, no tokio/grpc concepts leak through
//! the kernel boundary. The kernel is sync; adapters that live in async
//! runtimes (`http_axum`, `grpc_tonic`) hop onto a blocking pool via
//! [`tokio::task::spawn_blocking`] before calling `kernel.invoke`.

#[cfg(feature = "transport-http")]
pub mod http_axum;

#[cfg(feature = "transport-grpc")]
pub mod grpc_tonic;

#[cfg(feature = "transport-uniffi")]
pub mod uniffi_bridge;

#[cfg(feature = "transport-ipc")]
pub mod ipc_local;
