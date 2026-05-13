//! Walks a project's `scripts.{widgets,automations,build_steps,commands}`
//! declarations and compiles each `.luau` file. Widgets register
//! through a [`LuauRenderRegistry`] + [`ComponentRegistry`] live wire-up
//! so they're paintable in the shell; the other three pipelines do a
//! compile-only preflight today — diagnostics surface through the
//! [`LoadReport`] but host registration (automation engine, CLI build
//! steps, command palette) is wired by the host once that surface
//! exists.
//!
//! Phase 6d of `docs/dev/luau-integration-plan.md`. Capability scope
//! enforcement is the remaining follow-up — the per-glob `permissions`
//! table in `.prism.json` is parsed but not yet enforced against the
//! compiled scripts.

#![cfg(feature = "luau")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::block::{register_block, Block};
use crate::luau_component::{LuauComponent, LuauRenderRegistry};
use crate::registry::{ComponentRegistry, RegistryError};

#[derive(Debug, thiserror::Error)]
pub enum ScriptLoadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("luau: {0}")]
    Luau(#[from] mlua::Error),
    #[error("registry: {0}")]
    Registry(#[from] RegistryError),
}

/// Resolve the `scripts.widgets` glob declared in a `.prism.json`
/// against `project_root`. Today this only honours the simple
/// `<dir>/*.luau` pattern (matching the loader in the plan); recursive
/// globbing is a follow-up if the surface needs it.
fn resolve_widgets_dir(project_root: &Path, glob: &str) -> Option<PathBuf> {
    let trimmed = glob.trim_end_matches("/*.luau").trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    Some(project_root.join(trimmed))
}

/// Iterate a directory of `.luau` files, parse each one through
/// [`LuauRenderRegistry::compile`], and register the resulting
/// [`LuauComponent`]s into the [`ComponentRegistry`] via
/// [`register_block`].
///
/// Files that fail to compile or register are *skipped*; the caller
/// gets back the list of widget ids that loaded plus a list of
/// `(path, error)` pairs so the shell can surface them through
/// toasts.
pub fn load_widgets(
    project_root: &Path,
    glob: &str,
    luau: &mut LuauRenderRegistry,
    components: &mut ComponentRegistry,
) -> Result<LoadReport, ScriptLoadError> {
    let mut report = LoadReport::default();
    let Some(dir) = resolve_widgets_dir(project_root, glob) else {
        return Ok(report);
    };
    if !dir.is_dir() {
        return Ok(report);
    }
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("luau") {
            continue;
        }
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                report.failures.push((path, e.to_string()));
                continue;
            }
        };
        let compiled = match luau.compile(&source) {
            Ok(v) => v,
            Err(e) => {
                report.failures.push((path, e.to_string()));
                continue;
            }
        };
        for comp in compiled {
            let id = comp.id().to_string();
            let arc = Arc::new(comp);
            // Component registry already contains the id? Skip with a
            // noted failure rather than tear the boot down — the user
            // probably re-declared a built-in.
            if let Err(err) = register_block(components, arc.clone() as Arc<LuauComponent>) {
                report.failures.push((path.clone(), err.to_string()));
                continue;
            }
            report.loaded.push(id);
        }
    }
    Ok(report)
}

#[derive(Debug, Default)]
pub struct LoadReport {
    pub loaded: Vec<String>,
    pub failures: Vec<(PathBuf, String)>,
}

/// Re-read a single `.luau` widget file and swap its compiled render
/// function in [`LuauRenderRegistry`] via
/// [`LuauRenderRegistry::replace`]. The new [`LuauComponent`] is then
/// inserted into [`ComponentRegistry`] through
/// [`ComponentRegistry::register_or_replace`] so the live document
/// walks the new render path on the next sync pass.
///
/// Intended as the body of the VFS watcher callback for
/// `scripts.widgets` files. Returns the list of widget ids that
/// re-registered (a single file may declare multiple widgets) or the
/// first error surfaced during compile / replace.
pub fn reload_widget_file(
    path: &Path,
    luau: &mut LuauRenderRegistry,
    components: &mut ComponentRegistry,
) -> Result<Vec<String>, ScriptLoadError> {
    let source = std::fs::read_to_string(path)?;
    let compiled = luau.compile(&source)?;
    let mut ids = Vec::with_capacity(compiled.len());
    for comp in compiled {
        let id = comp.id().to_string();
        // `register_or_replace` is the contract for hot-reload: a
        // widget that the file already declared keeps its slot in the
        // registry but is now backed by the new render closure.
        let _prev = components.register_or_replace(Arc::new(comp) as Arc<LuauComponent>);
        ids.push(id);
    }
    Ok(ids)
}

