//! `prism fmt` — workspace `cargo fmt --all`, optionally `--check`.

use anyhow::Result;

use crate::builder::CommandBuilder;
use crate::workspace::Workspace;

pub fn plan(check: bool, workspace: &Workspace) -> Vec<CommandBuilder> {
    let mut cmd = CommandBuilder::cargo()
        .arg("fmt")
        .arg("--all")
        .cwd(workspace.root())
        .label("fmt");
    if check {
        cmd = cmd.arg("--").arg("--check");
    }
    vec![cmd]
}

pub fn run(check: bool, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let plan = plan(check, workspace);
    super::execute_plan(&plan, dry_run)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace::new("/tmp/fake")
    }

    #[test]
    fn fmt_default_is_write() {
        let p = plan(false, &ws());
        assert_eq!(p[0].argv().1, vec!["fmt", "--all"]);
    }

    #[test]
    fn fmt_check_adds_dash_check() {
        let p = plan(true, &ws());
        assert_eq!(p[0].argv().1, vec!["fmt", "--all", "--", "--check"]);
    }
}
