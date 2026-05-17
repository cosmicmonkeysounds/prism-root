//! `ProjectManager` — folder-backed object graph for the shell.
//!
//! Implements `docs/dev/project-vault.md`. A *Project Vault* is a
//! folder on disk Prism manages as a workspace: a `.prism.json`
//! manifest, `.loro` collection snapshots under `data/collections/`,
//! and a content-addressed blob store under `data/vfs/`. Files in the
//! folder are ingested into the live `CollectionStore` as
//! `GraphObject`s (type `"file"` / `"folder"`), and a recursive
//! `notify` watcher keeps the graph in sync with disk.
//!
//! Native-only: the persistent stack (`VaultManager` / `CollectionStore`)
//! rides the `crdt` feature which the shell's `native` feature pulls
//! in. The wasm matrix never compiles this module.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sha2::{Digest, Sha256};

use prism_core::foundation::object_model::{object_id, GraphObject, ObjectId};
use prism_core::foundation::persistence::{CollectionStore, FileSystemAdapter, VaultManager};
use prism_core::foundation::vfs::{FileSystemVfsAdapter, VfsManager};
use prism_core::identity::manifest::{
    add_collection, default_manifest, parse_manifest, serialise_manifest, CollectionRef,
    PrismManifest, MANIFEST_FILENAME,
};

use crate::state::{FileKind, FileNode};

/// The single default collection every vault opens. V1 ships one
/// collection; multi-collection vaults are a later phase.
const DEFAULT_COLLECTION: &str = "default";

/// Auto-save cadence: a dirty collection is flushed at most this
/// often by [`ProjectManager::poll`].
const AUTO_SAVE_INTERVAL: Duration = Duration::from_secs(30);

/// Directory / file names skipped by the scanner — Prism's own
/// `data/` store, VCS metadata, build output, and dependency trees.
fn is_skipped(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "target" | "node_modules" | "data")
}

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

