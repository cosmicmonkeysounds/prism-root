//! C-ABI adapter — exposes [`DaemonKernel`] to every non-Rust host through
//! a small, hand-rolled `extern "C"` surface.
//!
//! This module has two consumers that share the same four functions:
//!
//! 1. **Browser** — compiled into `wasm32-unknown-emscripten` under the
//!    `wasm` feature. Emscripten wraps the C symbols automatically via
//!    `cwrap`/`ccall`, so the JS side just imports the generated
//!    `prism_daemon_wasm.js` and calls through `Module`.
//!
//! 2. **Mobile (iOS + Android)** — compiled as a `staticlib` under the
//!    `mobile` feature for `aarch64-apple-ios`, `aarch64-apple-ios-sim`,
//!    and the `*-linux-android` triples. The `prism-capacitor-daemon`
//!    package's Swift plugin (iOS) and Kotlin plugin (Android) each wrap
//!    the same four functions and expose them to Studio as a Capacitor
//!    plugin. Mobile does NOT go through emscripten — it links
//!    `libprism_daemon.a` directly into the native shell.
//!
//! Desktop (Tauri) speaks Rust natively and bypasses this adapter
//! entirely — `tauri::command` functions hold an `Arc<DaemonKernel>` and
//! call `kernel.invoke()` without ever crossing the C boundary.
//!
//! The module is named `wasm` for historical reasons (the browser build
//! came first), but every extern function in here is transport-neutral:
//! the same bytes flow through emscripten's `cwrap` and through Swift's
//! `UnsafeMutablePointer<CChar>`.
//!
//! ## Why a C ABI (and not `wasm-bindgen` / UniFFI)?
//!
//! The only WASM triple `mlua` (our Luau runtime) supports is
//! `wasm32-unknown-emscripten`: emscripten ships a libc, which the vendored
//! Luau C++ source needs to compile. `wasm-bindgen` targets
//! `wasm32-unknown-unknown` and produces its own JS glue, which does not
//! cohabit with emscripten's glue. So the "other WASM option" — a C ABI
//! surface that emscripten wraps automatically via `cwrap`/`ccall` — is
//! the one that actually lets us keep real Luau in the browser build.
//!
//! Pure-Rust Luau VMs would let us use `wasm-bindgen`, but there is no
//! production-ready equivalent of `mlua` for Luau. Going through emscripten
//! keeps the exact same mlua-backed runtime everywhere — desktop, mobile,
//! browser.
//!
//! ## Build
//!
//! ```sh
//! # one-time: install emscripten SDK + add the target
//! # (see https://emscripten.org/docs/getting_started/downloads.html)
//! source /path/to/emsdk/emsdk_env.sh
//! rustup target add wasm32-unknown-emscripten
//!
//! cargo build \
//!   --release \
//!   --target wasm32-unknown-emscripten \
//!   --no-default-features \
//!   --features wasm
//! ```
//!
//! Emscripten produces `prism_daemon.wasm` + a small `prism_daemon.js`
//! loader. Load it from any browser (Chrome, Firefox, Safari) the same
//! way you'd load any other emscripten module.
//!
//! ## JS usage
//!
//! ```js
//! import createModule from './prism_daemon.js';
//!
//! const Module = await createModule();
//! const invoke = Module.cwrap(
//!   'prism_daemon_invoke', 'number',
//!   ['number', 'string', 'string']
//! );
//! const freeString = Module.cwrap('prism_daemon_free_string', null, ['number']);
//!
//! const kernel = Module.ccall('prism_daemon_create', 'number', [], []);
//!
//! const resultPtr = invoke(
//!   kernel,
//!   'crdt.write',
//!   JSON.stringify({ docId: 'notes', key: 'title', value: 'Hello' }),
//! );
//! const response = JSON.parse(Module.UTF8ToString(resultPtr));
//! freeString(resultPtr);
//!
//! // response is { ok: true, result: ... } or { ok: false, error: "..." }
//! ```
//!
//! ## Ownership
//!
//! - `prism_daemon_create` hands out an owning `*mut DaemonKernel`. The
//!   caller must eventually pass it to `prism_daemon_destroy` (or leak it
//!   for the lifetime of the page, which is also fine — the browser will
//!   reclaim the WASM linear memory on reload).
//! - Every function that returns `*mut c_char` transfers ownership of a
//!   freshly-allocated, nul-terminated UTF-8 string to the caller. The
//!   caller must free it with `prism_daemon_free_string`. Never call
//!   `free()` on it directly; emscripten's allocator is not Rust's.

#![allow(clippy::missing_safety_doc)]

