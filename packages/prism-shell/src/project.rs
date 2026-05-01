#[cfg(feature = "native")]
use std::collections::BTreeMap;
#[cfg(feature = "native")]
use std::path::{Path, PathBuf};

#[cfg(feature = "native")]
use chrono::Utc;
#[cfg(feature = "native")]
use prism_core::foundation::object_model::types::{GraphObject, ObjectId};
#[cfg(feature = "native")]
use prism_core::foundation::persistence::{
    CollectionStore, FileSystemAdapter, ObjectFilter, PersistenceError, VaultManager,
};
#[cfg(feature = "native")]
use prism_core::foundation::vfs::{FileSystemVfsAdapter, VfsManager};
#[cfg(feature = "native")]
use prism_core::identity::manifest::{
    add_collection, default_manifest, CollectionRef, PrismManifest, MANIFEST_FILENAME,
};
#[cfg(feature = "native")]
use sha2::{Digest, Sha256};

#[cfg(feature = "native")]
const DEFAULT_COLLECTION: &str = "default";

#[cfg(feature = "native")]
pub struct ProjectManager {
    root: PathBuf,
    vault: VaultManager<FileSystemAdapter>,
    vfs: VfsManager,
}

#[cfg(feature = "native")]
impl ProjectManager {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, ProjectError> {
        let root: PathBuf = root.into();
        if !root.is_dir() {
            return Err(ProjectError::NotADirectory(root));
        }

        let manifest_path = root.join(MANIFEST_FILENAME);
        let manifest = if manifest_path.exists() {
            let content = std::fs::read_to_string(&manifest_path).map_err(ProjectError::Io)?;
            serde_json::from_str::<PrismManifest>(&content).map_err(ProjectError::Manifest)?
        } else {
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "project".into());
            let id = uuid::Uuid::new_v4().to_string();
            let m = default_manifest(&name, &id);
            let cref = CollectionRef::new(DEFAULT_COLLECTION, "Default");
            let m = add_collection(&m, cref).expect("fresh manifest has no collections");
            let json = serde_json::to_string_pretty(&m).map_err(ProjectError::Manifest)?;
            std::fs::write(&manifest_path, json).map_err(ProjectError::Io)?;
            m
        };

        let adapter = FileSystemAdapter::new(&root);
        let mut vault = VaultManager::new(manifest, adapter);

        vault
            .open_collection(DEFAULT_COLLECTION)
            .map_err(ProjectError::Persistence)?;

        let vfs_root = root.join("data").join("vfs");
        let fs_vfs = FileSystemVfsAdapter::new(&vfs_root);
        let vfs = VfsManager::with_adapter(Box::new(fs_vfs));

        let mut mgr = Self { root, vault, vfs };
        mgr.scan_files()?;
        Ok(mgr)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn collection(&mut self) -> &mut CollectionStore {
        self.vault
            .open_collection(DEFAULT_COLLECTION)
            .expect("default collection opened at construction")
    }

    pub fn vfs(&self) -> &VfsManager {
        &self.vfs
    }

    pub fn vfs_mut(&mut self) -> &mut VfsManager {
        &mut self.vfs
    }

    pub fn save(&mut self) -> Result<Vec<String>, ProjectError> {
        self.vault.save_all().map_err(ProjectError::Persistence)
    }

    pub fn is_dirty(&self) -> bool {
        self.vault.is_dirty(DEFAULT_COLLECTION)
    }

    fn scan_files(&mut self) -> Result<(), ProjectError> {
        let entries = collect_project_files(&self.root);
        let collection = self
            .vault
            .open_collection(DEFAULT_COLLECTION)
            .map_err(ProjectError::Persistence)?;

        let existing: std::collections::HashSet<String> = collection
            .list_objects(Some(&ObjectFilter {
                types: Some(vec!["file".into()]),
                ..Default::default()
            }))
            .iter()
            .map(|o| o.id.as_str().to_string())
            .collect();

        let mut seen = std::collections::HashSet::new();

        for entry in &entries {
            let rel = entry
                .strip_prefix(&self.root)
                .unwrap_or(entry)
                .to_string_lossy()
                .replace('\\', "/");

            let id = file_object_id(&rel);
            seen.insert(id.as_str().to_string());

            if existing.contains(id.as_str()) {
                continue;
            }

            let ext = entry
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();
            let mime = super::app::mime_from_extension(&ext);
            let filename = entry
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let meta = std::fs::metadata(entry).ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);

            let bytes = std::fs::read(entry).ok();
            let hash = bytes
                .as_ref()
                .map(|b| self.vfs.import_file(b, &filename, mime).hash);

            let mut data = BTreeMap::new();
            data.insert("path".into(), serde_json::Value::String(rel));
            if let Some(h) = &hash {
                data.insert("hash".into(), serde_json::Value::String(h.clone()));
            }
            data.insert(
                "mimeType".into(),
                serde_json::Value::String(mime.to_string()),
            );
            data.insert("size".into(), serde_json::json!(size));
            data.insert("extension".into(), serde_json::Value::String(ext));

            let now = Utc::now();
            let obj = GraphObject {
                id,
                type_name: "file".into(),
                name: filename,
                parent_id: None,
                position: 0.0,
                status: None,
                tags: Vec::new(),
                date: None,
                end_date: None,
                description: String::new(),
                color: None,
                image: None,
                pinned: false,
                data,
                created_at: now,
                updated_at: now,
                deleted_at: None,
            };

            let _ = collection.put_object(&obj);
        }

