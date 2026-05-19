//! Build acceleration — runtime-detected, gracefully degrading.
//!
//! Two accelerators materially cut Prism's build time on the 772-crate
//! dependency graph:
//!
//! 1. **`sccache`** as `RUSTC_WRAPPER`. A shared compilation cache that
//!    survives `cargo clean`, branch switches, and `target/` GC — the
//!    single biggest defence against the "I cleaned and now it's a
//!    30-minute cold rebuild" trap.
//! 2. **`lld`** as the native linker. Linking the femtovg / winit /
//!    glutin / resvg desktop binary is a large fraction of every
//!    incremental build; `lld` is several times faster than the
//!    default. The toolchain already ships `ld64.lld` under
//!    `rustup`'s `gcc-ld` dir, so no external install is required.
//!
//! ## Why runtime-gated instead of `.cargo/config.toml`
//!
//! Hardcoding `RUSTC_WRAPPER`/linker in `.cargo/config.toml` applies
//! *unconditionally* — including to plain `cargo`. If the tool is
//! missing every single build breaks with a spawn error. Detecting
//! here and injecting the env only when the tool is actually present
//! keeps the stock toolchain working and means the optimisation
//! "just turns on" once a dev installs `sccache`, with zero config
//! churn. `PRISM_NO_ACCEL=1` opts out entirely.

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

    // lld for the native host target. Scoped to the host triple's
    // CARGO_TARGET_<triple>_RUSTFLAGS so the wasm leg (which already
    // links with wasm-ld) and any cross target are untouched.
    if let (Some(host), Some(lld)) = (host_triple(), lld_path()) {
        let key = format!(
            "CARGO_TARGET_{}_RUSTFLAGS",
            host.to_uppercase().replace(['-', '.'], "_")
        );
        if std::env::var_os(&key).is_none() {
            env.push((
                key,
                format!("-Clink-arg=-fuse-ld={}", lld.to_string_lossy()),
            ));
        }
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

/// `rustc --print sysroot`, memoised.
fn sysroot() -> Option<&'static PathBuf> {
    static SYSROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    SYSROOT
        .get_or_init(|| {
            let out = std::process::Command::new("rustc")
                .arg("--print")
                .arg("sysroot")
                .output()
                .ok()?;
            out.status
                .success()
                .then(|| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string()))
        })
        .as_ref()
}

/// Host target triple parsed from `rustc -vV`, memoised.
fn host_triple() -> Option<&'static String> {
    static HOST: OnceLock<Option<String>> = OnceLock::new();
    HOST.get_or_init(|| {
        let out = std::process::Command::new("rustc")
            .arg("-vV")
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|l| l.strip_prefix("host: "))
            .map(|s| s.trim().to_string())
    })
    .as_ref()
}

/// Path to the `ld64.lld` / `ld.lld` shim rustup ships under
/// `lib/rustlib/<host>/bin/gcc-ld/`. `None` if the component layout
/// differs (e.g. a distro-packaged rustc) — caller degrades to the
/// default linker.
fn lld_path() -> Option<PathBuf> {
    let sysroot = sysroot()?;
    let host = host_triple()?;
    let gcc_ld = sysroot
        .join("lib")
        .join("rustlib")
        .join(host)
        .join("bin")
        .join("gcc-ld");
    // macOS links Mach-O via ld64.lld; everything else via ld.lld.
    let driver = if cfg!(target_os = "macos") {
        "ld64.lld"
    } else {
        "ld.lld"
    };
    let p = gcc_ld.join(driver);
    p.is_file().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_accel_env_disables_everything() {
        // detect() reads the live env; assert the gate short-circuits
        // by calling it directly with the var set for this thread's
        // process view is not possible without unsafe set_var, so we
        // assert the structural invariant instead: an empty PATH and
        // disabled gate yield nothing.
        temp_env(&[("PRISM_NO_ACCEL", Some("1"))], || {
            assert!(detect().is_empty());
        });
    }

    #[test]
    fn which_finds_sh_on_unix_path() {
        // `sh` is on PATH on every platform the CLI targets.
        assert!(which("sh").is_some() || which("sh.exe").is_some());
    }

    #[test]
    fn which_misses_nonexistent_binary() {
        assert!(which("definitely-not-a-real-binary-xyz").is_none());
    }

    #[test]
    fn lld_path_when_present_is_a_file() {
        if let Some(p) = lld_path() {
            assert!(p.is_file());
            assert!(p.ends_with("ld64.lld") || p.ends_with("ld.lld"));
        }
    }

    /// Minimal scoped-env helper. `std::env::set_var` is process-wide
    /// and `unsafe` on 2024+ editions; we keep the surface tiny and
    /// restore on drop.
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