fn mime_for(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "md" | "markdown" => "text/markdown",
        "txt" | "log" => "text/plain",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "rs" => "text/rust",
        "toml" => "text/toml",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

/// `sha256("<prefix><relative_path>")` truncated to 16 hex chars.
/// Stable across runs so the same path always maps to the same
/// `ObjectId` — a rename is a delete + create.
fn deterministic_id(prefix: &str, rel: &str) -> ObjectId {
    let mut h = Sha256::new();
    h.update(prefix.as_bytes());
    h.update(rel.as_bytes());
    object_id(hex::encode(h.finalize())[..16].to_string())
}

fn rel_to_string(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Orchestrates one open project: manifest + vault + VFS + watcher.
pub struct ProjectManager {
    root: PathBuf,
    vault: VaultManager<FileSystemAdapter>,
    vfs: VfsManager,
    collection_id: String,
    /// Held so its `Drop` tears the OS watch down when the project
    /// closes. `None` in headless tests that skip live watching.
    _watcher: Option<RecommendedWatcher>,
    rx: Option<Receiver<PathBuf>>,
    last_save: Instant,
}

impl ProjectManager {
    /// Open `root` as a project vault. Reads (or creates) the
    /// manifest, hydrates the default collection from disk, ingests
    /// every file as a `GraphObject`, persists, and starts the
    /// recursive change watcher.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, String> {
        Self::open_inner(root.into(), true)
    }

    /// Same as [`Self::open`] but skips the live `notify` watcher.
    /// Used by tests that drive ingestion deterministically.
    pub fn open_headless(root: impl Into<PathBuf>) -> Result<Self, String> {
        Self::open_inner(root.into(), false)
    }

    fn open_inner(root: PathBuf, watch: bool) -> Result<Self, String> {
        if !root.is_dir() {
            return Err(format!("not a directory: {}", root.display()));
        }
        let manifest = load_or_create_manifest(&root)?;
        let adapter = FileSystemAdapter::new(&root);
        let mut vault = VaultManager::new(manifest, adapter);
        // Hydrate (or create) the default collection from
        // `data/collections/default.loro`.
        vault
            .open_collection(DEFAULT_COLLECTION)
            .map_err(|e| format!("open collection: {e}"))?;

        let vfs = VfsManager::with_adapter(Box::new(FileSystemVfsAdapter::new(
            root.join("data").join("vfs"),
        )));

        let (watcher, rx) = if watch {
            let (w, r) = spawn_watcher(&root)?;
            (Some(w), Some(r))
        } else {
            (None, None)
        };

        let mut pm = Self {
            root,
            vault,
            vfs,
            collection_id: DEFAULT_COLLECTION.to_string(),
            _watcher: watcher,
            rx,
            last_save: Instant::now(),
        };
        pm.ingest_all()?;
        pm.save()?;
        Ok(pm)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn collection(&mut self) -> &mut CollectionStore {
        self.vault
            .open_collection(&self.collection_id)
            .expect("default collection opened in ::open")
    }

    /// Every object currently in the live graph.
    pub fn objects(&mut self) -> Vec<GraphObject> {
        self.collection().all_objects()
    }

    /// Force-flush the dirty collection to disk.
    pub fn save(&mut self) -> Result<(), String> {
        self.vault
            .save_all()
            .map(|_| ())
            .map_err(|e| format!("save: {e}"))?;
        self.last_save = Instant::now();
        Ok(())
    }

    /// Drain pending filesystem events, apply them to the graph, and
    /// auto-save if the cadence elapsed. Returns `true` when the
    /// object graph changed (the explorer / canvas should re-render).
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        let paths: Vec<PathBuf> = match &self.rx {
            Some(rx) => rx.try_iter().collect(),
            None => Vec::new(),
        };
        for path in paths {
            if self.apply_path_change(&path) {
                changed = true;
            }
        }
        if changed && self.last_save.elapsed() >= AUTO_SAVE_INTERVAL {
            let _ = self.save();
        }
        changed
    }

    /// Re-ingest a single path after a watcher event. Deletes that
    /// race the read collapse to a soft-delete (`deleted_at`).
    fn apply_path_change(&mut self, abs: &Path) -> bool {
        let Ok(rel) = abs.strip_prefix(&self.root) else {
            return false;
        };
        if rel
            .components()
            .any(|c| is_skipped(&c.as_os_str().to_string_lossy()))
        {
            return false;
        }
        let rel_str = rel_to_string(rel);
        if rel_str.is_empty() {
            return false;
        }
        if abs.is_dir() {
            let parent = rel
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(folder_id_for);
            let obj = folder_object(&rel_str, abs, parent);
            let _ = self.collection().put_object(&obj);
            return true;
        }
        if abs.is_file() {
            match self.ingest_file(rel, abs) {
                Ok(_) => return true,
                Err(_) => return false,
            }
        }
        // Path no longer exists → removed. Soft-delete the object.
        let id = deterministic_id("file:", &rel_str);
        let coll = self.collection();
        if let Some(mut obj) = coll.get_object(&id) {
            obj.deleted_at = Some(chrono_now());
            let _ = coll.put_object(&obj);
            return true;
        }
        let fid = deterministic_id("folder:", &rel_str);
        if let Some(mut obj) = coll.get_object(&fid) {
            obj.deleted_at = Some(chrono_now());
            let _ = coll.put_object(&obj);
            return true;
        }
        false
    }

    /// Full recursive scan. Folders become `folder` objects (parent
    /// chain wired for V3 hierarchy); files become `file` objects
    /// with content hashed into the VFS and image thumbnails
    /// generated.
    fn ingest_all(&mut self) -> Result<(), String> {
        let root = self.root.clone();
        let mut stack: Vec<(PathBuf, Option<ObjectId>)> = vec![(root.clone(), None)];
        while let Some((dir, parent)) = stack.pop() {
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if is_skipped(&name) {
                    continue;
                }
                let rel = path.strip_prefix(&root).unwrap_or(&path);
                let rel_str = rel_to_string(rel);
                let ft = match entry.file_type() {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };
                if ft.is_dir() {
                    let obj = folder_object(&rel_str, &path, parent.clone());
                    let fid = obj.id.clone();
                    self.collection()
                        .put_object(&obj)
                        .map_err(|e| format!("put folder: {e}"))?;
                    stack.push((path, Some(fid)));
                } else if ft.is_file() {
                    self.ingest_file_with_parent(rel, &path, parent.clone())?;
                }
            }
        }
        Ok(())
    }

    fn ingest_file(&mut self, rel: &Path, abs: &Path) -> Result<(), String> {
        let parent = rel
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(folder_id_for);
        self.ingest_file_with_parent(rel, abs, parent)
    }

    fn ingest_file_with_parent(
        &mut self,
        rel: &Path,
        abs: &Path,
        parent: Option<ObjectId>,
    ) -> Result<(), String> {
        let rel_str = rel_to_string(rel);
        let name = rel
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| rel_str.clone());
        let ext = rel
            .extension()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let bytes = std::fs::read(abs).map_err(|e| format!("read {}: {e}", abs.display()))?;
        let size = bytes.len();
        let mime = mime_for(&ext);

        // Content-addressed VFS persistence: binary assets survive
        // across sessions. Idempotent — re-importing identical bytes
        // is a no-op write.
        let asset = self.vfs.import_file(&bytes, &name, mime);
        let hash = asset.hash.clone();

        // V3 — image thumbnails. Decode + downscale to <=256px, store
        // the thumbnail as its own VFS blob, reference it on `image`.
        let thumb = if IMAGE_EXTS.contains(&ext.as_str()) {
            generate_thumbnail(&bytes)
                .map(|png| self.vfs.import_file(&png, "thumb.png", "image/png").hash)
        } else {
            None
        };

        let id = deterministic_id("file:", &rel_str);
        let mut obj = GraphObject::new(id, "file", name);
        obj.parent_id = parent;
        obj.image = thumb;
        obj.data
            .insert("path".into(), serde_json::Value::String(rel_str));
        obj.data
            .insert("hash".into(), serde_json::Value::String(hash));
        obj.data.insert(
            "mimeType".into(),
            serde_json::Value::String(mime.to_string()),
        );
        obj.data
            .insert("size".into(), serde_json::Value::Number(size.into()));
        obj.data
            .insert("extension".into(), serde_json::Value::String(ext));
        self.collection()
            .put_object(&obj)
            .map_err(|e| format!("put file: {e}"))
    }

    /// Pre-order explorer tree (folders before files, each group
    /// alpha-sorted) with depth derived from the parent chain.
    pub fn file_nodes(&mut self) -> Vec<FileNode> {
        let root = self.root.clone();
        let objs = self.objects();
        let mut by_parent: std::collections::BTreeMap<String, Vec<GraphObject>> =
            std::collections::BTreeMap::new();
        for o in objs {
            if o.deleted_at.is_some() {
                continue;
            }
            let key = o
                .parent_id
                .as_ref()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            by_parent.entry(key).or_default().push(o);
        }
        let mut out = Vec::new();
        emit_nodes(&by_parent, "", 0, &root, &mut out);
        out
    }
}

