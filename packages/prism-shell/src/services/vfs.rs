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

/// Read/write/list/delete. Four methods cover every IO call any
/// service in §26/§27 makes.
pub trait Vfs: Send + Sync {
    fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError>;
    fn write(&mut self, path: &Path, data: &[u8]) -> Result<(), VfsError>;
    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, VfsError>;
    fn exists(&self, path: &Path) -> bool;
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
    }
}
