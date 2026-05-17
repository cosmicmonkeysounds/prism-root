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
//!   write a snapshot to `<path>`. Output format is chosen from
//!   the file extension: `*.png` runs the Wave 7.2 software
//!   rasteriser (`prism_shell::png_paint`) and emits an RGBA PNG;
//!   anything else (including `.json`) emits the deterministic
//!   JSON dump of the lowered UI tree. Implies "don't open a
//!   window". Default PNG viewport is 1280×800; override with
//!   `--width` / `--height`.

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

    if let Cli::Screenshot {
        path,
        width,
        height,
        ..
    } = &cli
    {
        let is_png = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("png"))
            .unwrap_or(false);
        if is_png {
            let bytes = shell.dump_png(*width, *height)?;
            std::fs::write(path, bytes)?;
        } else {
            std::fs::write(path, shell.dump_frame())?;
        }
        return Ok(());
    }

    if let Cli::Project { path } = &cli {
        shell.open_project(path.clone())?;
        return shell.run_with_project();
    }

    if matches!(cli, Cli::Run) && std::env::var("PRISM_WATCH_UI").is_ok()
        || matches!(cli, Cli::WatchUi)
    {
        // C3 — hot-reload watcher. Watches the bundled
        // `ui/app.prism-ui` skeleton + every per-app
        // `apps/<id>/shell.prism-ui` discovered at boot. Edits apply
        // in place without a cargo respawn.
        let specs = build_watch_specs();
        return shell.run_with_hot_reload(specs);
    }

    shell.run()
}

/// Build the list of `.prism-ui` files the C3 hot-reload watcher
/// observes. Always includes the canonical
/// `packages/prism-shell/ui/app.prism-ui` (the default skeleton);
/// each `apps/<id>/shell.prism-ui` is included when the file exists.
/// Paths resolve relative to the workspace root, walked up from the
/// current working directory until a `Cargo.toml` containing
/// `[workspace]` is found.
fn build_watch_specs() -> Vec<prism_shell::hot_reload::WatchSpec> {
    use prism_shell::hot_reload::{ReloadTarget, WatchSpec};
    let mut specs = Vec::new();
    let Some(workspace_root) = find_workspace_root() else {
        return specs;
    };

    let app = workspace_root.join("packages/prism-shell/ui/app.prism-ui");
    if app.exists() {
        specs.push(WatchSpec {
            path: app,
            target: ReloadTarget::DefaultSkeleton,
        });
    }

    // §3.2 — host `.prss` stylesheet. Saving it re-classifies through
    // the `PrssFingerprintCache` and reinstalls live.
    let host_prss = workspace_root.join("packages/prism-shell/ui/app.prss");
    if host_prss.exists() {
        specs.push(WatchSpec {
            path: host_prss,
            target: ReloadTarget::Stylesheet { app_id: None },
        });
    }

    let apps_dir = workspace_root.join("apps");
    if let Ok(entries) = std::fs::read_dir(&apps_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skeleton = path.join("shell.prism-ui");
            if !skeleton.exists() {
                continue;
            }
            let Some(app_id) = path.file_name().and_then(|s| s.to_str()).map(String::from) else {
                continue;
            };
            specs.push(WatchSpec {
                path: skeleton,
                target: ReloadTarget::AppSkeleton {
                    app_id: app_id.clone(),
                },
            });
            // Sibling per-app stylesheet, if the app ships one.
            let app_prss = path.join("shell.prss");
            if app_prss.exists() {
                specs.push(WatchSpec {
                    path: app_prss,
                    target: ReloadTarget::Stylesheet {
                        app_id: Some(app_id),
                    },
                });
            }
        }
    }
    specs
}

fn find_workspace_root() -> Option<std::path::PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let manifest = dir.join("Cargo.toml");
        if manifest.exists() {
            if let Ok(s) = std::fs::read_to_string(&manifest) {
                if s.contains("[workspace]") {
                    return Some(dir);
                }
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[derive(Debug, PartialEq)]
enum Cli {
    /// No flags — boot into the femtovg event loop.
    Run,
    /// `--watch-ui` — boot the event loop with a hot-reload
    /// watcher attached to `ui/app.prism-ui` + every
    /// `apps/<id>/shell.prism-ui`. Closes C3 of
    /// `docs/dev/ui-migration-followups.md`.
    WatchUi,
    /// `--scene <name>` — apply the scene, run the event loop.
    Scene { scene: BuiltinScene },
    /// `--scene list` — print the scene names and exit.
    ListScenes,
    /// `--screenshot <path>` (optionally combined with `--scene`).
    /// `width`/`height` set the headless viewport in CSS pixels —
    /// only consumed by the PNG branch (the JSON branch is
    /// resolution-independent).
    Screenshot {
        path: String,
        scene: Option<BuiltinScene>,
        width: u32,
        height: u32,
    },
    /// `--project <path>` — open a Project Vault
    /// (`docs/dev/project-vault.md`) and run the event loop with the
    /// folder watcher driving the live object graph.
    Project { path: String },
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
    let mut watch_ui = false;
    let mut project: Option<String> = None;
    let mut width: u32 = 1280;
    let mut height: u32 = 800;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--watch-ui" => {
                watch_ui = true;
                i += 1;
            }
            "--project" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| "--project requires a path".to_string())?;
                project = Some(value.clone());
                i += 2;
            }
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
            "--width" => {
                width = args
                    .get(i + 1)
                    .ok_or_else(|| "--width requires a value".to_string())?
                    .parse()
                    .map_err(|_| "--width must be a positive integer".to_string())?;
                i += 2;
            }
            "--height" => {
                height = args
                    .get(i + 1)
                    .ok_or_else(|| "--height requires a value".to_string())?
                    .parse()
                    .map_err(|_| "--height must be a positive integer".to_string())?;
                i += 2;
            }
            other => return Err(format!("unknown flag '{other}'")),
        }
    }
    if list_scenes {
        return Ok(Cli::ListScenes);
    }
    if let Some(path) = project {
        return Ok(Cli::Project { path });
    }
    if let Some(path) = screenshot {
        return Ok(Cli::Screenshot {
            path,
            scene,
            width,
            height,
        });
    }
    if let Some(scene) = scene {
        return Ok(Cli::Scene { scene });
    }
    if watch_ui {
        return Ok(Cli::WatchUi);
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
                width: 1280,
                height: 800,
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
                width: 1280,
                height: 800,
            }
        );
    }

    /// Wave 7.2 — `--width` / `--height` flags ride alongside
    /// `--screenshot` to set the headless viewport for the PNG
    /// branch. The JSON branch ignores them (it has no resolution).
    #[test]
    fn screenshot_with_explicit_viewport_flags() {
        let cli = parse_cli(&s(&[
            "--screenshot",
            "/tmp/out.png",
            "--width",
            "640",
            "--height",
            "480",
        ]))
        .unwrap();
        assert_eq!(
            cli,
            Cli::Screenshot {
                path: "/tmp/out.png".into(),
                scene: None,
                width: 640,
                height: 480,
            }
        );
    }

    /// Wave 7.2 — invalid `--width` value falls through with a
    /// typed error rather than panicking.
    #[test]
    fn invalid_width_returns_error() {
        let err = parse_cli(&s(&["--width", "abc"])).unwrap_err();
        assert!(err.contains("--width"));
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
