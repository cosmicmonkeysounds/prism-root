//! Walks a project's `scripts.widgets` declaration and registers the
//! resulting Luau widgets into a [`LuauRenderRegistry`] +
//! [`ComponentRegistry`].
//!
//! Phase 6d of `docs/dev/luau-integration-plan.md`. Capability scope
//! enforcement, hot-reload via the VFS watcher, and the `automations`
//! / `build_steps` / `commands` sections are tracked as follow-ups —
//! this file only handles the widgets pipeline so the shell can light
//! up Luau-defined widgets in the component palette.

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
        let report = load_widgets(
            &tmp,
            "widgets/*.luau",
            &mut luau,
            &mut components,
        )
        .unwrap();
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
        let report =
            load_widgets(&tmp, "nope/*.luau", &mut luau, &mut components).unwrap();
        assert!(report.loaded.is_empty());
        assert!(report.failures.is_empty());
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
        let report = load_widgets(
            &tmp,
            "widgets/*.luau",
            &mut luau,
            &mut components,
        )
        .unwrap();
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
