//! UniFFI bridge — direct Swift/Kotlin bindings for the daemon kernel.
//!
//! Where the C ABI in [`crate::wasm`] is hand-written and string-based,
//! UniFFI generates *typed* bindings: every Swift / Kotlin call into the
//! daemon goes through `PrismDaemonHandle.invoke(command, payloadJson)`
//! which returns a typed `Result` whose error variants you can pattern
//! match on natively. The wire format inside that call is still JSON
//! (the kernel's lingua franca) — UniFFI just gives the host language a
//! nicer envelope around it.
//!
//! ### How it gets used
//!
//! 1. Build the daemon as a `cdylib` with `--features transport-uniffi`.
//! 2. Run `uniffi-bindgen generate --library libprism_daemon.dylib
//!    --language swift --out-dir Generated/` (and again for Kotlin).
//! 3. Drop the generated `prism_daemon.swift` / `prism_daemon.kt` into
//!    the host app, link the dylib, and call:
//!
//!    ```swift
//!    let kernel = PrismDaemonHandle.withDefaults()
//!    let json = try kernel.invoke(command: "vfs.put", payloadJson: …)
//!    ```
//!
//! ### Why a separate bridge from the C ABI
//!
//! The C ABI in `src/wasm.rs` is the lowest common denominator — every
//! host (browser, iOS staticlib, Android staticlib) can call it. The
//! UniFFI bridge is the *ergonomic* path for the two hosts that natively
//! speak typed bindings. They coexist; nothing forces a host to pick
//! one over the other.

use crate::builder::DaemonBuilder;
use crate::kernel::DaemonKernel;
use crate::registry::CommandError;
use std::sync::Arc;

// NOTE: `uniffi::setup_scaffolding!()` lives at the crate root in
// `lib.rs` (uniffi requires its `UniFfiTag` marker to be visible from
// the crate root). Everything else — types, exports, derives — stays
// in this module so the bridge surface is still browsable from one
// file.

/// Typed error surface exposed to Swift/Kotlin. Each variant maps 1:1
/// to a [`CommandError`] variant — the host language sees them as
/// pattern-matchable cases instead of opaque strings.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum PrismDaemonError {
    #[error("command not found: {name}")]
    CommandNotFound { name: String },

    #[error("command already registered: {name}")]
    CommandAlreadyRegistered { name: String },

    #[error("handler error in {command}: {message}")]
    HandlerError { command: String, message: String },

    #[error("registry lock poisoned")]
    LockPoisoned,

    #[error("invalid JSON payload: {message}")]
    InvalidPayload { message: String },

    #[error("kernel build failed: {message}")]
    BuildFailed { message: String },
}

impl From<CommandError> for PrismDaemonError {
    fn from(e: CommandError) -> Self {
        match e {
            CommandError::NotFound(name) => PrismDaemonError::CommandNotFound { name },
            CommandError::AlreadyRegistered { command } => {
                PrismDaemonError::CommandAlreadyRegistered { name: command }
            }
            CommandError::Handler { command, message } => {
                PrismDaemonError::HandlerError { command, message }
            }
            CommandError::LockPoisoned => PrismDaemonError::LockPoisoned,
        }
    }
}

/// Opaque handle to a built [`DaemonKernel`]. Mirrors the
/// `PrismDaemon` Swift class consumers will see post-codegen.
#[derive(uniffi::Object)]
pub struct PrismDaemonHandle {
    kernel: DaemonKernel,
}

#[uniffi::export]
impl PrismDaemonHandle {
    /// Build a daemon with every capability the current Cargo feature
    /// set enabled. Mirrors `DaemonBuilder::new().with_defaults().build()`.
    /// Most mobile callers want this — they linked the dylib with the
    /// `mobile` feature set, which already trims out the modules that
    /// don't make sense on iOS/Android (process spawning, notify).
    #[uniffi::constructor]
    pub fn with_defaults() -> Result<Arc<Self>, PrismDaemonError> {
        let kernel = DaemonBuilder::new().with_defaults().build().map_err(|e| {
            PrismDaemonError::BuildFailed {
                message: e.to_string(),
            }
        })?;
        Ok(Arc::new(Self { kernel }))
    }