        // Soft-delete objects whose files no longer exist
        for id_str in &existing {
            if !seen.contains(id_str) {
                let oid = ObjectId::new(id_str);
                if let Some(mut obj) = collection.get_object(&oid) {
                    if obj.deleted_at.is_none() {
                        obj.deleted_at = Some(Utc::now());
                        let _ = collection.put_object(&obj);
                    }
                }
            }
        }

        Ok(())
    }

    pub fn ingest_file(&mut self, path: &Path) -> Result<Option<ObjectId>, ProjectError> {
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if should_skip_path(&rel) {
            return Ok(None);
        }

        let id = file_object_id(&rel);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        let mime = super::app::mime_from_extension(&ext);
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let meta = std::fs::metadata(path).ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);

        let bytes = std::fs::read(path).map_err(ProjectError::Io)?;
        let bref = self.vfs.import_file(&bytes, &filename, mime);

        let mut data = BTreeMap::new();
        data.insert("path".into(), serde_json::Value::String(rel));
        data.insert("hash".into(), serde_json::Value::String(bref.hash));
        data.insert(
            "mimeType".into(),
            serde_json::Value::String(mime.to_string()),
        );
        data.insert("size".into(), serde_json::json!(size));
        data.insert("extension".into(), serde_json::Value::String(ext));

        let now = Utc::now();
        let obj = GraphObject {
            id: id.clone(),
            type_name: "file".into(),
            name: filename,
            parent_id: None,
            position: 0.0,
            status: None,
            tags: Vec::new(),
            date: None,
            end_date: None,
            description: String::new(),
            color: None,
            image: None,
            pinned: false,
            data,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };

        let collection = self
            .vault
            .open_collection(DEFAULT_COLLECTION)
            .map_err(ProjectError::Persistence)?;
        collection
            .put_object(&obj)
            .map_err(ProjectError::Persistence)?;

        Ok(Some(id))
    }

    pub fn remove_file(&mut self, path: &Path) -> Result<(), ProjectError> {
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let id = file_object_id(&rel);
        let collection = self
            .vault
            .open_collection(DEFAULT_COLLECTION)
            .map_err(ProjectError::Persistence)?;
        if let Some(mut obj) = collection.get_object(&id) {
            if obj.deleted_at.is_none() {
                obj.deleted_at = Some(Utc::now());
                collection
                    .put_object(&obj)
                    .map_err(ProjectError::Persistence)?;
            }
        }
        Ok(())
    }
}

#[cfg(feature = "native")]
fn file_object_id(relative_path: &str) -> ObjectId {
    let digest = Sha256::digest(format!("file:{relative_path}").as_bytes());
    ObjectId::new(hex::encode(&digest[..8]))
}

#[cfg(feature = "native")]
fn should_skip_path(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    parts.iter().any(|p| {
        p.starts_with('.') || *p == "data" || *p == "node_modules" || *p == "target" || *p == ".git"
    })
}

#[cfg(feature = "native")]
fn collect_project_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_recursive(root, root, &mut files);
    files
}

#[cfg(feature = "native")]
fn collect_recursive(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        if should_skip_path(&rel) {
            continue;
        }

        if path.is_dir() {
            collect_recursive(root, &path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

#[cfg(feature = "native")]
#[derive(Debug)]
pub enum ProjectError {
    NotADirectory(PathBuf),
    Io(std::io::Error),
    Manifest(serde_json::Error),
    Persistence(PersistenceError),
}

#[cfg(feature = "native")]
impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotADirectory(p) => write!(f, "Not a directory: {}", p.display()),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Manifest(e) => write!(f, "Manifest error: {e}"),
            Self::Persistence(e) => write!(f, "Persistence error: {e}"),
        }
    }
}

#[cfg(feature = "native")]
impl std::error::Error for ProjectError {}

#[cfg(test)]
#[cfg(feature = "native")]
mod tests {
    use super::*;

