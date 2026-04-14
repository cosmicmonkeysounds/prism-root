//! `prism test` — the unified test runner.
//!
//! The layout here is: first figure out which *test suites* to run
//! (Rust unit, relay E2E via playwright), then translate that into a
//! flat list of [`CommandBuilder`]s and hand them to
//! [`super::execute_plan`]. The plan is pure data so [`plan`] can be
//! unit-tested without shelling anything.
//!
//! Relay TS unit tests used to live behind a `--ts` flag that went
//! through `pnpm exec vitest run`, but vitest was never wired into
//! the relay's `devDependencies`. The flag is dropped until the
//! relay rewrite lands a real test runner.

use anyhow::Result;
use clap::Args;

use crate::builder::CommandBuilder;
use crate::workspace::Workspace;

/// Flags for `prism test`.
#[derive(Debug, Clone, Args)]
pub struct TestArgs {
    /// Filter to a single Rust package by name (e.g. `prism-core`).
    /// Applies to Rust tests only. Omit to run the whole workspace.
    #[arg(short = 'p', long = "package")]
    pub package: Option<String>,

    /// Run Rust tests (`cargo test`). Default on if no other suite
    /// flag is set.
    #[arg(long)]
    pub rust: bool,

    /// Run relay E2E tests (`pnpm --filter @prism/relay test:e2e`).
    #[arg(long)]
    pub e2e: bool,

    /// Run every suite: Rust + E2E.
    #[arg(long)]
    pub all: bool,

    /// Extra args forwarded to the underlying test runners after `--`.
    #[arg(last = true)]
    pub extra: Vec<String>,
}

impl TestArgs {
    /// Resolve which suites to run after applying the defaulting
    /// rules: `--all` implies everything; otherwise explicit flags
    /// win; otherwise fall back to Rust-only.
    pub fn resolved_suites(&self) -> Suites {
        if self.all {
            return Suites {
                rust: true,
                e2e: true,
            };
        }
        if !self.rust && !self.e2e {
            return Suites {
                rust: true,
                e2e: false,
            };
        }
        Suites {
            rust: self.rust,
            e2e: self.e2e,
        }
    }
}

/// Which test suites the user asked for, after defaulting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Suites {
    pub rust: bool,
    pub e2e: bool,
}

/// Translate flags into an ordered list of commands to execute.
pub fn plan(args: &TestArgs, workspace: &Workspace) -> Vec<CommandBuilder> {
    let suites = args.resolved_suites();
    let mut plan = Vec::new();

    if suites.rust {
        let mut cargo = CommandBuilder::cargo().arg("test").label("rust-test");
        cargo = match &args.package {
            Some(pkg) => cargo.package(pkg),
            None => cargo.workspace(),
        };
        if !args.extra.is_empty() {
            cargo = cargo.arg("--").args(args.extra.iter().cloned());
        }
        cargo = cargo.cwd(workspace.root());
        plan.push(cargo);
    }

    if suites.e2e {
        let e2e = CommandBuilder::pnpm()
            .arg("--filter")
            .arg("@prism/relay")
            .arg("run")
            .arg("test:e2e")
            .args(args.extra.iter().cloned())
            .cwd(workspace.root())
            .label("relay-e2e");
        plan.push(e2e);
    }

    plan
}

/// Run the resolved plan.
pub fn run(args: &TestArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let plan = plan(args, workspace);
    super::execute_plan(&plan, dry_run)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> TestArgs {
        TestArgs {
            package: None,
            rust: false,
            e2e: false,
            all: false,
            extra: Vec::new(),
        }
    }

    fn ws() -> Workspace {
        Workspace::new("/tmp/fake")
    }

    #[test]
    fn defaults_to_rust_only() {
        let s = args().resolved_suites();
        assert_eq!(
            s,
            Suites {
                rust: true,
                e2e: false
            }
        );
    }

    #[test]
    fn all_flag_runs_everything() {
        let mut a = args();
        a.all = true;
        let s = a.resolved_suites();
        assert_eq!(
            s,
            Suites {
                rust: true,
                e2e: true
            }
        );
    }

    #[test]
    fn explicit_e2e_flag_drops_rust_default() {
        let mut a = args();
        a.e2e = true;
        let s = a.resolved_suites();
        assert_eq!(
            s,
            Suites {
                rust: false,
                e2e: true
            }
        );
    }

    #[test]
    fn rust_and_e2e_combine() {
        let mut a = args();
        a.rust = true;
        a.e2e = true;
        let s = a.resolved_suites();
        assert_eq!(
            s,
            Suites {
                rust: true,
                e2e: true
            }
        );
    }

    #[test]
    fn plan_defaults_to_cargo_workspace() {
        let p = plan(&args(), &ws());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].argv().1, vec!["test", "--workspace"]);
    }

    #[test]
    fn plan_scopes_to_single_package() {
        let mut a = args();
        a.package = Some("prism-core".into());
        let p = plan(&a, &ws());
        assert_eq!(p[0].argv().1, vec!["test", "--package", "prism-core"]);
    }

    #[test]
    fn plan_all_emits_two_commands() {
        let mut a = args();
        a.all = true;
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].label_str(), Some("rust-test"));
        assert_eq!(p[1].label_str(), Some("relay-e2e"));
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

    #[test]
    fn plan_e2e_uses_pnpm_filter() {
        let mut a = args();
        a.rust = false;
        a.e2e = true;
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].program(), crate::builder::Program::Pnpm);
        assert_eq!(
            p[0].argv().1,
            vec!["--filter", "@prism/relay", "run", "test:e2e"]
        );
    }
}
