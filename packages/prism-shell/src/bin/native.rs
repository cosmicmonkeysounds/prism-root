//! `prism-shell` native dev binary.
//!
//! Standalone entry point used during the inner dev loop. Supports
//! CLI flags to skip the launchpad and jump directly to a specific
//! app, panel, or predefined test scene — eliminating the need for
//! manual click-through when testing specific views.
//!
//! Examples:
//!   prism-shell --app lattice --panel builder
//!   prism-shell --scene builder-empty
//!   prism-shell --scene builder-tablet --screenshot /tmp/tablet.png
//!   prism-shell --e2e                          # run all e2e tests
//!   prism-shell --e2e --e2e-test viewport-switching  # run one test

use clap::Parser;
#[cfg(feature = "e2e")]
use prism_shell::e2e::InputMode;
use prism_shell::e2e::{self, E2eDriver};
use prism_shell::testing::{apply_scene, BuiltinScene, ShellScreenshot};
use prism_shell::{AppState, Shell};

#[derive(Debug, Parser)]
#[command(
    name = "prism-shell",
    about = "Prism Shell — native dev binary with test harness and e2e support."
)]
struct Args {
    /// Jump directly into a named app (case-insensitive match on app
    /// name). Skips the launchpad.
    #[arg(long)]
    app: Option<String>,

    /// Start on a specific panel: identity, builder, code-editor, explorer.
    #[arg(long)]
    panel: Option<String>,

    /// Load a predefined test scene instead of the default state.
    /// Overrides --app and --panel. Use `--scene list` to see options.
    #[arg(long)]
    scene: Option<String>,

    /// Take a screenshot after the shell renders and save to this path,
    /// then exit. Requires a scene or app to be specified.
    #[arg(long)]
    screenshot: Option<String>,

    /// Set the viewport preset: desktop (1280), tablet (768), mobile (375).
    #[arg(long)]
    viewport: Option<String>,

    /// Set initial zoom level (e.g. 0.5, 1.0, 2.0).
    #[arg(long)]
    zoom: Option<f32>,

    // ── E2E testing flags ───────────────────────────────────────
    /// Run in e2e test mode. Executes built-in test scripts and exits.
    #[arg(long)]
    e2e: bool,

    /// Run a single named e2e test (implies --e2e).
    #[arg(long)]
    e2e_test: Option<String>,

    /// Capture baseline screenshots instead of running assertions.
    #[arg(long)]
    e2e_record: bool,

    /// Output directory for e2e screenshots.
    #[arg(long)]
    e2e_output: Option<String>,

    /// Use OS-level input injection (requires display + accessibility).
    #[arg(long)]
    e2e_os_input: bool,

    /// Step delay in milliseconds between e2e actions (default: 16).
    #[arg(long)]
    e2e_step_delay: Option<u64>,

    /// List available e2e tests and exit (used with --e2e).
    #[arg(long)]
    list: bool,
}

