//! Post-build garbage collection for stale compilation artefacts.
//!
//! Cargo's incremental compilation keeps per-session fingerprint data
//! under `target/<profile>/incremental/` (native) and
//! `target/<triple>/<profile>/incremental/` (cross-compilation). Each
//! build session writes a new sub-directory; old sessions are never
//! removed automatically, so the directory grows without bound.
//!
//! [`sweep`] walks every profile / cross-compile target it finds and
//! removes session sub-directories whose mtime is older than
//! [`STALE_INCREMENTAL`]. Incremental data is always regenerable on
//! the next build, so this is safe to call after any successful cargo
//! invocation.
//!
//! ## Why no `deps/` dedup
//!
//! An earlier draft tried to dedup `<crate>-<hash>` artefacts in
//! `deps/` / `.fingerprint/` / `build/` by keeping only the newest
//! hash per `(lib_prefix, crate)`. That's *not* safe in practice:
//! cargo can legitimately produce several rmetas for the same crate
//! at the same time when different consumers in the workspace ask
//! for different feature sets, and consumer crates' dep-info files
//! pin specific hashes. Deleting a "duplicate" can silently break
//! the next compile (`extern location for X does not exist: …`).
//!
//! For deeper cleanup the user runs `prism clean` (`cargo clean`)
//! or `cargo clean -p <crate>` — both are cargo-aware, both honour
//! the per-consumer hash pinning, and both are explicit operations
//! the user opts into.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Incremental session directories not modified within this window are removed.
const STALE_INCREMENTAL: Duration = Duration::from_secs(3 * 24 * 60 * 60);

/// Run the post-build sweep over `target_dir`. Best-effort; individual
/// failures are silently ignored.
pub fn sweep(target_dir: &Path) {
    if !target_dir.is_dir() {
        return;
    }
    let cutoff = match SystemTime::now().checked_sub(STALE_INCREMENTAL) {
        Some(t) => t,
        None => return,
    };
    for incremental in collect_incremental_dirs(target_dir) {
        sweep_sessions(&incremental, cutoff);
    }
}

/// Backwards-compatible alias for the old narrower entry point.
pub fn trim_incremental(target_dir: &Path) {
    sweep(target_dir);
}

/// Find every `incremental/` directory cargo may have written to.
fn collect_incremental_dirs(target_dir: &Path) -> Vec<PathBuf> {
    let profiles = ["debug", "release"];
    let mut dirs = Vec::new();

    for profile in &profiles {
        let dir = target_dir.join(profile).join("incremental");
        if dir.is_dir() {
            dirs.push(dir);
        }
    }

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
    fn noop_when_target_missing() {
        let tmp = tempfile::tempdir().unwrap();
        sweep(&tmp.path().join("nope"));
    }

    #[test]
    fn keeps_fresh_sessions_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let inc = pkg_dir(tmp.path(), "debug", "prism_shell-abc");
        make_session(&inc, "s-fresh", Duration::from_secs(60));

        sweep(tmp.path());

        assert!(inc.join("s-fresh").exists());
    }

    #[test]
    fn removes_stale_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let inc = pkg_dir(tmp.path(), "debug", "prism_shell-abc");
        make_session(&inc, "s-old", Duration::from_secs(5 * 24 * 60 * 60));
        make_session(&inc, "s-new", Duration::from_secs(60));

        sweep(tmp.path());

        assert!(!inc.join("s-old").exists());
        assert!(inc.join("s-new").exists());
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

        sweep(tmp.path());

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

        sweep(tmp.path());

        assert!(!wasm_pkg.join("s-stale").exists());
        assert!(wasm_pkg.join("s-fresh").exists());
    }

    #[test]
    fn trim_incremental_alias_calls_sweep() {
        let tmp = tempfile::tempdir().unwrap();
        let inc = pkg_dir(tmp.path(), "debug", "prism_shell-abc");
        make_session(&inc, "s-old", Duration::from_secs(7 * 24 * 60 * 60));

        trim_incremental(tmp.path());

        assert!(!inc.join("s-old").exists());
    }
}
