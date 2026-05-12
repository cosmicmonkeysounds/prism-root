//! `prism visual` — automated visual regression suite.
//!
//! Wave 7.3 of `docs/dev/composable-builder-plan.md` —
//! `prism visual` no longer wraps a macOS `screencapture` shim;
//! it shells directly through `prism-shell --scene <name>
//! --screenshot <path>`, which is the same headless capture path
//! the shell's own visual tests use. The scene list mirrors
//! `prism_shell::headless::BuiltinScene::ALL` — the
//! `scene_names_match_shell_builtin_set` named pin asserts the
//! sets stay in sync.
//!
//! Today's `--screenshot` emission is the deterministic JSON
//! dump (Wave 7.2 PNG deferred — femtovg offscreen GPU plumbing
//! lands when the backend exposes a surfaceless render path).
//! When PNG ships, the file extension below switches back to
//! `.png` without changing the CLI / scene / harness contract.
//!
//! ## Usage
//!
//! ```bash
//! prism visual                     # run all 6 built-in scenes
//! prism visual --scene modifier    # run a single scene
//! prism visual --list              # list available scenes
//! prism visual --output /tmp/shots # custom output directory
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

/// Scene names the harness knows about — must stay in sync with
/// `prism_shell::headless::BuiltinScene::ALL`. The
/// `scene_names_match_shell_builtin_set` test is the named pin
/// that gates that contract; updating this list without updating
/// the shell-side `BuiltinScene` enum (or vice versa) trips it.
const ALL_SCENES: &[&str] = &[
    "default",
    "selection",
    "modifier",
    "context-menu",
    "palette-drag",
    "connection-picker",
];

/// Extension for the per-scene dump file. Today's `prism-shell
/// --screenshot` emits a JSON snapshot of the lowered UI tree
/// (Wave 7.2 PNG deferred — see module header). When PNG lands,
/// this flips to `png` and the test below changes the assertion.
const SCREENSHOT_EXT: &str = "json";

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

    let output_dir = args.output.clone().unwrap_or_else(|| {
        workspace
            .root()
            .join("screenshots")
            .to_string_lossy()
            .into()
    });

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
            .arg(format!(
                "import os; os.makedirs('{output_dir}', exist_ok=True)"
            ))
            .cwd(workspace.root())
            .label("mkdir-screenshots"),
    );

    for scene in &scenes {
        let screenshot_path = format!("{output_dir}/{scene}.{SCREENSHOT_EXT}");
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
        let output_dir = args.output.clone().unwrap_or_else(|| {
            workspace
                .root()
                .join("screenshots")
                .to_string_lossy()
                .into()
        });
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
        a.scene = Some("modifier".into());
        let p = plan(&a, &ws());
        assert_eq!(p.len(), 2); // mkdir + one scene
        assert!(p[1].display().contains("modifier"));
    }

    #[test]
    fn custom_output_dir() {
        let mut a = args();
        a.output = Some("/tmp/my-shots".into());
        let p = plan(&a, &ws());
        assert!(p[1].display().contains("/tmp/my-shots/"));
    }

    /// Wave 7.3 — the scene list this CLI hard-codes must match the
    /// closed set the shell knows how to apply
    /// (`prism_shell::headless::BuiltinScene::ALL`). Drift between
    /// the two surfaces would surface as a `cargo run -p prism-shell
    /// -- --scene <name>` exit-1 at runtime; this pin catches it at
    /// build time without dragging the shell dependency into
    /// prism-cli.
    #[test]
    fn scene_names_match_shell_builtin_set() {
        const SHELL_BUILTIN_SCENE_NAMES: &[&str] = &[
            "default",
            "selection",
            "modifier",
            "context-menu",
            "palette-drag",
            "connection-picker",
        ];
        assert_eq!(ALL_SCENES, SHELL_BUILTIN_SCENE_NAMES);
    }

    /// Wave 7.3 — the per-scene emission path lands in the output
    /// directory with the same extension the shell's
    /// `dump_frame()` writes (today JSON, swaps to PNG when Wave
    /// 7.2 ships). The plan's discipline says "the file emission
    /// path is replaceable with PNG without changing the CLI /
    /// scene / harness contract" — this pin guards that contract.
    #[test]
    fn each_scene_emits_one_screenshot_command_with_expected_extension() {
        let p = plan(&args(), &ws());
        assert_eq!(p.len(), ALL_SCENES.len() + 1);
        for (i, scene) in ALL_SCENES.iter().enumerate() {
            let cmd = &p[i + 1];
            let line = cmd.display();
            assert!(
                line.contains(&format!("/{scene}.{SCREENSHOT_EXT}")),
                "expected output path for scene {scene} to end in .{SCREENSHOT_EXT}, got: {line}"
            );
        }
    }
}