fn main() -> Result<(), slint::PlatformError> {
    let args = Args::parse();

    // ── E2E mode ────────────────────────────────────────────────

    if args.e2e || args.e2e_test.is_some() || (args.list && args.scene.is_none()) {
        return run_e2e(&args);
    }

    // ── Scene listing ───────────────────────────────────────────

    if args.scene.as_deref() == Some("list") {
        println!("Available scenes:");
        for scene in BuiltinScene::all() {
            println!("  {:<24} {}", scene.name(), scene.description());
        }
        return Ok(());
    }

    // ── Normal shell startup ────────────────────────────────────

    let mut state = if let Some(scene_name) = &args.scene {
        let scene = BuiltinScene::by_name(scene_name).unwrap_or_else(|| {
            eprintln!("Unknown scene: {scene_name}. Use --scene list to see options.");
            std::process::exit(1);
        });
        apply_scene(scene)
    } else {
        let mut state = AppState::default();
        if let Some(app_name) = &args.app {
            let lower = app_name.to_lowercase();
            if let Some(app) = state.apps.iter().find(|a| a.name.to_lowercase() == lower) {
                let app_id = app.id.clone();
                state.shell_view = prism_shell::app::ShellView::App { app_id };
                state.sync_document_from_app_pub();
            } else {
                eprintln!("No app named '{app_name}'. Available:");
                for a in &state.apps {
                    eprintln!("  {}", a.name);
                }
                std::process::exit(1);
            }
        }
        if let Some(panel_name) = &args.panel {
            let page_id = prism_shell::app::page_id_for_panel(panel_name);
            if page_id == "edit"
                && ![
                    "identity",
                    "builder",
                    "edit",
                    "code-editor",
                    "code",
                    "explorer",
                    "design",
                    "fusion",
                ]
                .contains(&panel_name.as_str())
            {
                eprintln!("Unknown panel: {panel_name}. Options: identity, builder, code-editor, explorer, design, fusion");
                std::process::exit(1);
            }
            state.workspace.switch_page_by_id(page_id);
        }
        state
    };

    if let Some(vp) = &args.viewport {
        state.viewport_width = match vp.as_str() {
            "tablet" => 768.0,
            "mobile" => 375.0,
            "desktop" => 1280.0,
            _ => vp.parse().unwrap_or_else(|_| {
                eprintln!("Invalid viewport: {vp}. Use desktop/tablet/mobile or a number.");
                std::process::exit(1);
            }),
        };
    }

    let shell = Shell::from_state(state)?;

    if let Some(zoom) = args.zoom {
        shell.window().set_canvas_zoom(zoom);
    }

    if let Some(screenshot_path) = &args.screenshot {
        let path = screenshot_path.clone();
        ShellScreenshot::schedule_and_exit(&shell, &path);
    }

    shell.run()
}

fn run_e2e(args: &Args) -> Result<(), slint::PlatformError> {
    if args.list {
        println!("Available e2e tests:");
        for script in e2e::builtin_scripts() {
            println!("  {:<32} {}", script.name, script.description);
        }
        return Ok(());
    }

    let shell = Shell::new()?;
    let mut driver = E2eDriver::new(shell);

    if let Some(ref dir) = args.e2e_output {
        driver = driver.with_screenshot_dir(dir.clone());
    }

    if let Some(delay) = args.e2e_step_delay {
        driver = driver.with_step_delay(delay);
    }

    #[cfg(feature = "e2e")]
    if args.e2e_os_input {
        driver = driver.with_mode(InputMode::OsInput);
    }
    #[cfg(not(feature = "e2e"))]
    if args.e2e_os_input {
        eprintln!("OS input mode requires --features e2e");
        std::process::exit(1);
    }

    let scripts = if let Some(ref name) = args.e2e_test {
        match e2e::builtin_script_by_name(name) {
            Some(s) => vec![s],
            None => {
                eprintln!("Unknown e2e test: {name}. Use --e2e --list to see options.");
                std::process::exit(1);
            }
        }
    } else {
        e2e::builtin_scripts()
    };

    if args.e2e_record {
        eprintln!("Recording baselines...");
        for script in &scripts {
            if let Some(ref scene_name) = script.scene {
                if let Some(scene) = BuiltinScene::by_name(scene_name) {
                    let new_state = apply_scene(scene);
                    let bytes = serde_json::to_vec(&new_state).unwrap();
                    let _ = driver.shell().restore(&bytes);
                }
            }
            let name = &script.name;
            match driver.capture_baseline(name) {
                Some(path) => eprintln!("  {name} -> {}", path.display()),
                None => eprintln!("  {name} -> FAILED"),
            }
        }
        return Ok(());
    }

    let result = driver.run_suite(&scripts);
    print!("{}", result.summary());

    if !result.all_passed() {
        std::process::exit(1);
    }
    Ok(())
}
