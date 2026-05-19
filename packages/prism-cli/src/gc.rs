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

const DAY: u64 = 24 * 60 * 60;

/// Incremental session directories not modified within this window are removed.
const STALE_INCREMENTAL: Duration = Duration::from_secs(3 * DAY);

/// The `wasm32-unknown-unknown` tree is only live while someone is
/// working the web target. Untouched for this long it is pure dead
/// weight — and fully regenerable by the next `prism build web`.
const STALE_WASM: Duration = Duration::from_secs(7 * DAY);

/// When *both* `debug/` and `release/` exist, the one not touched in
/// this window is dropped. The freshly-built profile always has a
/// recent mtime so it is never the victim; this only reclaims the
/// ship profile you stopped using (or vice-versa) without ever
/// touching the dep cache of the profile you *are* using.
const STALE_PROFILE: Duration = Duration::from_secs(14 * DAY);

/// What a sweep reclaimed. Returned so `prism gc` can report; the
/// automatic post-build call ignores it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SweepReport {
    pub incremental_sessions: usize,
    pub wasm_pruned: bool,
    pub stale_profile_pruned: Option<String>,
}

/// Run the post-build sweep over `target_dir`. Best-effort; individual
/// failures are silently ignored. Reclaims, in order of safety:
/// stale incremental sessions, an idle wasm tree, and an idle build
/// profile — never the shared dependency cache of an active profile.
pub fn sweep(target_dir: &Path) -> SweepReport {
    let mut report = SweepReport::default();
    if !target_dir.is_dir() {
        return report;
    }
    let now = SystemTime::now();
    if let Some(cutoff) = now.checked_sub(STALE_INCREMENTAL) {
        for incremental in collect_incremental_dirs(target_dir) {
            report.incremental_sessions += sweep_sessions(&incremental, cutoff);
        }
    }
    report.wasm_pruned = prune_stale_wasm(target_dir, now);
    report.stale_profile_pruned = prune_stale_profile(target_dir, now);
    report
}

/// Remove `target/wasm32-unknown-unknown` when nothing in it has been
/// touched within [`STALE_WASM`].
fn prune_stale_wasm(target_dir: &Path, now: SystemTime) -> bool {
    let wasm = target_dir.join("wasm32-unknown-unknown");
    if !wasm.is_dir() {
        return false;
    }
    let Some(cutoff) = now.checked_sub(STALE_WASM) else {
        return false;
    };
    match newest_mtime(&wasm) {
        Some(m) if m < cutoff => std::fs::remove_dir_all(&wasm).is_ok(),
        _ => false,
    }
}

/// If both native profiles exist, drop whichever one is older than
/// [`STALE_PROFILE`]. At most one is removed per call; the active
/// profile (recent mtime) is always preserved with its dep cache.
fn prune_stale_profile(target_dir: &Path, now: SystemTime) -> Option<String> {
    let debug = target_dir.join("debug");
    let release = target_dir.join("release");
    if !debug.is_dir() || !release.is_dir() {
        return None;
    }
    let cutoff = now.checked_sub(STALE_PROFILE)?;
    for (name, dir) in [("release", &release), ("debug", &debug)] {
        if let Some(m) = newest_mtime(dir) {
            if m < cutoff && std::fs::remove_dir_all(dir).is_ok() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Newest mtime among the *immediate* children of `dir` (one level —
/// cheap, and cargo touches top-level artefacts on every build so it
/// is a faithful "last used" proxy without a deep 162 GB walk).
fn newest_mtime(dir: &Path) -> Option<SystemTime> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
}

/// Backwards-compatible alias for the old narrower entry point.
pub fn trim_incremental(target_dir: &Path) {
    let _ = sweep(target_dir);
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
fn sweep_sessions(incremental: &Path, cutoff: SystemTime) -> usize {
    let Ok(pkg_dirs) = std::fs::read_dir(incremental) else {
        return 0;
    };
    let mut removed = 0;
    for pkg in pkg_dirs.flatten() {
        let Ok(sessions) = std::fs::read_dir(pkg.path()) else {
            continue;
        };
        for session in sessions.flatten() {
            let path = session.path();
            if let Ok(meta) = path.metadata() {
                if meta.modified().map(|m| m < cutoff).unwrap_or(false)
                    && std::fs::remove_dir_all(&path).is_ok()
                {
                    removed += 1;
                }
            }
        }
    }
    removed
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

    fn set_mtime(path: &Path, age: Duration) {
        let mtime = SystemTime::now().checked_sub(age).unwrap();
        filetime::set_file_mtime(path, filetime::FileTime::from_system_time(mtime)).unwrap();
    }

    fn touch(path: &Path, age: Duration) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"x").unwrap();
        set_mtime(path, age);
    }

    #[test]
    fn prunes_idle_wasm_tree_but_keeps_fresh_one() {
        let tmp = tempfile::tempdir().unwrap();
        let wasm = tmp.path().join("wasm32-unknown-unknown");
        touch(&wasm.join("debug").join("prism_shell.wasm"), DAY_D * 30);
        // `newest_mtime` inspects immediate children, i.e. the
        // `debug/` dir mtime — exactly what cargo bumps on a build.
        set_mtime(&wasm.join("debug"), DAY_D * 30);
        let r = sweep(tmp.path());
        assert!(r.wasm_pruned);
        assert!(!wasm.exists());

        touch(
            &wasm.join("debug").join("prism_shell.wasm"),
            Duration::from_secs(60),
        );
        set_mtime(&wasm.join("debug"), Duration::from_secs(60));
        let r = sweep(tmp.path());
        assert!(!r.wasm_pruned);
        assert!(wasm.exists());
    }

    #[test]
    fn drops_stale_profile_keeps_active_one_and_its_deps() {
        let tmp = tempfile::tempdir().unwrap();
        // Active debug profile (fresh) with a dep cache.
        touch(
            &tmp.path().join("debug").join("prism"),
            Duration::from_secs(60),
        );
        touch(
            &tmp.path()
                .join("debug")
                .join("deps")
                .join("libprism_core.rlib"),
            Duration::from_secs(60),
        );
        // Stale release profile.
        touch(&tmp.path().join("release").join("prism"), DAY_D * 30);

        let r = sweep(tmp.path());

        assert_eq!(r.stale_profile_pruned.as_deref(), Some("release"));
        assert!(!tmp.path().join("release").exists());
        // The active profile and its 772-crate dep cache are untouched.
        assert!(tmp.path().join("debug").join("deps").exists());
    }

    #[test]
    fn keeps_both_profiles_when_both_active() {
        let tmp = tempfile::tempdir().unwrap();
        touch(
            &tmp.path().join("debug").join("prism"),
            Duration::from_secs(60),
        );
        touch(
            &tmp.path().join("release").join("prism"),
            Duration::from_secs(60),
        );

        let r = sweep(tmp.path());

        assert_eq!(r.stale_profile_pruned, None);
        assert!(tmp.path().join("debug").exists());
        assert!(tmp.path().join("release").exists());
    }

    const DAY_D: Duration = Duration::from_secs(super::DAY);
}
