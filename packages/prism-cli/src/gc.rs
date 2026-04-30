//! Post-build garbage collection for stale compilation artefacts.
//!
//! Cargo's incremental compilation keeps per-session fingerprint data under
//! `target/<profile>/incremental/` (native builds) and
//! `target/<triple>/<profile>/incremental/` (cross-compilation). Each build
//! session writes a new sub-directory; old sessions are never removed
//! automatically, so the directory grows without bound.
//!
//! [`trim_incremental`] walks those directories and removes session entries
//! whose last-modified timestamp is older than [`STALE_AFTER`]. Incremental
//! data is always regenerable on the next build, so this is safe to call
//! after any successful `cargo` invocation.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Session directories not modified within this window are removed.
const STALE_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Remove incremental session directories older than [`STALE_AFTER`].
///
/// Scans both native profiles (`target/{debug,release}/incremental/`) and
/// cross-compilation targets (`target/<triple>/{debug,release}/incremental/`).
/// Errors are silently ignored — this is a best-effort background sweep.
pub fn trim_incremental(target_dir: &Path) {
    let cutoff = match SystemTime::now().checked_sub(STALE_AFTER) {
        Some(t) => t,
        None => return,
    };

    for dir in collect_incremental_dirs(target_dir) {
        sweep_sessions(&dir, cutoff);
    }
}

/// Find every `incremental/` directory that cargo may have created.
fn collect_incremental_dirs(target_dir: &Path) -> Vec<PathBuf> {
    let profiles = ["debug", "release"];
    let mut dirs = Vec::new();

    // Native: target/{debug,release}/incremental/
    for profile in &profiles {
        let dir = target_dir.join(profile).join("incremental");
        if dir.is_dir() {
            dirs.push(dir);
        }
    }

    // Cross-compilation: target/<triple>/{debug,release}/incremental/
    let Ok(entries) = std::fs::read_dir(target_dir) else {
        return dirs;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name == "debug" || name == "release" {
            continue;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        for profile in &profiles {
            let dir = path.join(profile).join("incremental");
            if dir.is_dir() {
                dirs.push(dir);
            }
        }
    }

    dirs
}

/// Walk `incremental/<pkg-hash>/` and delete session sub-directories whose
/// last-modified timestamp predates `cutoff`.
fn sweep_sessions(incremental: &Path, cutoff: SystemTime) {
    let Ok(pkg_dirs) = std::fs::read_dir(incremental) else {
        return;
    };
    for pkg in pkg_dirs.flatten() {
        let Ok(sessions) = std::fs::read_dir(pkg.path()) else {
            continue;
        };
        for session in sessions.flatten() {
            let path = session.path();
            if let Ok(meta) = path.metadata() {
                if meta.modified().map(|m| m < cutoff).unwrap_or(false) {
                    let _ = std::fs::remove_dir_all(&path);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_session(base: &Path, name: &str, age: Duration) {
        let session = base.join(name);
        fs::create_dir_all(&session).unwrap();
        let mtime = SystemTime::now().checked_sub(age).unwrap();
        filetime::set_file_mtime(&session, filetime::FileTime::from_system_time(mtime)).unwrap();
    }

    fn pkg_dir(target: &Path, profile: &str, pkg: &str) -> PathBuf {
        let dir = target.join(profile).join("incremental").join(pkg);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn noop_when_no_incremental_dir() {
        let tmp = tempfile::tempdir().unwrap();
        trim_incremental(tmp.path());
    }

    #[test]
    fn keeps_fresh_sessions_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let inc = pkg_dir(tmp.path(), "debug", "prism_shell-abc");
        make_session(&inc, "s-fresh", Duration::from_secs(60));

        trim_incremental(tmp.path());

        assert!(
            inc.join("s-fresh").exists(),
            "brand-new session should survive the sweep"
        );
    }

    #[test]
    fn removes_stale_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let inc = pkg_dir(tmp.path(), "debug", "prism_shell-abc");
        make_session(&inc, "s-old", Duration::from_secs(10 * 24 * 60 * 60));
        make_session(&inc, "s-new", Duration::from_secs(60));

        trim_incremental(tmp.path());

        assert!(
            !inc.join("s-old").exists(),
            "10-day-old session should be removed"
        );
        assert!(
            inc.join("s-new").exists(),
            "1-minute-old session should survive"
        );
    }

    #[test]
    fn sweeps_both_debug_and_release() {
        let tmp = tempfile::tempdir().unwrap();
        let debug_pkg = pkg_dir(tmp.path(), "debug", "prism_core-111");
        let release_pkg = pkg_dir(tmp.path(), "release", "prism_core-222");
        make_session(&debug_pkg, "s-stale", Duration::from_secs(8 * 24 * 60 * 60));
        make_session(
            &release_pkg,
            "s-stale",
            Duration::from_secs(8 * 24 * 60 * 60),
        );

        trim_incremental(tmp.path());

        assert!(!debug_pkg.join("s-stale").exists());
        assert!(!release_pkg.join("s-stale").exists());
    }

    #[test]
    fn sweeps_cross_compilation_targets() {
        let tmp = tempfile::tempdir().unwrap();
        let wasm_pkg = pkg_dir(
            &tmp.path().join("wasm32-unknown-unknown"),
            "debug",
            "prism_shell-wasm",
        );
        make_session(&wasm_pkg, "s-stale", Duration::from_secs(9 * 24 * 60 * 60));
        make_session(&wasm_pkg, "s-fresh", Duration::from_secs(3600));

        trim_incremental(tmp.path());

        assert!(!wasm_pkg.join("s-stale").exists());
        assert!(wasm_pkg.join("s-fresh").exists());
    }

    #[test]
    fn multiple_packages_in_same_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_a = pkg_dir(tmp.path(), "debug", "prism_shell-aaa");
        let pkg_b = pkg_dir(tmp.path(), "debug", "prism_core-bbb");
        make_session(&pkg_a, "s-old", Duration::from_secs(14 * 24 * 60 * 60));
        make_session(&pkg_b, "s-old", Duration::from_secs(14 * 24 * 60 * 60));
        make_session(&pkg_a, "s-new", Duration::from_secs(60));

        trim_incremental(tmp.path());

        assert!(!pkg_a.join("s-old").exists());
        assert!(!pkg_b.join("s-old").exists());
        assert!(pkg_a.join("s-new").exists());
    }

    #[test]
    fn collect_finds_native_and_cross_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        pkg_dir(tmp.path(), "debug", "a");
        pkg_dir(tmp.path(), "release", "b");
        pkg_dir(&tmp.path().join("wasm32-unknown-unknown"), "debug", "c");

        let dirs = collect_incremental_dirs(tmp.path());
        assert_eq!(dirs.len(), 3);
    }

    #[test]
    fn collect_skips_non_dir_entries() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("CACHEDIR.TAG"), "").unwrap();
        pkg_dir(tmp.path(), "debug", "x");

        let dirs = collect_incremental_dirs(tmp.path());
        assert_eq!(dirs.len(), 1);
    }
}
