//! Native dev binary — boots `prism_shell::Shell` on
//! `prism_ui_runtime`'s femtovg backend, with optional headless
//! capture flags landed in Wave 7 of
//! `docs/dev/composable-builder-plan.md`.
//!
//! Recognised flags:
//!
//! * `--scene <name>` — apply a named scene from
//!   `prism_shell::headless::BuiltinScene` before run / dump.
//!   Pass `--scene list` to print the available names. Unknown
//!   names exit with a non-zero status.
//! * `--screenshot <path>` — render one frame headlessly and
//!   write a deterministic JSON snapshot of the lowered UI tree to
//!   `<path>`. Implies "don't open a window". The PNG variant is a
//!   follow-up that ships when the femtovg offscreen surface
//!   wiring lands; today's text snapshot is the regression target
//!   the visual harness diffs against.

use prism_shell::headless::BuiltinScene;
use prism_shell::Shell;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = parse_cli(&args)?;

    if let Cli::ListScenes = cli {
        for scene in BuiltinScene::ALL {
            println!("{}", scene.name());
        }
        return Ok(());
    }

    let shell = Shell::new()?;
    if let Some(scene) = scene_from_cli(&cli) {
        shell.apply_scene(scene);
    }

    if let Cli::Screenshot { path, .. } = &cli {
        std::fs::write(path, shell.dump_frame())?;
        return Ok(());
    }

    shell.run()
}

#[derive(Debug, PartialEq)]
enum Cli {
    /// No flags — boot into the femtovg event loop.
    Run,
    /// `--scene <name>` — apply the scene, run the event loop.
    Scene { scene: BuiltinScene },
    /// `--scene list` — print the scene names and exit.
    ListScenes,
    /// `--screenshot <path>` (optionally combined with `--scene`).
    Screenshot {
        path: String,
        scene: Option<BuiltinScene>,
    },
}

fn scene_from_cli(cli: &Cli) -> Option<BuiltinScene> {
    match cli {
        Cli::Scene { scene } => Some(*scene),
        Cli::Screenshot { scene: Some(s), .. } => Some(*s),
        _ => None,
    }
}

fn parse_cli(args: &[String]) -> Result<Cli, String> {
    let mut scene: Option<BuiltinScene> = None;
    let mut screenshot: Option<String> = None;
    let mut list_scenes = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--scene" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| "--scene requires a value".to_string())?;
                if value == "list" {
                    list_scenes = true;
                } else {
                    scene = Some(BuiltinScene::from_name(value).ok_or_else(|| {
                        format!("unknown scene '{value}'; --scene list prints available names")
                    })?);
                }
                i += 2;
            }
            "--screenshot" => {
                let path = args
                    .get(i + 1)
                    .ok_or_else(|| "--screenshot requires a path".to_string())?;
                screenshot = Some(path.clone());
                i += 2;
            }
            other => return Err(format!("unknown flag '{other}'")),
        }
    }
    if list_scenes {
        return Ok(Cli::ListScenes);
    }
    if let Some(path) = screenshot {
        return Ok(Cli::Screenshot { path, scene });
    }
    if let Some(scene) = scene {
        return Ok(Cli::Scene { scene });
    }
    Ok(Cli::Run)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn empty_args_run_full_shell() {
        assert_eq!(parse_cli(&[]).unwrap(), Cli::Run);
    }

    #[test]
    fn scene_flag_routes_to_scene_variant() {
        let cli = parse_cli(&s(&["--scene", "modifier"])).unwrap();
        assert_eq!(
            cli,
            Cli::Scene {
                scene: BuiltinScene::Modifier
            }
        );
    }

    #[test]
    fn scene_list_routes_to_list_variant() {
        assert_eq!(
            parse_cli(&s(&["--scene", "list"])).unwrap(),
            Cli::ListScenes
        );
    }

    #[test]
    fn screenshot_flag_routes_to_screenshot_variant() {
        let cli = parse_cli(&s(&["--screenshot", "/tmp/out.json"])).unwrap();
        assert_eq!(
            cli,
            Cli::Screenshot {
                path: "/tmp/out.json".into(),
                scene: None,
            }
        );
    }

    #[test]
    fn screenshot_combines_with_scene() {
        let cli = parse_cli(&s(&[
            "--scene",
            "selection",
            "--screenshot",
            "/tmp/sel.json",
        ]))
        .unwrap();
        assert_eq!(
            cli,
            Cli::Screenshot {
                path: "/tmp/sel.json".into(),
                scene: Some(BuiltinScene::Selection),
            }
        );
    }

    #[test]
    fn unknown_scene_name_returns_error() {
        let err = parse_cli(&s(&["--scene", "made-up"])).unwrap_err();
        assert!(err.contains("unknown scene"));
    }

    #[test]
    fn unknown_flag_returns_error() {
        let err = parse_cli(&s(&["--bogus"])).unwrap_err();
        assert!(err.contains("unknown flag"));
    }
}
