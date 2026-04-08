//! Tauri IPC command bindings for the Studio shell.
//! These are thin wrappers delegating to prism-daemon.

use prism_daemon::commands::build::{run_build_step as daemon_run_build_step, BuildStep, BuildStepOutput};
use prism_daemon::DocManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn crdt_write(
    manager: State<'_, Arc<DocManager>>,
    doc_id: String,
    key: String,
    value: String,
) -> Result<Vec<u8>, String> {
    manager.get_or_create(&doc_id);
    manager.write(&doc_id, &key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn crdt_read(
    manager: State<'_, Arc<DocManager>>,
    doc_id: String,
    key: String,
) -> Result<Option<String>, String> {
    manager.get_or_create(&doc_id);
    manager.read(&doc_id, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn crdt_export(
    manager: State<'_, Arc<DocManager>>,
    doc_id: String,
) -> Result<Vec<u8>, String> {
    manager.get_or_create(&doc_id);
    manager.export_snapshot(&doc_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn lua_exec(
    script: String,
    args: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    prism_daemon::commands::lua::lua_exec(&script, args.as_ref())
}

/// Execute a single BuildStep from Studio's BuilderManager.
///
/// `step` is a JSON-serialized BuildStep from @prism/core/builder.
/// `workingDir` (camelCase on the JS side, `working_dir` on the Rust
/// side thanks to Tauri's default rename) is `BuildPlan.workingDir` —
/// absolute, or relative to the daemon's cwd. `env` is `BuildPlan.env`
/// merged into the child process. Returns captured stdout/stderr. Errors
/// propagate as rejected Promises to the Tauri executor, which marks the
/// step as `failed`.
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
