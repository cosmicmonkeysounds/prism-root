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
    /// ADR-009: the parsed app skeleton when `[entry] skeleton` was
    /// declared and the file parses cleanly. `None` means the app
    /// either declared no skeleton or its skeleton failed to load —
    /// the shell falls back to [`crate::render::default_app_skeleton`].
    pub skeleton: Option<crate::render::Skeleton>,
    /// ADR-009 follow-on: the parsed PRSS stylesheet when
    /// `[entry] styles` was declared and the file loaded cleanly.
    /// Cascades over the host's global stylesheet when this app is
    /// active. `None` means the app inherits the host stylesheet
    /// untouched.
    pub stylesheet: Option<crate::render::Stylesheet>,
    /// Persistent-Luau follow-up: the on-disk source for the app's
    /// `[entry] script`. Read but not executed by the loader — the
    /// shell's `LuauRuntime` runs it once at boot against a long-lived
    /// `Lua` state. `None` when the manifest declared no script or the
    /// file failed to load (read error logged + skipped, app loads
    /// without scripting).
    pub script_source: Option<String>,
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
        // ADR-009: optional per-app skeleton. Parse failures are
        // logged but don't abort the load — the app falls back to
        // the default skeleton so the launchpad tile still surfaces.
        let skeleton = manifest.entry.skeleton.as_ref().and_then(|rel| {
            let skel_path = path.join(rel);
            match crate::render::Skeleton::load_from_path(&skel_path) {
                Ok(s) => Some(s),
                Err(e) => {
                    eprintln!(
                        "prism-shell: failed to load app skeleton `{}` for `{}`: {e}",
                        skel_path.display(),
                        manifest.id
                    );
                    None
                }
            }
        });
        // ADR-009 follow-on: optional per-app PRSS stylesheet.
        // Read failures log + drop the sheet (app inherits host
        // stylesheet); syntactically invalid PRSS still loads with
        // its `parse` diagnostics — the host's PRSS diagnostic
        // surface picks those up if needed.
        let stylesheet = manifest.entry.styles.as_ref().and_then(|rel| {
            let prss_path = path.join(rel);
            match crate::render::Stylesheet::load_from_path(&prss_path) {
                Ok(s) => Some(s),
                Err(e) => {
                    eprintln!(
                        "prism-shell: failed to load app stylesheet `{}` for `{}`: {e}",
                        prss_path.display(),
                        manifest.id
                    );
                    None
                }
            }
        });
        // Persistent-Luau follow-up: read the script source if the
        // manifest declared one. Read failures log + drop the source
        // (app boots without Luau registration, same as if no script
        // were declared at all).
        let script_source = manifest.entry.script.as_ref().and_then(|rel| {
            let script_path = path.join(rel);
            match std::fs::read_to_string(&script_path) {
                Ok(src) => Some(src),
                Err(e) => {
                    eprintln!(
                        "prism-shell: failed to load app script `{}` for `{}`: {e}",
                        script_path.display(),
                        manifest.id
                    );
                    None
                }
            }
        });
        out.push(LoadedApp {
            manifest,
            base_dir: path,
            skeleton,
            stylesheet,
            script_source,
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

// ── Luau hot-reload watcher ──────────────────────────────────────

/// Sibling of [`crate::render::StylesheetWatcher`] for app
/// `main.luau` scripts. Each `observe(app_id, path)` call reads the
/// file, classifies the change relative to the watcher's last
/// snapshot, and on a real change returns the new source so the host
/// can call `Shell::install_app_script(app_id, source)`.
///
/// "Change" is detected by content equality — fingerprint hashing
/// would be marginally faster but the per-app scripts are small,
/// and we already pay the read either way (the new contents are the
/// payload the host wants).
#[derive(Default)]
pub struct LuauScriptWatcher {
    /// Last successfully-read source per app id. `None` per-app key
    /// means the watcher has never seen that file (`FirstSighting`
    /// path).
    last: std::collections::HashMap<String, String>,
}

/// What [`LuauScriptWatcher::observe`] saw on a single tick.
#[derive(Debug, Clone)]
pub enum LuauScriptChange {
    /// File doesn't exist (or was deleted since the last observe).
    /// Hosts may drop the app's registrations on this signal — the
    /// caller decides; the watcher itself never mutates remote state.
    Missing,
    /// File read failed for a reason other than `not found`. Watcher
    /// keeps its prior cached contents so a transient `EBUSY` /
    /// permission glitch doesn't lose the last known good build.
    ReadError(String),
    /// First time we've seen this app's script — the host should
    /// install it just like a boot run would.
    FirstSighting { source: String },
    /// Source bytes changed since the last observe.
    Changed { source: String },
    /// No change since the last observe. The watcher returns this
    /// for steady-state polls so the dev-loop spends zero work in
    /// the no-op case.
    NoChange,
}

impl LuauScriptWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe `path` once. `app_id` keys the per-app cache so a
    /// single watcher can serve any number of apps.
    pub fn observe(&mut self, app_id: &str, path: impl AsRef<Path>) -> LuauScriptChange {
        let path = path.as_ref();
        match std::fs::read_to_string(path) {
            Ok(source) => {
                let prior = self.last.get(app_id);
                match prior {
                    None => {
                        self.last.insert(app_id.to_string(), source.clone());
                        LuauScriptChange::FirstSighting { source }
                    }
                    Some(p) if p == &source => LuauScriptChange::NoChange,
                    Some(_) => {
                        self.last.insert(app_id.to_string(), source.clone());
                        LuauScriptChange::Changed { source }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.last.remove(app_id);
                LuauScriptChange::Missing
            }
            Err(e) => LuauScriptChange::ReadError(e.to_string()),
        }
    }

    /// Number of apps the watcher has seen so far. Test introspection.
    pub fn tracked_count(&self) -> usize {
        self.last.len()
    }

    /// Drop the cached source for `app_id`. Next `observe` against
    /// that id will surface as `FirstSighting`. Used by hosts that
    /// reset an app's state (uninstall + reinstall).
    pub fn forget(&mut self, app_id: &str) -> bool {
        self.last.remove(app_id).is_some()
    }
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

    // ── LuauScriptWatcher ────────────────────────────────────────

    #[test]
    fn luau_watcher_first_sighting_returns_source() {
        let root = tempdir("luau_watcher_first");
        let script = root.join("main.luau");
        std::fs::write(&script, "return 1").unwrap();
        let mut w = LuauScriptWatcher::new();
        let change = w.observe("lattice", &script);
        match change {
            LuauScriptChange::FirstSighting { source } => assert_eq!(source, "return 1"),
            other => panic!("expected FirstSighting, got {other:?}"),
        }
        assert_eq!(w.tracked_count(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn luau_watcher_no_change_on_identical_observe() {
        let root = tempdir("luau_watcher_nochange");
        let script = root.join("main.luau");
        std::fs::write(&script, "return 1").unwrap();
        let mut w = LuauScriptWatcher::new();
        let _ = w.observe("lattice", &script);
        assert!(matches!(
            w.observe("lattice", &script),
            LuauScriptChange::NoChange
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn luau_watcher_changed_returns_new_source() {
        let root = tempdir("luau_watcher_changed");
        let script = root.join("main.luau");
        std::fs::write(&script, "return 1").unwrap();
        let mut w = LuauScriptWatcher::new();
        let _ = w.observe("lattice", &script);
        std::fs::write(&script, "return 2").unwrap();
        match w.observe("lattice", &script) {
            LuauScriptChange::Changed { source } => assert_eq!(source, "return 2"),
            other => panic!("expected Changed, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn luau_watcher_missing_after_delete() {
        let root = tempdir("luau_watcher_missing");
        let script = root.join("main.luau");
        std::fs::write(&script, "return 1").unwrap();
        let mut w = LuauScriptWatcher::new();
        let _ = w.observe("lattice", &script);
        std::fs::remove_file(&script).unwrap();
        assert!(matches!(
            w.observe("lattice", &script),
            LuauScriptChange::Missing
        ));
        assert_eq!(w.tracked_count(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn luau_watcher_keys_per_app() {
        // The watcher tracks each app independently — a change to
        // app A's script must not classify app B's identical-source
        // observe as a change.
        let root = tempdir("luau_watcher_per_app");
        let a = root.join("a.luau");
        let b = root.join("b.luau");
        std::fs::write(&a, "return 1").unwrap();
        std::fs::write(&b, "return 1").unwrap();
        let mut w = LuauScriptWatcher::new();
        assert!(matches!(
            w.observe("a", &a),
            LuauScriptChange::FirstSighting { .. }
        ));
        assert!(matches!(
            w.observe("b", &b),
            LuauScriptChange::FirstSighting { .. }
        ));
        // Both apps now cached; re-observing each is NoChange.
        assert!(matches!(w.observe("a", &a), LuauScriptChange::NoChange));
        assert!(matches!(w.observe("b", &b), LuauScriptChange::NoChange));
        // A change to a doesn't affect b.
        std::fs::write(&a, "return 99").unwrap();
        assert!(matches!(
            w.observe("a", &a),
            LuauScriptChange::Changed { .. }
        ));
        assert!(matches!(w.observe("b", &b), LuauScriptChange::NoChange));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn luau_watcher_forget_resets_to_first_sighting() {
        let root = tempdir("luau_watcher_forget");
        let script = root.join("main.luau");
        std::fs::write(&script, "return 1").unwrap();
        let mut w = LuauScriptWatcher::new();
        let _ = w.observe("lattice", &script);
        assert!(w.forget("lattice"));
        assert!(matches!(
            w.observe("lattice", &script),
            LuauScriptChange::FirstSighting { .. }
        ));
        let _ = std::fs::remove_dir_all(&root);
    }
}
