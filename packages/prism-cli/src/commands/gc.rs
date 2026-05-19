//! `prism gc` — reclaim `target/` space the *safe* way.
//!
//! The default reclaims everything that is provably regenerable on
//! the next compile **without** throwing away the 772-crate
//! dependency cache of the profile you are actively using:
//!
//! - incremental session dirs older than 3 days,
//! - the `wasm32-unknown-unknown` tree if idle > 7 days,
//! - a build profile (`debug`/`release`) if idle > 14 days while the
//!   other is active.
//!
//! `--hard` is the nuclear option: full `cargo clean`. It is the
//! single biggest anti-optimisation on this workspace (every
//! subsequent build is a ~30-minute cold rebuild of all 772 crates),
//! so it is opt-in and loudly the exception, not a scheduled default.

use anyhow::Result;
use clap::Args;

use crate::builder::CommandBuilder;
use crate::workspace::Workspace;

/// Flags for `prism gc`.
#[derive(Debug, Clone, Args)]
pub struct GcArgs {
    /// Nuclear: full `cargo clean`. Wipes the entire dependency
    /// cache — the next build is a full cold rebuild. Only reach for
    /// this when artefacts are corrupt, not for routine housekeeping
    /// (the default smart sweep is what you want there).
    #[arg(long)]
    pub hard: bool,
}

/// `--hard` maps to the existing `cargo clean` plan so the argv
/// lives in exactly one place.
pub fn plan(args: &GcArgs, workspace: &Workspace) -> Vec<CommandBuilder> {
    if args.hard {
        super::clean::plan(workspace)
    } else {
        Vec::new()
    }
}

pub fn run(args: &GcArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    if args.hard {
        return super::execute_plan(&plan(args, workspace), dry_run);
    }
    if dry_run {
        println!(
            "$ prism gc (smart sweep over {})",
            workspace.target_dir().display()
        );
        return Ok(0);
    }
    let r = crate::gc::sweep(&workspace.target_dir());
    println!(
        "gc: {} stale incremental session(s) removed{}{}",
        r.incremental_sessions,
        if r.wasm_pruned {
            ", idle wasm tree pruned"
        } else {
            ""
        },
        match &r.stale_profile_pruned {
            Some(p) => format!(", idle `{p}` profile pruned"),
            None => String::new(),
        }
    );
    println!(
        "gc: dependency cache for the active profile left intact (use --hard to wipe everything)"
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace::new("/tmp/fake")
    }

    #[test]
    fn soft_gc_has_no_cargo_plan() {
        assert!(plan(&GcArgs { hard: false }, &ws()).is_empty());
    }

    #[test]
    fn hard_gc_delegates_to_cargo_clean() {
        let p = plan(&GcArgs { hard: true }, &ws());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].argv().1, vec!["clean"]);
    }
}