fn emit_nodes(
    by_parent: &std::collections::BTreeMap<String, Vec<GraphObject>>,
    parent_key: &str,
    depth: u32,
    root: &Path,
    out: &mut Vec<FileNode>,
) {
    let Some(children) = by_parent.get(parent_key) else {
        return;
    };
    let mut sorted = children.clone();
    sorted.sort_by(|a, b| {
        let a_dir = a.type_name == "folder";
        let b_dir = b.type_name == "folder";
        b_dir.cmp(&a_dir).then_with(|| a.name.cmp(&b.name))
    });
    for o in sorted {
        let rel = o
            .data
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(&o.name)
            .to_string();
        let kind = if o.type_name == "folder" {
            FileKind::Directory
        } else {
            FileKind::File
        };
        out.push(FileNode {
            id: rel.clone(),
            label: o.name.clone(),
            depth,
            kind,
            path: root.join(&rel),
        });
        if matches!(kind, FileKind::Directory) {
            emit_nodes(by_parent, o.id.as_str(), depth + 1, root, out);
        }
    }
}

fn folder_id_for(rel: &Path) -> ObjectId {
    deterministic_id("folder:", &rel_to_string(rel))
}

fn folder_object(rel_str: &str, abs: &Path, parent: Option<ObjectId>) -> GraphObject {
    let name = abs
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel_str.to_string());
    let id = deterministic_id("folder:", rel_str);
    let mut obj = GraphObject::new(id, "folder", name);
    obj.parent_id = parent;
    obj.data.insert(
        "path".into(),
        serde_json::Value::String(rel_str.to_string()),
    );
    obj
}

