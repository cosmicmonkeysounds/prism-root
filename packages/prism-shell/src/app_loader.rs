//! `AppLoader` — filesystem discovery for `apps/<id>/manifest.toml`.
//!
//! Pure data hand-off: walks the `apps/` directory, parses each
//! manifest via [`prism_core::AppManifest::parse`], and returns the
//! hydrated set. The pure-data half lives in `prism_core::app` so
//! the relay / wasm builds can construct manifests from memory
//! without a filesystem dependency.
//!
//! On the wasm target the loader returns an empty list — discovery
//! is host-driven on the desktop shell, and the browser shell will
//! receive its app set through a future relay channel.

use std::path::{Path, PathBuf};

use prism_core::{AppManifest, AppManifestError};

/// One app hydrated from disk.
#[derive(Clone, Debug)]
pub struct LoadedApp {
    pub manifest: AppManifest,
    /// Absolute path to the app's directory (the parent of `manifest.toml`).
    pub base_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum AppLoaderError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest in {path}: {source}")]
    Manifest {
        path: PathBuf,
        #[source]
        source: AppManifestError,
    },
}

/// Discover and parse every `apps/*/manifest.toml` under `root`. If
/// `root` doesn't exist, returns an empty list — callers fall back to
/// hardcoded defaults.
#[cfg(not(target_arch = "wasm32"))]
pub fn discover(root: impl AsRef<Path>) -> Result<Vec<LoadedApp>, AppLoaderError> {
    let root = root.as_ref();
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("manifest.toml");
        if !manifest_path.exists() {
            continue;
        }
        let source = std::fs::read_to_string(&manifest_path)?;
        let manifest = AppManifest::parse(&source).map_err(|source| AppLoaderError::Manifest {
            path: manifest_path.clone(),
            source,
        })?;
        out.push(LoadedApp {
            manifest,
            base_dir: path,
        });
    }
    // Deterministic order — file system iteration is unspecified on
    // some platforms (notably macOS APFS vs Linux ext4), and the
    // launchpad order should be stable across runs.
    out.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
    Ok(out)
}

#[cfg(target_arch = "wasm32")]
pub fn discover(_root: impl AsRef<Path>) -> Result<Vec<LoadedApp>, AppLoaderError> {
    Ok(Vec::new())
}

/// Resolve the workspace `apps/` directory relative to the current
/// process. Defaults to `./apps`; the `PRISM_APPS_DIR` env var
/// overrides for tests + non-standard layouts.
#[cfg(not(target_arch = "wasm32"))]
pub fn default_apps_root() -> PathBuf {
    std::env::var_os("PRISM_APPS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("apps"))
}

#[cfg(target_arch = "wasm32")]
pub fn default_apps_root() -> PathBuf {
    PathBuf::new()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, contents: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn tempdir(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("prism-app-loader-{}-{}", label, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn empty_when_root_missing() {
        let apps = discover("/nonexistent/path/that/does/not/exist").unwrap();
        assert!(apps.is_empty());
    }

    #[test]
    fn discovers_apps_alphabetically() {
        let root = tempdir("discover");
        write(
            &root,
            "zebra/manifest.toml",
            "id = \"zebra\"\nlabel = \"Zebra\"\n",
        );
        write(
            &root,
            "alpha/manifest.toml",
            "id = \"alpha\"\nlabel = \"Alpha\"\n",
        );
        let apps = discover(&root).unwrap();
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].manifest.id, "alpha");
        assert_eq!(apps[1].manifest.id, "zebra");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn skips_directories_without_manifest() {
        let root = tempdir("skip");
        std::fs::create_dir_all(root.join("bare")).unwrap();
        write(
            &root,
            "lattice/manifest.toml",
            "id = \"lattice\"\nlabel = \"Lattice\"\n",
        );
        let apps = discover(&root).unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].manifest.id, "lattice");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reports_bad_manifest_with_path() {
        let root = tempdir("bad");
        write(&root, "broken/manifest.toml", "label = \"No id\"");
        let err = discover(&root).unwrap_err();
        match err {
            AppLoaderError::Manifest { path, source } => {
                assert!(path.ends_with("broken/manifest.toml"));
                assert!(matches!(source, AppManifestError::MissingField("id")));
            }
            other => panic!("expected Manifest error, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