use crate::{DaemonBuilder, DaemonKernel};
use serde_json::json;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Encode a `serde_json::Value` as an owned, nul-terminated C string and
/// leak it to the caller. `prism_daemon_free_string` is the matching
/// deallocator.
fn into_leaked_cstring(value: &serde_json::Value) -> *mut c_char {
    let encoded = serde_json::to_string(value)
        .unwrap_or_else(|_| r#"{"ok":false,"error":"failed to encode response"}"#.to_string());
    // CString::new only fails if the string contains a nul byte. JSON
    // encoding never produces nul bytes, so `unwrap` is sound.
    CString::new(encoded)
        .expect("JSON encoding produced a nul byte")
        .into_raw()
}

/// Build a fresh [`DaemonKernel`] with every capability the `wasm` feature
/// enables (CRDT + Luau today). The returned pointer is owned by the caller
/// and must be released with [`prism_daemon_destroy`].
///
/// Returns `null` if kernel construction fails — which, given the current
/// module set, should only happen if two modules try to register the same
/// command name (a programmer error, not a runtime one).
#[no_mangle]
pub extern "C" fn prism_daemon_create() -> *mut DaemonKernel {
    match DaemonBuilder::new().with_defaults().build() {
        Ok(kernel) => Box::into_raw(Box::new(kernel)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Release a kernel returned by [`prism_daemon_create`]. Safe to call with
/// a null pointer (no-op). Calling it twice on the same pointer is
/// undefined behavior — don't do that.
#[no_mangle]
pub unsafe extern "C" fn prism_daemon_destroy(kernel: *mut DaemonKernel) {
    if kernel.is_null() {
        return;
    }
    let kernel = Box::from_raw(kernel);
    kernel.dispose();
    // `kernel` drops here.
}

/// Invoke a registered command.
///
/// - `kernel`: pointer from [`prism_daemon_create`]
/// - `name`: nul-terminated UTF-8 command name (e.g. `"crdt.write"`)
/// - `payload_json`: nul-terminated UTF-8 JSON string; may be `"null"`
///
/// Returns a freshly-allocated, nul-terminated UTF-8 JSON string in one
/// of two shapes, to be freed with [`prism_daemon_free_string`]:
///
/// ```json
/// { "ok": true,  "result": <command output> }
/// { "ok": false, "error":  "<message>" }
/// ```
///
/// Reserved command names that don't touch the registry:
///
/// | name                    | result shape                         |
/// |-------------------------|--------------------------------------|
/// | `daemon.capabilities`   | `{ "commands": [...] }`              |
/// | `daemon.modules`        | `{ "modules":  [...] }`              |
#[no_mangle]
pub unsafe extern "C" fn prism_daemon_invoke(
    kernel: *mut DaemonKernel,
    name: *const c_char,
    payload_json: *const c_char,
) -> *mut c_char {
    let response = (|| -> Result<serde_json::Value, String> {
        if kernel.is_null() {
            return Err("kernel pointer is null".to_string());
        }
        if name.is_null() {
            return Err("command name is null".to_string());
        }
        if payload_json.is_null() {
            return Err("payload pointer is null".to_string());
        }

        let kernel_ref: &DaemonKernel = &*kernel;
        let name = CStr::from_ptr(name)
            .to_str()
            .map_err(|e| format!("command name is not valid UTF-8: {e}"))?;
        let payload_str = CStr::from_ptr(payload_json)
            .to_str()
            .map_err(|e| format!("payload is not valid UTF-8: {e}"))?;
        let payload: serde_json::Value = serde_json::from_str(payload_str)
            .map_err(|e| format!("payload is not valid JSON: {e}"))?;

        match name {
            "daemon.capabilities" => Ok(json!({ "commands": kernel_ref.capabilities() })),
            "daemon.modules" => Ok(json!({ "modules": kernel_ref.installed_modules() })),
            other => kernel_ref.invoke(other, payload).map_err(|e| e.to_string()),
        }
    })();

    let envelope = match response {
        Ok(result) => json!({ "ok": true, "result": result }),
        Err(error) => json!({ "ok": false, "error": error }),
    };
    into_leaked_cstring(&envelope)
}

/// Free a string previously returned by any `prism_daemon_*` function.
/// Safe to call with a null pointer (no-op).
#[no_mangle]
pub unsafe extern "C" fn prism_daemon_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    drop(CString::from_raw(ptr));
}

// ── Tests ───────────────────────────────────────────────────────────────
//
// These run on the host as regular unit tests (`cargo test --features wasm`).
// They drive the same C ABI an emscripten-wrapped JS caller would, so we
// exercise the ownership dance (create → invoke → free → destroy) without
// needing an actual browser.

#[cfg(test)]
mod tests {
    use super::*;

    fn cstr(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    unsafe fn take_response(ptr: *mut c_char) -> serde_json::Value {
        assert!(!ptr.is_null(), "expected non-null response pointer");
        let parsed: serde_json::Value =
            serde_json::from_str(CStr::from_ptr(ptr).to_str().unwrap()).unwrap();
        prism_daemon_free_string(ptr);
        parsed
    }

    #[test]
    fn create_destroy_is_idempotent_safe() {
        unsafe {
            let k = prism_daemon_create();
            assert!(!k.is_null());
            prism_daemon_destroy(k);
            // A second destroy would be UB — we just verify the null path.
            prism_daemon_destroy(std::ptr::null_mut());
        }
    }

    #[test]
    fn daemon_capabilities_reserved_command_lists_installed() {
        unsafe {
            let kernel = prism_daemon_create();
            let name = cstr("daemon.capabilities");
            let payload = cstr("null");
            let resp = prism_daemon_invoke(kernel, name.as_ptr(), payload.as_ptr());
            let value = take_response(resp);
            assert_eq!(value["ok"], serde_json::Value::Bool(true));
            let commands = value["result"]["commands"].as_array().unwrap();
            assert!(!commands.is_empty(), "expected at least one command");
            prism_daemon_destroy(kernel);
        }
    }

    #[test]
    fn daemon_modules_reserved_command_lists_installed() {
        unsafe {
            let kernel = prism_daemon_create();
            let name = cstr("daemon.modules");
            let payload = cstr("null");
            let resp = prism_daemon_invoke(kernel, name.as_ptr(), payload.as_ptr());
            let value = take_response(resp);
            assert_eq!(value["ok"], serde_json::Value::Bool(true));
            let modules = value["result"]["modules"].as_array().unwrap();
            assert!(!modules.is_empty());
            prism_daemon_destroy(kernel);
        }
    }

    #[cfg(feature = "crdt")]
    #[test]
    fn crdt_write_then_read_roundtrips_through_the_c_abi() {
        unsafe {
            let kernel = prism_daemon_create();

            let write_name = cstr("crdt.write");
            let write_payload =
                cstr(r#"{"docId":"notes","key":"title","value":"Hello from WASM"}"#);
            let write_resp =
                prism_daemon_invoke(kernel, write_name.as_ptr(), write_payload.as_ptr());
            let write_value = take_response(write_resp);
            assert_eq!(write_value["ok"], serde_json::Value::Bool(true));

            let read_name = cstr("crdt.read");
            let read_payload = cstr(r#"{"docId":"notes","key":"title"}"#);
            let read_resp = prism_daemon_invoke(kernel, read_name.as_ptr(), read_payload.as_ptr());
            let read_value = take_response(read_resp);
            assert_eq!(read_value["ok"], serde_json::Value::Bool(true));
            // DocManager stores values as JSON-encoded strings, so the
            // round-trip shape is a string literal with embedded quotes.
            assert_eq!(read_value["result"]["value"], "\"Hello from WASM\"");

            prism_daemon_destroy(kernel);
        }
    }

    #[cfg(feature = "luau")]
    #[test]
    fn luau_exec_runs_through_the_c_abi() {
        unsafe {
            let kernel = prism_daemon_create();
            let name = cstr("luau.exec");
            let payload = cstr(r#"{"script":"return 21 * 2"}"#);
            let resp = prism_daemon_invoke(kernel, name.as_ptr(), payload.as_ptr());
            let value = take_response(resp);
            assert_eq!(value["ok"], serde_json::Value::Bool(true));
            assert_eq!(value["result"], 42);
            prism_daemon_destroy(kernel);
        }
    }

    #[test]
    fn invalid_json_payload_returns_error_envelope() {
        unsafe {
            let kernel = prism_daemon_create();
            let name = cstr("crdt.read");
            let payload = cstr("not valid json");
            let resp = prism_daemon_invoke(kernel, name.as_ptr(), payload.as_ptr());
            let value = take_response(resp);
            assert_eq!(value["ok"], serde_json::Value::Bool(false));
            assert!(value["error"].as_str().unwrap().contains("JSON"));
            prism_daemon_destroy(kernel);
        }
    }

    #[test]
    fn null_kernel_pointer_returns_error_envelope() {
        unsafe {
            let name = cstr("daemon.capabilities");
            let payload = cstr("null");
            let resp = prism_daemon_invoke(std::ptr::null_mut(), name.as_ptr(), payload.as_ptr());
            let value = take_response(resp);
            assert_eq!(value["ok"], serde_json::Value::Bool(false));
            assert!(value["error"].as_str().unwrap().contains("null"));
        }
    }
}
