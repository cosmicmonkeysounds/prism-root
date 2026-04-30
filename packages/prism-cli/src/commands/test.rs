//! `prism test` — the unified test runner.
//!
//! Thin wrapper around `cargo test`. The flag surface used to carry
//! `--rust`, `--e2e`, and `--all` for the legacy Hono TS relay's
//! Playwright e2e suite; that suite was retired 2026-04-15 alongside
//! the TypeScript relay rewrite, so the runner is now Rust-only.
//! Integration tests for the new axum relay (the `routes` test
//! binary in `packages/prism-relay/tests/routes.rs`) already run
//! under the default `cargo test --workspace` path.

use anyhow::Result;
use clap::Args;

use crate::builder::CommandBuilder;
use crate::workspace::Workspace;

/// Flags for `prism test`.
#[derive(Debug, Clone, Args)]
pub struct TestArgs {
    /// Filter to a single cargo package by name (e.g. `prism-core`).
    /// Omit to run the whole workspace.
    #[arg(short = 'p', long = "package")]
    pub package: Option<String>,

    /// Extra args forwarded to `cargo test` after `--`.
    #[arg(last = true)]
    pub extra: Vec<String>,
}

/// Translate flags into an ordered list of commands to execute.
pub fn plan(args: &TestArgs, workspace: &Workspace) -> Vec<CommandBuilder> {
    let mut cargo = CommandBuilder::cargo().arg("test").label("rust-test");
    cargo = match &args.package {
        Some(pkg) => cargo.package(pkg),
        None => cargo.workspace(),
    };
    if !args.extra.is_empty() {
        cargo = cargo.arg("--").args(args.extra.iter().cloned());
    }
    vec![cargo.cwd(workspace.root())]
}

/// Run the resolved plan.
pub fn run(args: &TestArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let plan = plan(args, workspace);
    let code = super::execute_plan(&plan, dry_run)?;
    if code == 0 {
        crate::gc::trim_incremental(&workspace.target_dir());
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> TestArgs {
        TestArgs {
            package: None,
            extra: Vec::new(),
        }
    }

    fn ws() -> Workspace {
        Workspace::new("/tmp/fake")
    }

    #[test]
    fn plan_defaults_to_cargo_workspace() {
        let p = plan(&args(), &ws());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].argv().1, vec!["test", "--workspace"]);
        assert_eq!(p[0].label_str(), Some("rust-test"));
    }

    #[test]
    fn plan_scopes_to_single_package() {
        let mut a = args();
        a.package = Some("prism-core".into());
        let p = plan(&a, &ws());
        assert_eq!(p[0].argv().1, vec!["test", "--package", "prism-core"]);
    }

    #[test]
    fn plan_forwards_extra_args_after_dashdash() {
        let mut a = args();
        a.extra = vec!["--nocapture".into()];
        let p = plan(&a, &ws());
        assert_eq!(
            p[0].argv().1,
            vec!["test", "--workspace", "--", "--nocapture"]
        );
    }
}
