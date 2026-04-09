//! CRDT module — exposes [`DocManager`] behind four `crdt.*` commands.
//!
//! | Command        | Payload                                       | Result          |
//! |----------------|-----------------------------------------------|-----------------|
//! | `crdt.write`   | `{ docId, key, value }`                       | `Vec<u8>` bytes |
//! | `crdt.read`    | `{ docId, key }`                              | `Option<String>` |
//! | `crdt.export`  | `{ docId }`                                   | `Vec<u8>` bytes |
//! | `crdt.import`  | `{ docId, snapshot: base64 or Vec<u8> }`      | `null`           |
//!
//! All byte arrays are transferred as JSON arrays of numbers for the
//! registry's JSON-in/JSON-out contract. Transport adapters that have a
//! better representation (e.g. Tauri's `Vec<u8>` → `number[]`) can bypass
//! the registry and call [`DaemonKernel::doc_manager`] directly for hot
//! paths — the two entry points are deliberately symmetric.

use crate::builder::DaemonBuilder;
use crate::doc_manager::DocManager;
use crate::module::DaemonModule;
use crate::registry::CommandError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

/// The CRDT module. Stateless — the state lives on the shared
/// [`DocManager`] stashed into the builder.
pub struct CrdtModule;

impl DaemonModule for CrdtModule {
    fn id(&self) -> &str {
        "prism.crdt"
    }

    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        // Reuse an injected DocManager if the host already provided one,
        // otherwise spin a fresh one up.
        let manager = builder
            .doc_manager_slot()
            .get_or_insert_with(|| Arc::new(DocManager::new()))
            .clone();
        let registry = builder.registry().clone();

        let mgr = manager.clone();
        registry.register("crdt.write", move |payload| {
            let args: WriteArgs = parse(payload, "crdt.write")?;
            mgr.get_or_create(&args.doc_id);
            let bytes = mgr
                .write(&args.doc_id, &args.key, &args.value)
                .map_err(|e| CommandError::handler("crdt.write", e.to_string()))?;
            Ok(json!({ "bytes": bytes }))
        })?;

        let mgr = manager.clone();
        registry.register("crdt.read", move |payload| {
            let args: ReadArgs = parse(payload, "crdt.read")?;
            mgr.get_or_create(&args.doc_id);
            let value = mgr
                .read(&args.doc_id, &args.key)
                .map_err(|e| CommandError::handler("crdt.read", e.to_string()))?;
            Ok(json!({ "value": value }))
        })?;

        let mgr = manager.clone();
        registry.register("crdt.export", move |payload| {
            let args: ExportArgs = parse(payload, "crdt.export")?;
            mgr.get_or_create(&args.doc_id);
            let bytes = mgr
                .export_snapshot(&args.doc_id)
                .map_err(|e| CommandError::handler("crdt.export", e.to_string()))?;
            Ok(json!({ "bytes": bytes }))
        })?;

        let mgr = manager;
        registry.register("crdt.import", move |payload| {
            let args: ImportArgs = parse(payload, "crdt.import")?;
            mgr.import_snapshot(&args.doc_id, &args.snapshot)
                .map_err(|e| CommandError::handler("crdt.import", e.to_string()))?;
            Ok(JsonValue::Null)
        })?;

        Ok(())
    }
}

fn parse<T: for<'de> Deserialize<'de>>(
    payload: JsonValue,
    command: &str,
) -> Result<T, CommandError> {
    serde_json::from_value::<T>(payload)
        .map_err(|e| CommandError::handler(command.to_string(), e.to_string()))
}

#[derive(Debug, Deserialize, Serialize)]
struct WriteArgs {
    #[serde(rename = "docId")]
    doc_id: String,
    key: String,
    value: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReadArgs {
    #[serde(rename = "docId")]
    doc_id: String,
    key: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ExportArgs {
    #[serde(rename = "docId")]
    doc_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ImportArgs {
    #[serde(rename = "docId")]
    doc_id: String,
    snapshot: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;

    #[test]
    fn crdt_module_registers_four_commands() {
        let kernel = DaemonBuilder::new().with_crdt().build().unwrap();
        let caps = kernel.capabilities();
        assert!(caps.contains(&"crdt.write".to_string()));
        assert!(caps.contains(&"crdt.read".to_string()));
        assert!(caps.contains(&"crdt.export".to_string()));
        assert!(caps.contains(&"crdt.import".to_string()));
    }

    #[test]
    fn crdt_write_then_read_roundtrip() {
        let kernel = DaemonBuilder::new().with_crdt().build().unwrap();

        kernel
            .invoke(
                "crdt.write",
                json!({ "docId": "doc1", "key": "greeting", "value": "hello" }),
            )
            .unwrap();

        let out = kernel
            .invoke("crdt.read", json!({ "docId": "doc1", "key": "greeting" }))
            .unwrap();
        assert_eq!(out["value"], JsonValue::String("\"hello\"".to_string()));
    }

    #[test]
    fn crdt_export_then_import_restores_state() {
        let a = DaemonBuilder::new().with_crdt().build().unwrap();
        a.invoke(
            "crdt.write",
            json!({ "docId": "doc1", "key": "k", "value": "v" }),
        )
        .unwrap();
        let exported = a.invoke("crdt.export", json!({ "docId": "doc1" })).unwrap();
        let bytes: Vec<u8> = serde_json::from_value(exported["bytes"].clone()).unwrap();

        let b = DaemonBuilder::new().with_crdt().build().unwrap();
        b.invoke("crdt.import", json!({ "docId": "doc1", "snapshot": bytes }))
            .unwrap();
        let out = b
            .invoke("crdt.read", json!({ "docId": "doc1", "key": "k" }))
            .unwrap();
        assert_eq!(out["value"], JsonValue::String("\"v\"".to_string()));
    }

    #[test]
    fn crdt_module_reuses_injected_doc_manager() {
        let injected = Arc::new(DocManager::new());
        injected.get_or_create("preloaded");
        injected.write("preloaded", "k", "v").unwrap();

        let mut builder = DaemonBuilder::new();
        builder.set_doc_manager(injected.clone());
        let kernel = builder.with_crdt().build().unwrap();

        let out = kernel
            .invoke("crdt.read", json!({ "docId": "preloaded", "key": "k" }))
            .unwrap();
        assert_eq!(out["value"], JsonValue::String("\"v\"".to_string()));

        // Kernel's direct accessor hands back the very same Arc.
        let handle = kernel.doc_manager().unwrap();
        assert!(Arc::ptr_eq(&handle, &injected));
    }
}
