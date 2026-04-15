//! Workspace discovery.
//!
//! Every CLI subcommand needs to resolve paths relative to the Prism
//! workspace root (the directory that holds the top-level
//! `Cargo.toml`). We walk up from the current working directory
//! until we find it, then cache the path on a [`Workspace`] value
//! that the commands and tests can pass around.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

/// Resolved handle to a Prism workspace on disk.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// Walk up from the current working directory until we find a
    /// Cargo.toml whose `[workspace]` table lists `packages/prism-cli`
    /// as a member. We look for that specific marker instead of just
    /// "any Cargo.toml" so `prism` can be invoked from inside a
    /// nested crate (e.g. `packages/prism-shell`) and still land on
    /// the root.
    pub fn discover() -> Result<Self> {
        let cwd = std::env::current_dir().context("reading current directory")?;
        Self::discover_from(&cwd)
    }

    /// Same as [`Self::discover`] but scans upward starting from
    /// `start`. Exposed for tests that pin the search to a tempdir.
    pub fn discover_from(start: &Path) -> Result<Self> {
        let mut current = start.to_path_buf();
        loop {
            let candidate = current.join("Cargo.toml");
            if candidate.is_file() && is_prism_workspace_manifest(&candidate)? {
                return Ok(Self { root: current });
            }
            if !current.pop() {
                return Err(anyhow!(
                    "no Prism workspace Cargo.toml found above {}",
                    start.display()
                ));
            }
        }
    }

    /// Construct a [`Workspace`] for an explicit root directory.
    /// Used by tests that already know where the workspace lives.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The resolved workspace root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `packages/<name>` inside the workspace.
    pub fn package(&self, name: &str) -> PathBuf {
        self.root.join("packages").join(name)
    }

    /// Directory served by `prism dev web` — holds `index.html`
    /// and (after a build) the wasm-bindgen-produced
    /// `prism_shell.js` + `prism_shell_bg.wasm` pair.
    pub fn shell_web_dir(&self) -> PathBuf {
        self.package("prism-shell").join("web")
    }

    /// Where cargo drops the raw `wasm32-unknown-unknown` cdylib
    /// for a given profile. `debug` for `cargo build` without
    /// `--release`, `release` otherwise. `wasm-bindgen` reads
    /// `prism_shell.wasm` from this directory and writes its
    /// processed output into [`Self::shell_web_dir`].
    pub fn wasm_artifact_dir(&self, release: bool) -> PathBuf {
        let profile = if release { "release" } else { "debug" };
        self.root
            .join("target")
            .join("wasm32-unknown-unknown")
            .join(profile)
    }

    /// Path to the cargo-emitted `prism_shell.wasm` for a given
    /// profile — the input `wasm-bindgen` post-processes into the
    /// ESM loader pair. Thin helper so build/dev don't duplicate
    /// the join.
    pub fn shell_wasm_artifact(&self, release: bool) -> PathBuf {
        self.wasm_artifact_dir(release).join("prism_shell.wasm")
    }
}

fn is_prism_workspace_manifest(path: &Path) -> Result<bool> {
    let contents =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    // Primitive check — we deliberately avoid pulling in a TOML
    // parser just for workspace discovery. The root Cargo.toml is
    // the only one in the tree that declares `packages/prism-cli`
    // under a `[workspace]` `members` list, so a substring match is
    // precise enough and stays trivial to audit.
    Ok(contents.contains("[workspace]") && contents.contains("packages/prism-cli"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_finds_project_root() {
        let ws = Workspace::discover().expect("discover from cargo test cwd");
        assert!(ws.root().join("Cargo.toml").is_file());
        assert!(ws.package("prism-core").join("Cargo.toml").is_file());
    }

    #[test]
    fn discover_climbs_from_nested_dir() {
        let ws = Workspace::discover().expect("discover");
        let nested = ws.package("prism-core").join("src");
        let rediscovered = Workspace::discover_from(&nested).expect("rediscover");
        assert_eq!(rediscovered.root(), ws.root());
    }

    #[test]
    fn discover_errors_outside_workspace() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let err = Workspace::discover_from(tmp.path()).unwrap_err();
        assert!(err.to_string().contains("no Prism workspace"));
    }

    #[test]
    fn package_path_resolution() {
        let ws = Workspace::new("/tmp/fake-root");
        assert_eq!(
            ws.package("prism-shell"),
            PathBuf::from("/tmp/fake-root/packages/prism-shell")
        );
        assert_eq!(
            ws.shell_web_dir(),
            PathBuf::from("/tmp/fake-root/packages/prism-shell/web")
        );
    }

    #[test]
    fn wasm_artifact_dir_distinguishes_profile() {
        let ws = Workspace::new("/tmp/fake-root");
        assert_eq!(
            ws.wasm_artifact_dir(false),
            PathBuf::from("/tmp/fake-root/target/wasm32-unknown-unknown/debug")
        );
        assert_eq!(
            ws.wasm_artifact_dir(true),
            PathBuf::from("/tmp/fake-root/target/wasm32-unknown-unknown/release")
        );
    }

    #[test]
    fn shell_wasm_artifact_points_at_cargo_output() {
        let ws = Workspace::new("/tmp/fake-root");
        assert_eq!(
            ws.shell_wasm_artifact(true),
            PathBuf::from(
                "/tmp/fake-root/target/wasm32-unknown-unknown/release/prism_shell.wasm"
            )
        );
    }
}
