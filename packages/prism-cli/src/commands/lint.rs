//! `prism lint` — workspace clippy with `-D warnings`.

use anyhow::Result;

use crate::builder::CommandBuilder;
use crate::workspace::Workspace;

pub fn plan(workspace: &Workspace) -> Vec<CommandBuilder> {
    vec![CommandBuilder::cargo()
        .arg("clippy")
        .workspace()
        .arg("--all-targets")
        .arg("--")
        .arg("-D")
        .arg("warnings")
        .cwd(workspace.root())
        .label("clippy")]
}

pub fn run(workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let plan = plan(workspace);
    super::execute_plan(&plan, dry_run)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clippy_plan_matches_root_claude_md() {
        let p = plan(&Workspace::new("/tmp/fake"));
        assert_eq!(p.len(), 1);
        assert_eq!(
            p[0].argv().1,
            vec![
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings"
            ]
        );
    }
}
