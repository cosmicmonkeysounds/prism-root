//! Filesystem seam — the single trait every IO-bearing service
//! borrows through `MutCtx::vfs`. `OsVfs` is the production impl
//! (stdlib `std::fs`); tests build an `InMemVfs` (private to the
//! tests that need one) and slot it in via `MutCtx`.
//!
//! No service constructs an `OsVfs` directly — `ShellInner` owns
//! the one instance and lends `&mut dyn Vfs` into every dispatch.
//! This is the §26 IO discipline: services declare *what* the user
//! asks for, the host wires *where* that lands.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("io: {0}")]
    Io(String),
    #[error("not found: {0}")]
    NotFound(String),
    /// Wave 2.5 of `docs/dev/composable-builder-plan.md` — a
    /// `Vfs::pick_file` / `pick_save` call landed on a backend that
    /// has no native dialog (wasm, headless tests, or any host that
    /// chose not to wire one). Callers treat this as "user cancelled"
    /// for the purposes of UI state; the toast is only emitted when
    /// the *user* explicitly asked for a picker.
    #[error("dialog unsupported on this platform")]
    Unsupported,
    /// Wave 2.5 — the user closed the dialog without selecting.
    /// Indistinguishable from an empty multi-select; callers default
    /// to "noop" rather than emitting an error toast.
    #[error("dialog cancelled")]
    Cancelled,
}

/// Wave 2.5 — descriptor for a file-picker invocation. The fields
/// mirror the cross-platform subset `rfd::FileDialog` exposes; any
/// concrete picker (rfd today, web `<input type=file>` later) accepts
/// the same shape.
#[derive(Debug, Clone, Default)]
pub struct FilePickerSpec {
    /// Window title. Empty → backend default.
    pub title: String,
    /// Optional "Filter Name" + extension list (without dots).
    /// e.g. `("Images", &["png", "jpg", "jpeg"])`.
    pub filters: Vec<(String, Vec<String>)>,
    /// Initial directory hint. Empty → backend default.
    pub start_dir: Option<PathBuf>,
    /// Pre-filled filename (save dialogs).
    pub start_file_name: String,
    /// Allow multi-select (open dialogs only). Ignored for save.
    pub multiple: bool,
    /// If true, present a save-as picker (one path, writable).
    /// Open pickers default to false.
    pub save: bool,
}

impl FilePickerSpec {
    pub fn open(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    pub fn save(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            save: true,
            ..Default::default()
        }
    }

    pub fn with_filter(mut self, name: impl Into<String>, exts: &[&str]) -> Self {
        self.filters
            .push((name.into(), exts.iter().map(|s| (*s).to_string()).collect()));
        self
    }

    pub fn with_accept_attr(mut self, accept: &str) -> Self {
        // Parse a comma-joined list of MIME types / extension globs
        // (the `data-accept` shape `<input type="file" accept="…">`
        // uses). Bare extensions (`.png`) and explicit globs (`*.png`)
        // are coalesced under one "Files" filter so the dialog stays
        // single-row. MIME types pass through unchanged where the
        // platform supports them.
        let mut exts: Vec<String> = Vec::new();
        for tok in accept.split(',') {
            let tok = tok.trim();
            if tok.is_empty() {
                continue;
            }
            if let Some(stripped) = tok.strip_prefix("*.") {
                exts.push(stripped.to_string());
            } else if let Some(stripped) = tok.strip_prefix('.') {
                exts.push(stripped.to_string());
            }
        }
        if !exts.is_empty() {
            let refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            self = self.with_filter("Files", &refs);
        }
        self
    }
}

impl From<std::io::Error> for VfsError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            VfsError::NotFound(e.to_string())
        } else {
            VfsError::Io(e.to_string())
        }
    }
}

/// Read/write/list/delete plus the Wave 2.5 native dialog seam.
/// Five methods cover every IO call any service in §26/§27 makes.
pub trait Vfs: Send + Sync {
    fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError>;
    fn write(&mut self, path: &Path, data: &[u8]) -> Result<(), VfsError>;
    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, VfsError>;
    fn exists(&self, path: &Path) -> bool;

