//! `prism e2e` — end-to-end test runner.
//!
//! Runs the built-in e2e test scripts through the shell binary in
//! `--e2e` mode. Each script is a sequence of input actions and
//! state assertions that exercises the shell through the same code
//! paths a human uses.
//!
//! ## Usage
//!
//! ```bash
//! prism e2e                          # run all tests
//! prism e2e --test viewport-switching # run a single test
//! prism e2e --list                   # list available tests
//! prism e2e --record                 # capture baseline screenshots
//! prism e2e --output /tmp/e2e        # custom screenshot directory
//! ```

use anyhow::Result;
use clap::Args;

use crate::builder::CommandBuilder;
use crate::workspace::Workspace;

#[derive(Debug, Clone, Args)]
pub struct E2eArgs {
    /// Run a single named test.
    #[arg(long)]
    pub test: Option<String>,

    /// List available e2e tests and exit.
    #[arg(long)]
    pub list: bool,

    /// Capture baseline screenshots instead of running assertions.
    #[arg(long)]
    pub record: bool,

    /// Output directory for screenshots.
    #[arg(long, short)]
    pub output: Option<String>,

    /// Use OS-level input injection (requires display + accessibility).
    #[arg(long)]
    pub os_input: bool,

    /// Step delay in milliseconds (default: 16).
    #[arg(long)]
    pub step_delay: Option<u64>,
}

const ALL_TESTS: &[&str] = &[
    "launchpad-state",
    "scene-loading",
    "viewport-switching",
    "keyboard-dispatch",
    "command-palette-toggle",
    "panel-navigation",
    "selection-lifecycle",
    "undo-redo-cycle",
    "grid-cell-interaction",
    "sidebar-toggles",
    "zoom-controls",
    "document-structure",
];

pub fn plan(args: &E2eArgs, workspace: &Workspace) -> Vec<CommandBuilder> {
    if args.list {
        return vec![CommandBuilder::cargo()
            .arg("run")
            .package("prism-shell")
            .arg("--")
            .arg("--e2e")
            .arg("--list")
            .cwd(workspace.root())
            .label("e2e-list")];
    }

    let mut cmd = CommandBuilder::cargo()
        .arg("run")
        .package("prism-shell")
        .arg("--")
        .arg("--e2e")
        .cwd(workspace.root())
        .label("e2e-run");

    if let Some(ref test_name) = args.test {
        cmd = cmd.arg("--e2e-test").arg(test_name.clone());
    }

    if args.record {
        cmd = cmd.arg("--e2e-record");
    }

    if let Some(ref output) = args.output {
        cmd = cmd.arg("--e2e-output").arg(output.clone());
    }

    if args.os_input {
        cmd = cmd.arg("--e2e-os-input");
    }

    if let Some(delay) = args.step_delay {
        cmd = cmd.arg("--e2e-step-delay").arg(delay.to_string());
    }

    vec![cmd]
}

pub fn run(args: &E2eArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    if args.list && !dry_run {
        println!("Available e2e tests:");
        for name in ALL_TESTS {
            println!("  {name}");
        }
        return Ok(0);
    }

    let plan = plan(args, workspace);
    super::execute_plan(&plan, dry_run)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace::new("/tmp/fake")
    }

    fn args() -> E2eArgs {
        E2eArgs {
            test: None,
            list: false,
            record: false,
            output: None,
            os_input: false,
            step_delay: None,
        }
    }

    #[test]
    fn list_flag_produces_single_command() {
        let mut a = args();
        a.list = true;
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 1);
        assert!(p[0].display().contains("--e2e"));
        assert!(p[0].display().contains("--list"));
    }

    #[test]
    fn default_plan_runs_all() {
        let p = plan(&args(), &ws());
        assert_eq!(p.len(), 1);
        assert!(p[0].display().contains("--e2e"));
    }

    #[test]
    fn single_test_plan() {
        let mut a = args();
        a.test = Some("viewport-switching".into());
        let p = plan(&a, &ws());
        assert!(p[0].display().contains("--e2e-test"));
        assert!(p[0].display().contains("viewport-switching"));
    }

    #[test]
    fn record_flag() {
        let mut a = args();
        a.record = true;
        let p = plan(&a, &ws());
        assert!(p[0].display().contains("--e2e-record"));
    }

    #[test]
    fn custom_output() {
        let mut a = args();
        a.output = Some("/tmp/shots".into());
        let p = plan(&a, &ws());
        assert!(p[0].display().contains("--e2e-output"));
        assert!(p[0].display().contains("/tmp/shots"));
    }
}
