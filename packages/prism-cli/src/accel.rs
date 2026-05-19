//! Build acceleration — runtime-detected, gracefully degrading.
//!
//! Currently this is **`sccache`** as `RUSTC_WRAPPER`: a shared
//! compilation cache that survives `cargo clean`, branch switches, and
//! `target/` GC — the single biggest defence against the "I cleaned
//! and now it's a 30-minute cold rebuild" trap.
//!
//! ## Why no linker override here anymore
//!
//! An earlier revision also injected an `lld` linker via
//! `CARGO_TARGET_<host>_RUSTFLAGS`. That changes the rustc command
//! line, so any cargo invocation that *didn't* go through this CLI
//! (a raw `cargo build`, `cargo test`, or rust-analyzer's
//! `cargo check`) saw a different fingerprint and triggered a full
//! workspace recompile every time you alternated tools. Build-time
//! flags that aren't applied *uniformly* are worse than no flags at
//! all. The linker choice now lives (commented, opt-in) in
//! `.cargo/config.toml`, which every cargo invocation reads, so the
//! fingerprint stays identical no matter who launched the build.
//!
//! `sccache` is safe to keep here: `RUSTC_WRAPPER` only adds a cache
//! shim — its presence/absence doesn't change the artifact
//! fingerprint, so a CLI-only wrapper can't cause the recompile
//! thrash a CLI-only RUSTFLAGS does. `PRISM_NO_ACCEL=1` opts out.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Environment overlay applied to every `cargo` command the CLI
/// spawns. Computed once per process.
fn overlay() -> &'static Vec<(String, String)> {
    static OVERLAY: OnceLock<Vec<(String, String)>> = OnceLock::new();
    OVERLAY.get_or_init(detect)
}

/// Return `(key, value)` env pairs to layer onto a `cargo` invocation.
/// Empty when acceleration is disabled or no accelerator is available.
pub fn cargo_env() -> &'static [(String, String)] {
    overlay().as_slice()
}

fn detect() -> Vec<(String, String)> {
    if std::env::var_os("PRISM_NO_ACCEL").is_some() {
        return Vec::new();
    }
    let mut env = Vec::new();

    // sccache — only if the user hasn't already chosen a wrapper.
    if std::env::var_os("RUSTC_WRAPPER").is_none() && which("sccache").is_some() {
        env.push(("RUSTC_WRAPPER".to_string(), "sccache".to_string()));
    }

    env
}

/// First match for `name` across `PATH`. Pure filesystem probe — no
/// subprocess — so it is cheap enough to run unconditionally.
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(name);
        candidate.is_file().then_some(candidate)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_accel_env_disables_everything() {
        temp_env(&[("PRISM_NO_ACCEL", Some("1"))], || {
            assert!(detect().is_empty());
        });
    }

    #[test]
    fn which_finds_sh_on_unix_path() {
        assert!(which("sh").is_some() || which("sh.exe").is_some());
    }

    #[test]
    fn which_misses_nonexistent_binary() {
        assert!(which("definitely-not-a-real-binary-xyz").is_none());
    }

    #[test]
    fn sccache_wrapper_not_added_when_user_set_one() {
        temp_env(
            &[
                ("PRISM_NO_ACCEL", None),
                ("RUSTC_WRAPPER", Some("/usr/bin/true")),
            ],
            || {
                assert!(!detect().iter().any(|(k, _)| k == "RUSTC_WRAPPER"));
            },
        );
    }

    /// Minimal scoped-env helper. Process-wide and restored on return.
    fn temp_env(vars: &[(&str, Option<&str>)], f: impl FnOnce()) {
        let saved: Vec<_> = vars
            .iter()
            .map(|(k, _)| (k.to_string(), std::env::var(k).ok()))
            .collect();
        for (k, v) in vars {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        f();
        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(&k, v),
                None => std::env::remove_var(&k),
            }
        }
    }
}