    /// Present a native open/save file dialog and return the picked
    /// path(s). Default impl returns [`VfsError::Unsupported`] — only
    /// the production [`OsVfs`] overrides this (and only on native
    /// targets; wasm builds inherit the default).
    fn pick_file(&self, _spec: &FilePickerSpec) -> Result<Vec<PathBuf>, VfsError> {
        Err(VfsError::Unsupported)
    }
}

#[derive(Default)]
pub struct OsVfs;

impl Vfs for OsVfs {
    fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        Ok(std::fs::read(path)?)
    }

    fn write(&mut self, path: &Path, data: &[u8]) -> Result<(), VfsError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        Ok(std::fs::write(path, data)?)
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, VfsError> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path)? {
            out.push(entry?.path());
        }
        Ok(out)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    /// Wave 2.5 — native picker. Bridges to `rfd` when the
    /// `desktop-dialogs` half of the `native` feature is on and the
    /// target isn't wasm; falls through to the trait default
    /// otherwise (so a `--no-default-features` build, or a future
    /// embedded host, stays compiling).
    #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
    fn pick_file(&self, spec: &FilePickerSpec) -> Result<Vec<PathBuf>, VfsError> {
        let mut dlg = rfd::FileDialog::new();
        if !spec.title.is_empty() {
            dlg = dlg.set_title(&spec.title);
        }
        if let Some(dir) = &spec.start_dir {
            dlg = dlg.set_directory(dir);
        }
        if !spec.start_file_name.is_empty() {
            dlg = dlg.set_file_name(&spec.start_file_name);
        }
        for (name, exts) in &spec.filters {
            let ext_refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            dlg = dlg.add_filter(name, &ext_refs);
        }
        let picked: Vec<PathBuf> = if spec.save {
            dlg.save_file().into_iter().collect()
        } else if spec.multiple {
            dlg.pick_files().unwrap_or_default()
        } else {
            dlg.pick_file().into_iter().collect()
        };
        if picked.is_empty() {
            Err(VfsError::Cancelled)
        } else {
            Ok(picked)
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::collections::HashMap;

    /// Tiny in-memory `Vfs` for service tests. Owns the bytes; clones
    /// on read so callers can't mutate the store.
    #[derive(Default)]
    pub struct InMemVfs {
        files: HashMap<PathBuf, Vec<u8>>,
        /// Optional canned response for `pick_file`. The first call
        /// consumes the value; subsequent calls fall back to
        /// `VfsError::Unsupported` (the trait default). Used by D3
        /// tests to drive the picker-aware persistence flow without
        /// pulling in `rfd`. `Mutex` rather than `RefCell` so
        /// `InMemVfs` keeps the `Vfs: Send + Sync` bound.
        pick_response: std::sync::Mutex<Option<Result<Vec<PathBuf>, VfsError>>>,
    }

    impl InMemVfs {
        /// Queue a single canned picker response. Subsequent
        /// `pick_file` calls without a queued response fall back to
        /// the trait default (`Err(Unsupported)`).
        pub fn queue_pick(&self, response: Result<Vec<PathBuf>, VfsError>) {
            *self.pick_response.lock().unwrap() = Some(response);
        }
    }

    impl Vfs for InMemVfs {
        fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| VfsError::NotFound(path.display().to_string()))
        }

        fn write(&mut self, path: &Path, data: &[u8]) -> Result<(), VfsError> {
            self.files.insert(path.to_path_buf(), data.to_vec());
            Ok(())
        }

        fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, VfsError> {
            let prefix = path.to_path_buf();
            Ok(self
                .files
                .keys()
                .filter(|p| p.starts_with(&prefix) && p != &&prefix)
                .cloned()
                .collect())
        }

        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path)
        }

        fn pick_file(&self, _spec: &FilePickerSpec) -> Result<Vec<PathBuf>, VfsError> {
            if let Some(r) = self.pick_response.lock().unwrap().take() {
                r
            } else {
                Err(VfsError::Unsupported)
            }
        }
    }
}