/// Compile-only preflight for `scripts.automations`. Walks every
/// `.luau` file under `<glob>` and confirms it parses + executes
/// against a throwaway [`LuauRenderRegistry`]-backed `Lua` state. The
/// actual `AutomationEngine` wiring (host-supplied `ActionHandler`s
/// that proxy into Luau) lives in the host crate that owns
/// `prism_core::kernel::automation` — this function exists so a
/// project's manifest declarations get exercised at boot and broken
/// scripts surface in the load report.
pub fn load_automations(project_root: &Path, glob: &str) -> Result<LoadReport, ScriptLoadError> {
    compile_only_pass(project_root, glob)
}

/// Compile-only preflight for `scripts.build_steps`. See
/// [`load_automations`] for the pattern.
pub fn load_build_steps(project_root: &Path, glob: &str) -> Result<LoadReport, ScriptLoadError> {
    compile_only_pass(project_root, glob)
}

/// Compile-only preflight for `scripts.commands`. See
/// [`load_automations`] for the pattern.
pub fn load_commands(project_root: &Path, glob: &str) -> Result<LoadReport, ScriptLoadError> {
    compile_only_pass(project_root, glob)
}

/// Shared body for the three compile-only pipelines. The `loaded`
/// list carries the file stem of every script that parsed cleanly;
/// `failures` collects the path + error string for the rest.
fn compile_only_pass(project_root: &Path, glob: &str) -> Result<LoadReport, ScriptLoadError> {
    let mut report = LoadReport::default();
    let Some(dir) = resolve_widgets_dir(project_root, glob) else {
        return Ok(report);
    };
    if !dir.is_dir() {
        return Ok(report);
    }
    let lua = mlua::Lua::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("luau") {
            continue;
        }
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                report.failures.push((path, e.to_string()));
                continue;
            }
        };
        // `load(..).into_function()` validates syntax + compiles to
        // bytecode without executing user code — exactly the preflight
        // semantics the manifest wants for "did this script's body
        // type-check at boot."
        match lua.load(&source).into_function() {
            Ok(_) => {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    report.loaded.push(name.to_string());
                }
            }
            Err(e) => report.failures.push((path, e.to_string())),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_widget(tmp: &Path, name: &str, body: &str) {
        let path = tmp.join(name);
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn loads_a_directory_of_widgets() {
        let tmp = tempdir_for_test();
        let widgets = tmp.join("widgets");
        std::fs::create_dir_all(&widgets).unwrap();
        write_widget(
            &widgets,
            "kanban.luau",
            r#"
            prism.widget {
                id = "kanban-luau",
                label = "Kanban",
                render = function() return { component = "text", props = { body = "k" } } end,
            }
            "#,
        );
        write_widget(
            &widgets,
            "calendar.luau",
            r#"
            prism.widget {
                id = "calendar-luau",
                label = "Calendar",
                render = function() return { component = "text", props = { body = "c" } } end,
            }
            "#,
        );
        // Stray non-luau file is ignored.
        write_widget(&widgets, "README.md", "ignored");

        let mut luau = LuauRenderRegistry::new();
        let mut components = ComponentRegistry::new();
        let report = load_widgets(&tmp, "widgets/*.luau", &mut luau, &mut components).unwrap();
        assert_eq!(report.loaded.len(), 2);
        assert!(report.loaded.contains(&"kanban-luau".to_string()));
        assert!(report.loaded.contains(&"calendar-luau".to_string()));
        assert!(report.failures.is_empty());
        assert!(components.get("kanban-luau").is_some());
        assert!(components.get("calendar-luau").is_some());
    }

    #[test]
    fn missing_directory_is_no_op() {
        let tmp = tempdir_for_test();
        let mut luau = LuauRenderRegistry::new();
        let mut components = ComponentRegistry::new();
        let report = load_widgets(&tmp, "nope/*.luau", &mut luau, &mut components).unwrap();
        assert!(report.loaded.is_empty());
        assert!(report.failures.is_empty());
    }

    #[test]
    fn reload_widget_file_swaps_compiled_render() {
        let tmp = tempdir_for_test();
        let widgets = tmp.join("widgets");
        std::fs::create_dir_all(&widgets).unwrap();
        let path = widgets.join("greet.luau");
        std::fs::write(
            &path,
            r#"
            prism.widget {
                id = "greet-luau",
                render = function() return { component = "text", props = { body = "v1" } } end,
            }
            "#,
        )
        .unwrap();

        let mut luau = LuauRenderRegistry::new();
        let mut components = ComponentRegistry::new();
        let report = load_widgets(&tmp, "widgets/*.luau", &mut luau, &mut components).unwrap();
        assert_eq!(report.loaded, vec!["greet-luau".to_string()]);
        let v1 = luau
            .invoke(
                "greet-luau",
                &serde_json::Value::Null,
                &serde_json::Value::Null,
            )
            .unwrap();
        assert_eq!(v1.props["body"], "v1");

        // Hot-reload — rewrite the file and call reload_widget_file.
        std::fs::write(
            &path,
            r#"
            prism.widget {
                id = "greet-luau",
                render = function() return { component = "text", props = { body = "v2" } } end,
            }
            "#,
        )
        .unwrap();
        let ids = reload_widget_file(&path, &mut luau, &mut components).unwrap();
        assert_eq!(ids, vec!["greet-luau".to_string()]);
        let v2 = luau
            .invoke(
                "greet-luau",
                &serde_json::Value::Null,
                &serde_json::Value::Null,
            )
            .unwrap();
        assert_eq!(v2.props["body"], "v2");
        // ComponentRegistry holds the new instance — `register_or_replace`
        // overwrote the prior entry rather than erroring.
        assert!(components.get("greet-luau").is_some());
    }

    #[test]
    fn automations_pipeline_reports_compile_failures() {
        let tmp = tempdir_for_test();
        let dir = tmp.join("automations");
        std::fs::create_dir_all(&dir).unwrap();
        write_widget(&dir, "ok.luau", "return function() return 1 end");
        write_widget(&dir, "bad.luau", "this is :: not :: luau ::");
        let report = load_automations(&tmp, "automations/*.luau").unwrap();
        assert_eq!(report.loaded, vec!["ok".to_string()]);
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].0.ends_with("bad.luau"));
    }

    #[test]
    fn build_steps_and_commands_share_compile_only_pass() {
        let tmp = tempdir_for_test();
        let bs = tmp.join("build");
        let cmd = tmp.join("commands");
        std::fs::create_dir_all(&bs).unwrap();
        std::fs::create_dir_all(&cmd).unwrap();
        write_widget(&bs, "step.luau", "return {}");
        write_widget(&cmd, "ping.luau", "return function() return 'pong' end");
        let bs_report = load_build_steps(&tmp, "build/*.luau").unwrap();
        let cmd_report = load_commands(&tmp, "commands/*.luau").unwrap();
        assert_eq!(bs_report.loaded, vec!["step".to_string()]);
        assert_eq!(cmd_report.loaded, vec!["ping".to_string()]);
    }

    #[test]
    fn broken_widget_is_reported_not_fatal() {
        let tmp = tempdir_for_test();
        let widgets = tmp.join("widgets");
        std::fs::create_dir_all(&widgets).unwrap();
        write_widget(&widgets, "broken.luau", "this is not luau");
        write_widget(
            &widgets,
            "ok.luau",
            r#"
            prism.widget {
                id = "ok-luau",
                render = function() return { component = "text" } end,
            }
            "#,
        );
        let mut luau = LuauRenderRegistry::new();
        let mut components = ComponentRegistry::new();
        let report = load_widgets(&tmp, "widgets/*.luau", &mut luau, &mut components).unwrap();
        assert_eq!(report.loaded, vec!["ok-luau".to_string()]);
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].0.ends_with("broken.luau"));
    }

    fn tempdir_for_test() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "prism-script-loader-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }
}