fn chrono_now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

/// Decode arbitrary image bytes and downscale to a ≤256px PNG.
/// Returns `None` for formats the decoder can't read.
fn generate_thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(bytes).ok()?;
    let thumb = img.thumbnail(256, 256);
    let mut out = std::io::Cursor::new(Vec::new());
    thumb.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// Read `<root>/.prism.json`, or synthesise a default manifest and
/// write it. Either way the returned manifest carries a `default`
/// collection ref so [`VaultManager::open_collection`] resolves.
fn load_or_create_manifest(root: &Path) -> Result<PrismManifest, String> {
    let path = root.join(MANIFEST_FILENAME);
    let mut manifest = if path.exists() {
        let raw = std::fs::read_to_string(&path).map_err(|e| format!("read manifest: {e}"))?;
        parse_manifest(&raw).map_err(|e| format!("parse manifest: {e}"))?
    } else {
        let name = root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Project".to_string());
        default_manifest(
            name,
            format!(
                "vault-{}",
                deterministic_id("root:", &root.to_string_lossy()).as_str()
            ),
        )
    };
    let has_default = manifest
        .collections
        .as_ref()
        .map(|c| c.iter().any(|c| c.id == DEFAULT_COLLECTION))
        .unwrap_or(false);
    if !has_default {
        manifest = add_collection(&manifest, CollectionRef::new(DEFAULT_COLLECTION, "Default"))
            .map_err(|e| format!("add default collection: {e}"))?;
    }
    let serialised =
        serialise_manifest(&manifest).map_err(|e| format!("serialise manifest: {e}"))?;
    std::fs::write(&path, serialised).map_err(|e| format!("write manifest: {e}"))?;
    Ok(manifest)
}

