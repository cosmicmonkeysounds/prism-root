//! CRDT IPC commands for frontend <-> daemon Loro operations.

use crate::{DaemonError, DocManager};

/// Write a key-value pair to a CRDT document.
/// Called from frontend via: invoke('crdt_write', { docId, key, value })
pub fn crdt_write(
    manager: &DocManager,
    doc_id: &str,
    key: &str,
    value: &str,
) -> Result<Vec<u8>, DaemonError> {
    manager.get_or_create(doc_id);
    manager.write(doc_id, key, value)
}

/// Read a value from a CRDT document.
/// Called from frontend via: invoke('crdt_read', { docId, key })
pub fn crdt_read(
    manager: &DocManager,
    doc_id: &str,
    key: &str,
) -> Result<Option<String>, DaemonError> {
    manager.get_or_create(doc_id);
    manager.read(doc_id, key)
}

/// Export a document's full CRDT state as a snapshot.
/// Called from frontend via: invoke('crdt_export', { docId })
pub fn crdt_export(
    manager: &DocManager,
    doc_id: &str,
) -> Result<Vec<u8>, DaemonError> {
    manager.get_or_create(doc_id);
    manager.export_snapshot(doc_id)
}

/// Import a CRDT snapshot into the daemon.
/// Called from frontend via: invoke('crdt_import', { docId, snapshot })
pub fn crdt_import(
    manager: &DocManager,
    doc_id: &str,
    snapshot: &[u8],
) -> Result<(), DaemonError> {
    manager.import_snapshot(doc_id, snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crdt_write_and_read() {
        let manager = DocManager::new();
        let _update = crdt_write(&manager, "test-doc", "greeting", "hello").unwrap();
        let value = crdt_read(&manager, "test-doc", "greeting").unwrap();
        assert_eq!(value, Some("\"hello\"".to_string()));
    }

    #[test]
    fn test_crdt_export_and_import() {
        let manager = DocManager::new();
        crdt_write(&manager, "doc1", "key", "value").unwrap();
        let snapshot = crdt_export(&manager, "doc1").unwrap();

        let manager2 = DocManager::new();
        crdt_import(&manager2, "doc2", &snapshot).unwrap();
        let value = crdt_read(&manager2, "doc2", "key").unwrap();
        assert_eq!(value, Some("\"value\"".to_string()));
    }

    #[test]
    fn test_crdt_merge() {
        // Two peers edit independently, then merge
        let manager_a = DocManager::new();
        let manager_b = DocManager::new();

        crdt_write(&manager_a, "shared", "from_a", "hello").unwrap();
        crdt_write(&manager_b, "shared", "from_b", "world").unwrap();

        let snap_a = crdt_export(&manager_a, "shared").unwrap();
        let snap_b = crdt_export(&manager_b, "shared").unwrap();

        // Import B into A
        crdt_import(&manager_a, "shared", &snap_b).unwrap();
        // Import A into B
        crdt_import(&manager_b, "shared", &snap_a).unwrap();

        // Both should now have both keys
        let a_from_a = crdt_read(&manager_a, "shared", "from_a").unwrap();
        let a_from_b = crdt_read(&manager_a, "shared", "from_b").unwrap();
        assert_eq!(a_from_a, Some("\"hello\"".to_string()));
        assert_eq!(a_from_b, Some("\"world\"".to_string()));
    }
}
