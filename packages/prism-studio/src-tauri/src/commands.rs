//! Tauri IPC bindings — thin wrappers over the Prism Daemon kernel.
//!
//! These preserve the existing frontend-facing command surface (crdt_*,
//! lua_exec, run_build_step) while routing through the kernel instead of
//! touching the services directly. Hot paths that have a natural Rust
//! shape (e.g. CRDT byte arrays) still call `kernel.doc_manager()` for
//! zero-copy access; the rest funnel through `kernel.invoke(...)` which
//! is the same entry point every transport (Tauri / UniFFI / CLI / HTTP)
//! funnels through.

use prism_daemon::modules::build_module::{run_build_step as daemon_run_build_step, BuildStep, BuildStepOutput};
use prism_daemon::modules::lua_module::exec as daemon_lua_exec;
use prism_daemon::DaemonKernel;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

type Kernel<'a> = State<'a, Arc<DaemonKernel>>;

#[tauri::command]
pub fn crdt_write(
    kernel: Kernel<'_>,
    doc_id: String,
    key: String,
    value: String,
) -> Result<Vec<u8>, String> {
    let dm = kernel
        .doc_manager()
        .ok_or_else(|| "crdt module not installed".to_string())?;
    dm.get_or_create(&doc_id);
    dm.write(&doc_id, &key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn crdt_read(
    kernel: Kernel<'_>,
    doc_id: String,
    key: String,
) -> Result<Option<String>, String> {
    let dm = kernel
        .doc_manager()
        .ok_or_else(|| "crdt module not installed".to_string())?;
    dm.get_or_create(&doc_id);
    dm.read(&doc_id, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn crdt_export(kernel: Kernel<'_>, doc_id: String) -> Result<Vec<u8>, String> {
    let dm = kernel
        .doc_manager()
        .ok_or_else(|| "crdt module not installed".to_string())?;
    dm.get_or_create(&doc_id);
    dm.export_snapshot(&doc_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn lua_exec(
    script: String,
    args: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    daemon_lua_exec(&script, args.as_ref())
}

/// Execute a single BuildStep from Studio's BuilderManager.
///
/// `step` is a JSON-serialized BuildStep from @prism/core/builder.
/// `workingDir` (camelCase on the JS side) is `BuildPlan.workingDir`.
/// `env` is `BuildPlan.env` merged into the child process. Errors
/// propagate as rejected Promises to the Tauri executor.
#[tauri::command(rename_all = "camelCase")]
pub fn run_build_step(
    step: BuildStep,
    working_dir: Option<String>,
    env: Option<HashMap<String, String>>,
) -> Result<BuildStepOutput, String> {
    let cwd = working_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let env_map = env.unwrap_or_default();
    daemon_run_build_step(&step, &cwd, &env_map)
}

/// Introspect the kernel: which modules are loaded, which commands are
/// registered. Useful for debugging and for the Studio status bar.
#[tauri::command]
pub fn daemon_capabilities(kernel: Kernel<'_>) -> serde_json::Value {
    serde_json::json!({
        "modules": kernel.installed_modules(),
        "commands": kernel.capabilities(),
    })
}
