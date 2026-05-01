# Project Vault — Folder-Based Object Graph for Prism

## Problem

Prism Core ships a full object graph stack — `CollectionStore` (Loro CRDT), `VaultManager`
(lazy-load/save orchestrator), `VfsManager` (content-addressed blob store), `PrismManifest`
(`.prism.json` workspace definition) — but the shell uses none of it. The `CollectionStore`
we wired into facet resolution is ephemeral (in-memory, empty on boot). There is no way to:

- Point an app at a project folder on disk
- Have files in that folder appear as objects in the graph
- Persist the object graph across sessions
- Persist binary assets (images, PDFs) across sessions

The daemon ships a recursive directory watcher (`WatcherManager` via `notify` crate) and a
local filesystem VFS backend, but neither is connected to the shell.

## Solution: Project Vault

A **Project Vault** is a folder on disk that Prism manages as a workspace. The folder
contains a `.prism.json` manifest, collection data (`.loro` snapshots), and a VFS blob
store. Files added to the folder are automatically ingested into the object graph.

### On-disk layout

```
~/Desktop/my-project/
├── .prism.json                  # PrismManifest
├── data/
│   ├── collections/
│   │   └── default.loro         # Loro snapshot (CollectionStore)
│   └── vfs/
│       ├── a1b2c3...            # SHA-256-keyed binary blobs
│       └── d4e5f6...
├── report.pdf                   # User files — auto-ingested
├── photo.jpg
└── notes.md
```

### Flow

1. User opens a folder via File → Open Project (or `--project` CLI flag)
2. Shell reads `.prism.json` — if missing, creates a default manifest
3. `FileSystemAdapter` (new) wraps the folder for `PersistenceAdapter` I/O
4. `VaultManager` is constructed with the manifest + adapter
5. Default collection is opened → `CollectionStore` is live, persistent
6. `FileSystemVfsAdapter` (new) wraps `data/vfs/` for `VfsAdapter` blob I/O
7. Shell's `VfsManager` is reconfigured with the filesystem adapter
8. Daemon's `WatcherManager` starts watching the project folder
9. Existing files are scanned and ingested as `GraphObject`s (type `"file"`)
10. New/modified/removed files trigger incremental updates to the graph
11. Dirty collections auto-save periodically (every 30s or on significant mutation)

### File → GraphObject mapping

When a file is detected (initial scan or watcher event):

```rust
GraphObject {
    id: ObjectId::new(deterministic_id_from_path),
    type_name: "file",
    name: "report.pdf",          // filename
    parent_id: None,             // or folder ObjectId for nested dirs
    position: 0.0,
    status: None,
    tags: vec![],
    date: None,
    description: String::new(),
    color: None,
    image: None,                 // thumbnail hash for images
    pinned: false,
    data: {
        "path": "report.pdf",    // relative to project root
        "hash": "a1b2c3...",     // SHA-256 content hash (VFS key)
        "mimeType": "application/pdf",
        "size": 1048576,
        "extension": "pdf",
    },
    created_at: file_created_time,
    updated_at: file_modified_time,
    deleted_at: None,            // set when file is removed
}
```

Deterministic IDs: `sha256("file:" + relative_path)` truncated to 16 hex chars. This means
the same file at the same path always gets the same ObjectId — renames are a delete + create.

### What this enables

- **ObjectQuery facets** — `entity_type: "file"` with `filter: "extension == pdf"` shows
  all PDFs in the project as cards/rows/tiles
- **Lookup facets** — edges between files and other entities (tags, collections, people)
- **Explorer panel** — can show real files from the project folder
- **Search** — files are searchable by name, type, tags
- **VFS persistence** — binary assets survive across sessions

## New types

### `FileSystemAdapter` (prism-core, behind `crdt` feature)

Implements `PersistenceAdapter` for local filesystem I/O. Paths are resolved relative to
a root directory. Creates parent directories on write. Used by `VaultManager` to persist
`.loro` collection snapshots.

```rust
pub struct FileSystemAdapter {
    root: PathBuf,
}

impl FileSystemAdapter {
    pub fn new(root: impl Into<PathBuf>) -> Self;
    pub fn root(&self) -> &Path;
}

impl PersistenceAdapter for FileSystemAdapter { ... }
```

### `FileSystemVfsAdapter` (prism-core)

Implements `VfsAdapter` for local filesystem blob storage. Blobs are stored as flat files
under `{root}/{hash}` (no subdirectory sharding for simplicity — content-addressed so no
conflicts). Used by `VfsManager` for persistent binary asset storage.

```rust
pub struct FileSystemVfsAdapter {
    root: PathBuf,
}

impl FileSystemVfsAdapter {
    pub fn new(root: impl Into<PathBuf>) -> Self;
}

impl VfsAdapter for FileSystemVfsAdapter { ... }
```

### `ProjectManager` (prism-shell, native only)

Orchestrates the vault lifecycle on `ShellInner`. Holds the `VaultManager`, watcher handle,
and auto-save timer. Exposed via `Shell::open_project(path)` and `Shell::close_project()`.

```rust
pub struct ProjectManager {
    root: PathBuf,
    vault: VaultManager<FileSystemAdapter>,
    watcher_id: Option<u64>,
    auto_save_timer: Timer,
}
```

## Phasing

| Phase | What | Status |
|-------|------|--------|
| **V1** | `FileSystemAdapter` + `FileSystemVfsAdapter` in prism-core | This PR |
| **V1** | `ProjectManager` in shell: open/close/save, initial file scan | This PR |
| **V2** | Watcher integration: live file change → graph updates | Next |
| **V2** | Explorer panel: show project files alongside document tree | Next |
| **V3** | Folder hierarchy: nested dirs as parent/child GraphObjects | Later |
| **V3** | Thumbnail generation for image files | Later |