/// Spawn a recursive `notify` watcher on `root`, forwarding changed
/// paths to an mpsc channel the shell drains on its own thread.
fn spawn_watcher(root: &Path) -> Result<(RecommendedWatcher, Receiver<PathBuf>), String> {
    let (tx, rx) = channel::<PathBuf>();
    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                ) {
                    for p in event.paths {
                        let _ = tx.send(p);
                    }
                }
            }
        })
        .map_err(|e| format!("notify::recommended_watcher: {e}"))?;
    watcher
        .watch(root, RecursiveMode::Recursive)
        .map_err(|e| format!("notify watch {}: {e}", root.display()))?;
    Ok((watcher, rx))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("prism-pm-{label}-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn open_creates_manifest_and_ingests_files() {
        let dir = temp_dir("open");
        std::fs::write(dir.join("notes.md"), b"# hello").unwrap();
        std::fs::write(dir.join("report.pdf"), b"%PDF-1.4 fake").unwrap();

        let mut pm = ProjectManager::open_headless(&dir).unwrap();
        assert!(dir.join(".prism.json").exists());
        assert!(dir.join("data/collections/default.loro").exists());

        let objs = pm.objects();
        let names: Vec<_> = objs.iter().map(|o| o.name.clone()).collect();
        assert!(names.contains(&"notes.md".to_string()));
        assert!(names.contains(&"report.pdf".to_string()));

        let pdf = objs.iter().find(|o| o.name == "report.pdf").unwrap();
        assert_eq!(pdf.type_name, "file");
        assert_eq!(pdf.data["extension"], serde_json::json!("pdf"));
        assert_eq!(pdf.data["mimeType"], serde_json::json!("application/pdf"));
        assert!(pdf.data.get("hash").and_then(|v| v.as_str()).is_some());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deterministic_ids_are_stable_across_reopen() {
        let dir = temp_dir("stable");
        std::fs::write(dir.join("a.txt"), b"a").unwrap();
        let id1 = {
            let mut pm = ProjectManager::open_headless(&dir).unwrap();
            pm.objects()
                .iter()
                .find(|o| o.name == "a.txt")
                .unwrap()
                .id
                .clone()
        };
        let id2 = {
            let mut pm = ProjectManager::open_headless(&dir).unwrap();
            pm.objects()
                .iter()
                .find(|o| o.name == "a.txt")
                .unwrap()
                .id
                .clone()
        };
        assert_eq!(id1, id2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn nested_dirs_become_folder_objects_with_parent_chain() {
        let dir = temp_dir("nested");
        std::fs::create_dir_all(dir.join("src/inner")).unwrap();
        std::fs::write(dir.join("src/inner/deep.rs"), b"fn main(){}").unwrap();

        let mut pm = ProjectManager::open_headless(&dir).unwrap();
        let objs = pm.objects();
        let src = objs.iter().find(|o| o.name == "src").unwrap();
        assert_eq!(src.type_name, "folder");
        assert!(src.parent_id.is_none());
        let inner = objs.iter().find(|o| o.name == "inner").unwrap();
        assert_eq!(inner.parent_id.as_ref(), Some(&src.id));
        let deep = objs.iter().find(|o| o.name == "deep.rs").unwrap();
        assert_eq!(deep.parent_id.as_ref(), Some(&inner.id));

        let nodes = pm.file_nodes();
        // Pre-order: src(0) → inner(1) → deep.rs(2)
        let labels: Vec<_> = nodes.iter().map(|n| (n.label.clone(), n.depth)).collect();
        assert_eq!(
            labels,
            vec![
                ("src".to_string(), 0),
                ("inner".to_string(), 1),
                ("deep.rs".to_string(), 2),
            ]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn data_dir_is_skipped() {
        let dir = temp_dir("skip");
        std::fs::write(dir.join("keep.txt"), b"k").unwrap();
        let mut pm = ProjectManager::open_headless(&dir).unwrap();
        let objs = pm.objects();
        assert!(objs.iter().all(|o| {
            o.data
                .get("path")
                .and_then(|v| v.as_str())
                .map(|p| !p.starts_with("data"))
                .unwrap_or(true)
        }));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reopen_hydrates_persisted_graph() {
        let dir = temp_dir("hydrate");
        std::fs::write(dir.join("one.txt"), b"1").unwrap();
        {
            let mut pm = ProjectManager::open_headless(&dir).unwrap();
            assert_eq!(
                pm.objects()
                    .iter()
                    .filter(|o| o.type_name == "file")
                    .count(),
                1
            );
        }
        // Second open with no rescan delta still sees the persisted
        // object (loaded from the .loro snapshot before re-ingest).
        let mut pm = ProjectManager::open_headless(&dir).unwrap();
        let one = pm.objects().into_iter().find(|o| o.name == "one.txt");
        assert!(one.is_some());
        std::fs::remove_dir_all(&dir).ok();
    }
}
