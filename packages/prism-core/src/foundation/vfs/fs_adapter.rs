use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::types::{FileStat, VfsAdapter};

#[derive(Serialize, Deserialize)]
struct BlobMeta {
    mime_type: String,
    size: usize,
    created_at: String,
}

pub struct FileSystemVfsAdapter {
    root: PathBuf,
    lock: Mutex<()>,
}

impl FileSystemVfsAdapter {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let _ = std::fs::create_dir_all(&root);
        Self {
            root,
            lock: Mutex::new(()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn blob_path(&self, hash: &str) -> PathBuf {
        self.root.join(hash)
    }

    fn meta_path(&self, hash: &str) -> PathBuf {
        self.root.join(format!("{hash}.meta"))
    }

    fn read_meta(&self, hash: &str) -> Option<BlobMeta> {
        let data = std::fs::read(self.meta_path(hash)).ok()?;
        serde_json::from_slice(&data).ok()
    }
}

impl VfsAdapter for FileSystemVfsAdapter {
    fn read(&self, hash: &str) -> Option<Vec<u8>> {
        std::fs::read(self.blob_path(hash)).ok()
    }

    fn write(&self, data: &[u8], mime_type: &str) -> String {
        let hash = hex::encode(Sha256::digest(data));
        let _guard = self.lock.lock().expect("vfs fs lock poisoned");

        let blob = self.blob_path(&hash);
        if !blob.exists() {
            let _ = std::fs::write(&blob, data);
            let meta = BlobMeta {
                mime_type: mime_type.to_string(),
                size: data.len(),
                created_at: Utc::now().to_rfc3339(),
            };
            if let Ok(json) = serde_json::to_vec(&meta) {
                let _ = std::fs::write(self.meta_path(&hash), json);
            }
        }

        hash
    }

    fn stat(&self, hash: &str) -> Option<FileStat> {
        let meta = self.read_meta(hash)?;
        let created = chrono::DateTime::parse_from_rfc3339(&meta.created_at)
            .ok()?
            .with_timezone(&Utc);
        let modified = std::fs::metadata(self.blob_path(hash))
            .ok()
            .and_then(|m| m.modified().ok())
            .map(chrono::DateTime::<Utc>::from)
            .unwrap_or(created);
        Some(FileStat {
            hash: hash.to_string(),
            size: meta.size,
            mime_type: meta.mime_type,
            created_at: created,
            modified_at: modified,
        })
    }

    fn list(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| !name.ends_with(".meta"))
            .collect()
    }

    fn delete(&self, hash: &str) -> bool {
        let removed = std::fs::remove_file(self.blob_path(hash)).is_ok();
        let _ = std::fs::remove_file(self.meta_path(hash));
        removed
    }

    fn has(&self, hash: &str) -> bool {
        self.blob_path(hash).exists()
    }

    fn count(&self) -> usize {
        self.list().len()
    }

    fn total_size(&self) -> usize {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return 0;
        };
        entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| !n.ends_with(".meta"))
                    .unwrap_or(false)
            })
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len() as usize)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!("prism-fs-vfs-{label}-{unique}"));
        dir
    }

    #[test]
    fn write_and_read() {
        let dir = temp_dir("write-read");
        let adapter = FileSystemVfsAdapter::new(&dir);
        let hash = adapter.write(b"hello world", "text/plain");
        assert!(!hash.is_empty());
        assert_eq!(adapter.read(&hash), Some(b"hello world".to_vec()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_is_idempotent() {
        let dir = temp_dir("idempotent");
        let adapter = FileSystemVfsAdapter::new(&dir);
        let h1 = adapter.write(b"data", "application/octet-stream");
        let h2 = adapter.write(b"data", "application/octet-stream");
        assert_eq!(h1, h2);
        assert_eq!(adapter.count(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stat_returns_metadata() {
        let dir = temp_dir("stat");
        let adapter = FileSystemVfsAdapter::new(&dir);
        let hash = adapter.write(b"pdf bytes", "application/pdf");
        let stat = adapter.stat(&hash).unwrap();
        assert_eq!(stat.hash, hash);
        assert_eq!(stat.size, 9);
        assert_eq!(stat.mime_type, "application/pdf");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_missing_returns_none() {
        let dir = temp_dir("missing");
        let adapter = FileSystemVfsAdapter::new(&dir);
        assert_eq!(adapter.read("nonexistent"), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_removes_blob_and_meta() {
        let dir = temp_dir("delete");
        let adapter = FileSystemVfsAdapter::new(&dir);
        let hash = adapter.write(b"remove me", "text/plain");
        assert!(adapter.has(&hash));
        assert!(adapter.delete(&hash));
        assert!(!adapter.has(&hash));
        assert!(!adapter.delete(&hash));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_and_count() {
        let dir = temp_dir("list");
        let adapter = FileSystemVfsAdapter::new(&dir);
        adapter.write(b"a", "text/plain");
        adapter.write(b"b", "text/plain");
        adapter.write(b"c", "text/plain");
        assert_eq!(adapter.count(), 3);
        assert_eq!(adapter.list().len(), 3);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn total_size() {
        let dir = temp_dir("size");
        let adapter = FileSystemVfsAdapter::new(&dir);
        adapter.write(b"aaaa", "text/plain");
        adapter.write(b"bb", "text/plain");
        assert_eq!(adapter.total_size(), 6);
        std::fs::remove_dir_all(&dir).ok();
    }
}
