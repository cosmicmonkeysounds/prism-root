//! `prism clean` — remove all build artefacts.
//!
//! Delegates to `cargo clean` which removes the entire `target/`
//! directory. For routine housekeeping the automatic post-build GC
//! in [`crate::gc`] handles stale incremental sessions; this command
//! is the nuclear option when you need a full wipe.

use anyhow::Result;

use crate::builder::CommandBuilder;
use crate::workspace::Workspace;

pub fn plan(workspace: &Workspace) -> Vec<CommandBuilder> {
    vec![CommandBuilder::cargo()
        .arg("clean")
        .label("clean")
        .cwd(workspace.root())]
}

pub fn run(workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let plan = plan(workspace);
    super::execute_plan(&plan, dry_run)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace::new("/tmp/fake")
    }

    #[test]
    fn plan_runs_cargo_clean() {
        let p = plan(&ws());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].argv().1, vec!["clean"]);
        assert_eq!(p[0].label_str(), Some("clean"));
    }
}
