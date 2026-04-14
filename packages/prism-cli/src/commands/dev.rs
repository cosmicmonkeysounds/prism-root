//! `prism dev` — spawn one or many dev servers through the supervisor.
//!
//! The legal targets match the top-level `pnpm dev:*` scripts in
//! the root `package.json`:
//!
//! - `shell`  — `cargo run -p prism-shell` (native winit+wgpu bin).
//! - `studio` — `cargo tauri dev --config …/tauri.conf.json`.
//! - `web`    — `trunk serve --config packages/prism-shell/Trunk.toml`.
//! - `relay`  — `pnpm --filter @prism/relay dev` (tsx watch).
//! - `all`    — every target above, spawned in parallel behind the
//!   supervisor.

use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::builder::CommandBuilder;
use crate::supervisor::Supervisor;
use crate::workspace::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DevTarget {
    Shell,
    Studio,
    Web,
    Relay,
    All,
}

/// Flags for `prism dev`.
#[derive(Debug, Clone, Args)]
pub struct DevArgs {
    /// Which dev server(s) to run. Defaults to `shell`.
    #[arg(value_enum, default_value_t = DevTarget::Shell)]
    pub target: DevTarget,
}

/// Resolve the dev target into a list of labeled command builders.
pub fn plan(args: &DevArgs, workspace: &Workspace) -> Vec<CommandBuilder> {
    let targets: Vec<DevTarget> = match args.target {
        DevTarget::All => vec![
            DevTarget::Shell,
            DevTarget::Studio,
            DevTarget::Web,
            DevTarget::Relay,
        ],
        one => vec![one],
    };

    targets
        .into_iter()
        .map(|t| builder_for(t, workspace))
        .collect()
}

fn builder_for(target: DevTarget, workspace: &Workspace) -> CommandBuilder {
    match target {
        DevTarget::Shell => CommandBuilder::cargo()
            .arg("run")
            .package("prism-shell")
            .cwd(workspace.root())
            .label("shell"),
        DevTarget::Studio => CommandBuilder::tauri()
            .arg("dev")
            .arg("--config")
            .arg(workspace.tauri_config().to_string_lossy().into_owned())
            .cwd(workspace.root())
            .label("studio"),
        DevTarget::Web => CommandBuilder::trunk()
            .arg("serve")
            .arg("--config")
            .arg(workspace.trunk_config().to_string_lossy().into_owned())
            .cwd(workspace.root())
            .label("web"),
        DevTarget::Relay => CommandBuilder::pnpm()
            .arg("--filter")
            .arg("@prism/relay")
            .arg("dev")
            .cwd(workspace.root())
            .label("relay"),
        DevTarget::All => unreachable!("expanded above"),
    }
}

pub fn run(args: &DevArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let plan = plan(args, workspace);
    if dry_run {
        for cmd in &plan {
            println!("$ [{}] {}", cmd.label_str().unwrap_or("?"), cmd.display());
        }
        return Ok(0);
    }

    // Single-target dev is just a foreground exec — no supervisor
    // overhead, so Ctrl+C still lands on the child directly and
    // the trunk/tauri TUIs keep working.
    if plan.len() == 1 {
        let cmd = &plan[0];
        println!("$ {}", cmd.display());
        let status = cmd
            .build()
            .status()
            .map_err(|e| anyhow::anyhow!("failed to spawn `{}`: {e}", cmd.display()))?;
        return Ok(status.code().unwrap_or(1) as u8);
    }

    // Multi-target dev runs under the supervisor.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let mut s = Supervisor::new();
        for cmd in plan {
            s.add(cmd)?;
        }
        let outcome = s.run().await?;
        Ok(outcome.exit_code)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace::new("/tmp/fake")
    }

    #[test]
    fn shell_is_the_default_target() {
        let a = DevArgs {
            target: DevTarget::Shell,
        };
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].label_str(), Some("shell"));
        assert_eq!(p[0].argv().1, vec!["run", "--package", "prism-shell"]);
    }

    #[test]
    fn studio_uses_tauri_dev_with_config() {
        let a = DevArgs {
            target: DevTarget::Studio,
        };
        let p = plan(&a, &ws());
        let argv = p[0].argv().1;
        assert_eq!(argv[0], "tauri");
        assert_eq!(argv[1], "dev");
        assert_eq!(argv[2], "--config");
        assert!(argv[3].ends_with("tauri.conf.json"));
    }

    #[test]
    fn web_uses_trunk_serve_with_config() {
        let a = DevArgs {
            target: DevTarget::Web,
        };
        let p = plan(&a, &ws());
        assert_eq!(p[0].program(), crate::builder::Program::Trunk);
        let argv = p[0].argv().1;
        assert_eq!(argv[0], "serve");
        assert_eq!(argv[1], "--config");
        assert!(argv[2].ends_with("Trunk.toml"));
    }

    #[test]
    fn relay_uses_pnpm_filter_dev() {
        let a = DevArgs {
            target: DevTarget::Relay,
        };
        let p = plan(&a, &ws());
        assert_eq!(p[0].argv().1, vec!["--filter", "@prism/relay", "dev"]);
    }

    #[test]
    fn all_target_fans_out_to_four_labeled_commands() {
        let a = DevArgs {
            target: DevTarget::All,
        };
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 4);
        let labels: Vec<_> = p.iter().map(|c| c.label_str().unwrap()).collect();
        assert_eq!(labels, vec!["shell", "studio", "web", "relay"]);
    }
}