    fn temp_project(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!("prism-project-{label}-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn open_creates_manifest_if_missing() {
        let dir = temp_project("create-manifest");
        let mgr = ProjectManager::open(&dir).unwrap();
        assert!(dir.join(MANIFEST_FILENAME).exists());
        drop(mgr);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_loads_existing_manifest() {
        let dir = temp_project("load-manifest");
        let id = uuid::Uuid::new_v4().to_string();
        let m = default_manifest("Test", &id);
        let m = add_collection(&m, CollectionRef::new("default", "Default")).unwrap();
        let json = serde_json::to_string_pretty(&m).unwrap();
        std::fs::write(dir.join(MANIFEST_FILENAME), json).unwrap();

        let mgr = ProjectManager::open(&dir).unwrap();
        assert_eq!(mgr.vault.manifest().name, "Test");
        drop(mgr);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scan_ingests_existing_files() {
        let dir = temp_project("scan");
        std::fs::write(dir.join("readme.md"), "# Hello").unwrap();
        std::fs::write(dir.join("photo.png"), b"fake-png").unwrap();

        let mut mgr = ProjectManager::open(&dir).unwrap();
        let objects = mgr.collection().list_objects(Some(&ObjectFilter {
            types: Some(vec!["file".into()]),
            exclude_deleted: true,
            ..Default::default()
        }));
        assert_eq!(objects.len(), 2);

        let names: Vec<&str> = objects.iter().map(|o| o.name.as_str()).collect();
        assert!(names.contains(&"readme.md"));
        assert!(names.contains(&"photo.png"));

        drop(mgr);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scan_skips_dotfiles_and_data_dir() {
        let dir = temp_project("skip");
        std::fs::write(dir.join("visible.txt"), "yes").unwrap();
        std::fs::write(dir.join(".hidden"), "no").unwrap();
        std::fs::create_dir_all(dir.join("data/vfs")).unwrap();
        std::fs::write(dir.join("data/vfs/abc123"), "blob").unwrap();

        let mut mgr = ProjectManager::open(&dir).unwrap();
        let objects = mgr.collection().list_objects(Some(&ObjectFilter {
            types: Some(vec!["file".into()]),
            exclude_deleted: true,
            ..Default::default()
        }));
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].name, "visible.txt");

        drop(mgr);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ingest_file_creates_object_and_vfs_blob() {
        let dir = temp_project("ingest");
        let mut mgr = ProjectManager::open(&dir).unwrap();

        std::fs::write(dir.join("new.pdf"), b"pdf-content").unwrap();
        let id = mgr.ingest_file(&dir.join("new.pdf")).unwrap().unwrap();

        let obj = mgr.collection().get_object(&id).unwrap();
        assert_eq!(obj.type_name, "file");
        assert_eq!(obj.name, "new.pdf");
        let hash = obj.data.get("hash").unwrap().as_str().unwrap();
        assert!(mgr.vfs().adapter().has(hash));

        drop(mgr);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remove_file_soft_deletes() {
        let dir = temp_project("remove");
        std::fs::write(dir.join("temp.txt"), "gone soon").unwrap();
        let mut mgr = ProjectManager::open(&dir).unwrap();

        let objects = mgr.collection().list_objects(Some(&ObjectFilter {
            types: Some(vec!["file".into()]),
            exclude_deleted: true,
            ..Default::default()
        }));
        assert_eq!(objects.len(), 1);

        mgr.remove_file(&dir.join("temp.txt")).unwrap();

        let objects = mgr.collection().list_objects(Some(&ObjectFilter {
            types: Some(vec!["file".into()]),
            exclude_deleted: true,
            ..Default::default()
        }));
        assert_eq!(objects.len(), 0);

        drop(mgr);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_persists_to_disk() {
        let dir = temp_project("save");
        std::fs::write(dir.join("doc.txt"), "content").unwrap();
        let mut mgr = ProjectManager::open(&dir).unwrap();
        mgr.save().unwrap();

        assert!(dir.join("data/collections/default.loro").exists());

        drop(mgr);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reopen_preserves_objects() {
        let dir = temp_project("reopen");
        std::fs::write(dir.join("keep.txt"), "persist me").unwrap();

        {
            let mut mgr = ProjectManager::open(&dir).unwrap();
            mgr.save().unwrap();
        }

        {
            let mut mgr = ProjectManager::open(&dir).unwrap();
            let objects = mgr.collection().list_objects(Some(&ObjectFilter {
                types: Some(vec!["file".into()]),
                exclude_deleted: true,
                ..Default::default()
            }));
            assert_eq!(objects.len(), 1);
            assert_eq!(objects[0].name, "keep.txt");
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deterministic_file_ids() {
        let id1 = file_object_id("readme.md");
        let id2 = file_object_id("readme.md");
        let id3 = file_object_id("other.txt");
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn not_a_directory_errors() {
        let result = ProjectManager::open("/nonexistent/path/that/should/not/exist");
        assert!(result.is_err());
    }
}
