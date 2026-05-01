use std::path::{Path, PathBuf};

use super::vault_manager::PersistenceAdapter;

pub struct FileSystemAdapter {
    root: PathBuf,
}

impl FileSystemAdapter {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }
}

impl PersistenceAdapter for FileSystemAdapter {
    fn load(&self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.resolve(path)).ok()
    }

    fn save(&mut self, path: &str, data: &[u8]) {
        let full = self.resolve(path);
        if let Some(parent) = full.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(full, data);
    }

    fn delete(&mut self, path: &str) -> bool {
        std::fs::remove_file(self.resolve(path)).is_ok()
    }

    fn exists(&self, path: &str) -> bool {
        self.resolve(path).exists()
    }

    fn list(&self, directory: &str) -> Vec<String> {
        let dir = self.resolve(directory);
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort();
        names
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
        let dir = std::env::temp_dir().join(format!("prism-fs-adapter-{label}-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_and_load() {
        let dir = temp_dir("save-load");
        let mut adapter = FileSystemAdapter::new(&dir);
        adapter.save("test.bin", b"hello");
        assert_eq!(adapter.load("test.bin"), Some(b"hello".to_vec()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = temp_dir("parents");
        let mut adapter = FileSystemAdapter::new(&dir);
        adapter.save("data/collections/foo.loro", b"snapshot");
        assert!(adapter.exists("data/collections/foo.loro"));
        assert_eq!(
            adapter.load("data/collections/foo.loro"),
            Some(b"snapshot".to_vec())
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_returns_none() {
        let dir = temp_dir("missing");
        let adapter = FileSystemAdapter::new(&dir);
        assert_eq!(adapter.load("nope.bin"), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_returns_true_if_existed() {
        let dir = temp_dir("delete");
        let mut adapter = FileSystemAdapter::new(&dir);
        adapter.save("x.bin", b"data");
        assert!(adapter.delete("x.bin"));
        assert!(!adapter.exists("x.bin"));
        assert!(!adapter.delete("x.bin"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_returns_sorted_filenames() {
        let dir = temp_dir("list");
        let mut adapter = FileSystemAdapter::new(&dir);
        adapter.save("stuff/c.bin", b"c");
        adapter.save("stuff/a.bin", b"a");
        adapter.save("stuff/b.bin", b"b");
        let names = adapter.list("stuff");
        assert_eq!(names, vec!["a.bin", "b.bin", "c.bin"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_empty_dir_returns_empty() {
        let dir = temp_dir("list-empty");
        let adapter = FileSystemAdapter::new(&dir);
        assert!(adapter.list("nonexistent").is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn root_accessor() {
        let dir = temp_dir("root");
        let adapter = FileSystemAdapter::new(&dir);
        assert_eq!(adapter.root(), dir.as_path());
        std::fs::remove_dir_all(&dir).ok();
    }
}
