//! Tauri IPC command bindings for the Studio shell.
//! These are thin wrappers delegating to prism-daemon.

use prism_daemon::DocManager;
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