    /// Build an empty daemon — no modules installed. Useful for tests
    /// and for hosts that want to install a custom module set from the
    /// Rust side via FFI extension points.
    #[uniffi::constructor]
    pub fn empty() -> Result<Arc<Self>, PrismDaemonError> {
        let kernel = DaemonBuilder::new()
            .build()
            .map_err(|e| PrismDaemonError::BuildFailed {
                message: e.to_string(),
            })?;
        Ok(Arc::new(Self { kernel }))
    }

    /// Run a command. The payload is a JSON string (anything
    /// `serde_json::from_str` accepts); the result is the JSON string
    /// the handler returned. The host language's JSON encoder is the
    /// other end of the rope.
    pub fn invoke(
        &self,
        command: String,
        payload_json: String,
    ) -> Result<String, PrismDaemonError> {
        let payload: serde_json::Value = if payload_json.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(&payload_json).map_err(|e| PrismDaemonError::InvalidPayload {
                message: e.to_string(),
            })?
        };
        let value = self.kernel.invoke(&command, payload)?;
        serde_json::to_string(&value).map_err(|e| PrismDaemonError::InvalidPayload {
            message: e.to_string(),
        })
    }

    /// Sorted list of every registered command name.
    pub fn capabilities(&self) -> Vec<String> {
        self.kernel.capabilities()
    }

    /// IDs of every installed module, in install order.
    pub fn installed_modules(&self) -> Vec<String> {
        self.kernel.installed_modules().to_vec()
    }

    /// Tear the kernel down — releases every initializer in reverse
    /// order. Subsequent invokes still work because the registry stays
    /// alive; this only undoes post-boot side effects.
    pub fn dispose(&self) {
        self.kernel.dispose();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_kernel_has_no_capabilities() {
        let handle = PrismDaemonHandle::empty().unwrap();
        assert!(handle.capabilities().is_empty());
        assert!(handle.installed_modules().is_empty());
    }

    #[test]
    fn with_defaults_registers_at_least_one_command() {
        let handle = PrismDaemonHandle::with_defaults().unwrap();
        let caps = handle.capabilities();
        assert!(
            !caps.is_empty(),
            "with_defaults() should register at least one command in any feature set"
        );
    }

    #[test]
    fn invoke_unknown_command_maps_to_not_found_variant() {
        let handle = PrismDaemonHandle::empty().unwrap();
        let err = handle
            .invoke("nope.nada".into(), "null".into())
            .unwrap_err();
        match err {
            PrismDaemonError::CommandNotFound { name } => assert_eq!(name, "nope.nada"),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn invoke_invalid_payload_json_maps_to_invalid_payload_variant() {
        let handle = PrismDaemonHandle::empty().unwrap();
        let err = handle.invoke("nope".into(), "not json".into()).unwrap_err();
        assert!(matches!(err, PrismDaemonError::InvalidPayload { .. }));
    }

    #[test]
    fn empty_payload_string_is_treated_as_json_null() {
        // Even with an empty kernel, the empty-string -> null path
        // shouldn't error out before reaching the registry — it should
        // bubble up as CommandNotFound, proving the JSON layer accepted
        // the empty input.
        let handle = PrismDaemonHandle::empty().unwrap();
        let err = handle.invoke("anything".into(), "".into()).unwrap_err();
        assert!(matches!(err, PrismDaemonError::CommandNotFound { .. }));
    }

    #[cfg(feature = "vfs")]
    #[test]
    fn invoke_vfs_stats_through_default_kernel_returns_json_string() {
        let handle = PrismDaemonHandle::with_defaults().unwrap();
        let result = handle
            .invoke("vfs.stats".into(), "{}".into())
            .expect("vfs.stats should succeed on a default kernel");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("entries").is_some());
        assert!(parsed.get("total_bytes").is_some());
        assert!(parsed.get("backend").is_some());
    }
}
