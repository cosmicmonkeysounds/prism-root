//! `prism test visual` — automated visual regression suite.
//!
//! Runs predefined test scenes through the `prism-shell` binary,
//! captures screenshots, and saves them to a known directory for
//! visual diff review. Each scene is a deterministic state
//! configuration — no manual interaction needed.
//!
//! ## Usage
//!
//! ```bash
//! prism test visual                     # run all scenes
//! prism test visual --scene builder-grid  # run a single scene
//! prism test visual --list              # list available scenes
//! prism test visual --output /tmp/shots # custom output directory
//! ```

use anyhow::Result;
use clap::Args;

use crate::builder::CommandBuilder;
use crate::workspace::Workspace;

/// Flags for `prism test visual`.
#[derive(Debug, Clone, Args)]
pub struct VisualArgs {
    /// Run only a specific scene (e.g. `builder-grid`).
    /// Omit to run all scenes.
    #[arg(long)]
    pub scene: Option<String>,

    /// List available scenes and exit.
    #[arg(long)]
    pub list: bool,

    /// Output directory for screenshots.
    /// Defaults to `<workspace>/screenshots/`.
    #[arg(long, short)]
    pub output: Option<String>,

    /// Viewports to test per scene. Comma-separated.
    /// Defaults to `desktop,tablet,mobile`.
    #[arg(long)]
    pub viewports: Option<String>,
}

const ALL_SCENES: &[&str] = &[
    "launchpad",
    "builder-empty",
    "builder-grid",
    "builder-tablet",
    "builder-mobile",
    "inspector",
    "code-editor",
    "explorer",
];

pub fn plan(args: &VisualArgs, workspace: &Workspace) -> Vec<CommandBuilder> {
    if args.list {
        return vec![CommandBuilder::cargo()
            .arg("run")
            .package("prism-shell")
            .arg("--")
            .arg("--scene")
            .arg("list")
            .cwd(workspace.root())
            .label("visual-list")];
    }

    let output_dir = args
        .output
        .clone()
        .unwrap_or_else(|| workspace.root().join("screenshots").to_string_lossy().into());

    let scenes: Vec<&str> = if let Some(ref s) = args.scene {
        vec![s.as_str()]
    } else {
        ALL_SCENES.to_vec()
    };

    let mut commands = Vec::new();

    // Ensure output directory exists
    commands.push(
        CommandBuilder::new(crate::builder::Program::Python3)
            .arg("-c")
            .arg(format!("import os; os.makedirs('{output_dir}', exist_ok=True)"))
            .cwd(workspace.root())
            .label("mkdir-screenshots"),
    );

    for scene in &scenes {
        let screenshot_path = format!("{output_dir}/{scene}.png");
        commands.push(
            CommandBuilder::cargo()
                .arg("run")
                .package("prism-shell")
                .arg("--")
                .arg("--scene")
                .arg(*scene)
                .arg("--screenshot")
                .arg(screenshot_path)
                .cwd(workspace.root())
                .label(format!("visual-{scene}")),
        );
    }

    commands
}

pub fn run(args: &VisualArgs, workspace: &Workspace, dry_run: bool) -> Result<u8> {
    let plan = plan(args, workspace);
    let result = super::execute_plan(&plan, dry_run)?;

    if !dry_run && result == 0 {
        let output_dir = args
            .output
            .clone()
            .unwrap_or_else(|| workspace.root().join("screenshots").to_string_lossy().into());
        println!("\nScreenshots saved to: {output_dir}/");
        println!("Review them visually or diff against a baseline.");
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace::new("/tmp/fake")
    }

    fn args() -> VisualArgs {
        VisualArgs {
            scene: None,
            list: false,
            output: None,
            viewports: None,
        }
    }

    #[test]
    fn list_flag_produces_single_command() {
        let mut a = args();
        a.list = true;
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 1);
        assert!(p[0].display().contains("--scene list"));
    }

    #[test]
    fn default_plan_runs_all_scenes_plus_mkdir() {
        let p = plan(&args(), &ws());
        assert_eq!(p.len(), ALL_SCENES.len() + 1);
        assert_eq!(p[0].label_str(), Some("mkdir-screenshots"));
    }

    #[test]
    fn single_scene_plan() {
        let mut a = args();
        a.scene = Some("builder-grid".into());
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 2); // mkdir + one scene
        assert!(p[1].display().contains("builder-grid"));
    }

    #[test]
    fn custom_output_dir() {
        let mut a = args();
        a.output = Some("/tmp/my-shots".into());
        let p = plan(&a, &ws());
        assert!(p[1].display().contains("/tmp/my-shots/"));
    }
}
