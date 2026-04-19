//! Build module — the daemon side of Studio's self-replicating build
//! pipeline. Exposes `build.run_step`.
//!
//! Studio's [`BuilderManager`](../../../../prism-studio/src/kernel/builder-manager.ts)
//! composes a `BuildPlan` from an `AppProfile` + `BuildTarget` and dispatches
//! each step through this module. The daemon executes the step against the
//! real filesystem / process environment and returns captured stdout/stderr.
//!
//! Three step kinds are supported:
//!   - `emit-file` — write a file, creating parent dirs as needed.
//!   - `run-command` — spawn a child process, capture output, fail on
//!     non-zero exit.
//!   - `invoke-ipc` — placeholder for cross-command chaining; returns an
//!     error today since no plan emits these yet.
//!
//! Path resolution: relative paths in a step are resolved against
//! `working_dir`. Absolute paths are used as-is.

use crate::builder::DaemonBuilder;
use crate::module::DaemonModule;
use crate::registry::CommandError;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── Wire types (mirror @prism/core/builder) ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum BuildStep {
    EmitFile {
        path: String,
        contents: String,
        description: String,
    },
    RunCommand {
        command: String,
        args: Vec<String>,
        #[serde(default)]
        cwd: Option<String>,
        description: String,
    },
    InvokeIpc {
        name: String,
        payload: JsonValue,
        description: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildStepOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

pub struct BuildModule;

impl DaemonModule for BuildModule {
    fn id(&self) -> &str {
        "prism.build"
    }

    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        builder.registry().register("build.run_step", |payload| {
            let args: RunStepArgs = serde_json::from_value(payload)
                .map_err(|e| CommandError::handler("build.run_step", e.to_string()))?;
            let cwd = args
                .working_dir
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            let env = args.env.unwrap_or_default();
            let out = run_build_step(&args.step, &cwd, &env)
                .map_err(|e| CommandError::handler("build.run_step", e))?;
            serde_json::to_value(out)
                .map_err(|e| CommandError::handler("build.run_step", e.to_string()))
        })?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct RunStepArgs {
    step: BuildStep,
    #[serde(rename = "workingDir", default)]
    working_dir: Option<String>,
    #[serde(default)]
    env: Option<HashMap<String, String>>,
}

/// Execute a single [`BuildStep`]. Kept as a free function so the Studio
/// shell can call it directly without re-serializing through JSON.
pub fn run_build_step(
    step: &BuildStep,
    working_dir: &Path,
    env: &HashMap<String, String>,
) -> Result<BuildStepOutput, String> {
    match step {
        BuildStep::EmitFile { path, contents, .. } => emit_file(working_dir, path, contents),
        BuildStep::RunCommand {
            command, args, cwd, ..
        } => run_command(working_dir, command, args, cwd.as_deref(), env),
        BuildStep::InvokeIpc { name, .. } => Err(format!(
            "invoke-ipc step '{}' is not yet supported by the daemon",
            name
        )),
    }
}

fn emit_file(working_dir: &Path, path: &str, contents: &str) -> Result<BuildStepOutput, String> {
    let resolved = resolve_path(working_dir, path);
    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
    }
    std::fs::write(&resolved, contents)
        .map_err(|e| format!("failed to write {}: {}", resolved.display(), e))?;
    Ok(BuildStepOutput {
        stdout: Some(format!(
            "wrote {} ({} bytes)",
            resolved.display(),
            contents.len()
        )),
        stderr: None,
    })
}

fn run_command(
    working_dir: &Path,
    command: &str,
    args: &[String],
    cwd: Option<&str>,
    env: &HashMap<String, String>,
) -> Result<BuildStepOutput, String> {
    let effective_cwd = match cwd {
        Some(c) => resolve_path(working_dir, c),
        None => working_dir.to_path_buf(),
    };

    let mut cmd = Command::new(command);
    cmd.args(args).current_dir(&effective_cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }

    let output = cmd.output().map_err(|e| {
        format!(
            "failed to spawn '{}' in {}: {}",
            command,
            effective_cwd.display(),
            e
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    if !output.status.success() {
        let code = output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string());
        return Err(format!(
            "command '{} {}' exited with code {}\nstdout:\n{}\nstderr:\n{}",
            command,
            args.join(" "),
            code,
            stdout,
            stderr
        ));
    }

    Ok(BuildStepOutput {
        stdout: if stdout.is_empty() {
            None
        } else {
            Some(stdout)
        },
        stderr: if stderr.is_empty() {
            None
        } else {
            Some(stderr)
        },
    })
}

fn resolve_path(working_dir: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        working_dir.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;
    use serde_json::json;
    use tempfile::tempdir;

    fn empty_env() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn build_module_registers_run_step() {
        let kernel = DaemonBuilder::new().with_build().build().unwrap();
        assert!(kernel
            .capabilities()
            .contains(&"build.run_step".to_string()));
    }

    #[test]
    fn emit_file_writes_content_to_relative_path() {
        let dir = tempdir().unwrap();
        let step = BuildStep::EmitFile {
            path: "sub/profile.json".to_string(),
            contents: "{\"id\":\"flux\"}".to_string(),
            description: "emit flux profile".to_string(),
        };

        let out = run_build_step(&step, dir.path(), &empty_env()).unwrap();
        assert!(out.stdout.unwrap().contains("wrote"));

        let written = std::fs::read_to_string(dir.path().join("sub/profile.json")).unwrap();
        assert_eq!(written, "{\"id\":\"flux\"}");
    }

    #[test]
    fn emit_file_accepts_absolute_path() {
        let dir = tempdir().unwrap();
        let abs = dir.path().join("abs.txt");
        let step = BuildStep::EmitFile {
            path: abs.to_string_lossy().into_owned(),
            contents: "hello".to_string(),
            description: "".to_string(),
        };

        run_build_step(&step, Path::new("/tmp"), &empty_env()).unwrap();
        assert_eq!(std::fs::read_to_string(&abs).unwrap(), "hello");
    }

    #[test]
    fn emit_file_creates_missing_parent_directories() {
        let dir = tempdir().unwrap();
        let step = BuildStep::EmitFile {
            path: "deeply/nested/dir/file.txt".to_string(),
            contents: "x".to_string(),
            description: "".to_string(),
        };

        run_build_step(&step, dir.path(), &empty_env()).unwrap();
        assert!(dir.path().join("deeply/nested/dir/file.txt").exists());
    }

    #[test]
    fn run_command_captures_stdout() {
        let dir = tempdir().unwrap();
        let step = BuildStep::RunCommand {
            command: "echo".to_string(),
            args: vec!["hello".to_string(), "build".to_string()],
            cwd: None,
            description: "".to_string(),
        };

        let out = run_build_step(&step, dir.path(), &empty_env()).unwrap();
        assert_eq!(out.stdout.unwrap().trim(), "hello build");
    }

    #[test]
    fn run_command_fails_on_nonzero_exit() {
        let dir = tempdir().unwrap();
        let step = BuildStep::RunCommand {
            command: "false".to_string(),
            args: vec![],
            cwd: None,
            description: "".to_string(),
        };

        let err = run_build_step(&step, dir.path(), &empty_env()).unwrap_err();
        assert!(err.contains("exited with code"));
    }

    #[test]
    fn run_command_uses_step_cwd_relative_to_working_dir() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("marker.txt"), "here").unwrap();

        let step = BuildStep::RunCommand {
            command: "ls".to_string(),
            args: vec![],
            cwd: Some("sub".to_string()),
            description: "".to_string(),
        };

        let out = run_build_step(&step, dir.path(), &empty_env()).unwrap();
        assert!(out.stdout.unwrap().contains("marker.txt"));
    }

    #[test]
    fn run_command_passes_env_vars() {
        let dir = tempdir().unwrap();
        let mut env = HashMap::new();
        env.insert("PRISM_BUILD_TAG".to_string(), "alpha-test".to_string());

        let step = BuildStep::RunCommand {
            command: "sh".to_string(),
            args: vec!["-c".to_string(), "echo $PRISM_BUILD_TAG".to_string()],
            cwd: None,
            description: "".to_string(),
        };

        let out = run_build_step(&step, dir.path(), &env).unwrap();
        assert_eq!(out.stdout.unwrap().trim(), "alpha-test");
    }

    #[test]
    fn run_command_reports_spawn_failure() {
        let dir = tempdir().unwrap();
        let step = BuildStep::RunCommand {
            command: "this-command-does-not-exist-xyz".to_string(),
            args: vec![],
            cwd: None,
            description: "".to_string(),
        };

        let err = run_build_step(&step, dir.path(), &empty_env()).unwrap_err();
        assert!(err.contains("failed to spawn"));
    }

    #[test]
    fn invoke_ipc_returns_unsupported_error() {
        let dir = tempdir().unwrap();
        let step = BuildStep::InvokeIpc {
            name: "some.ipc".to_string(),
            payload: json!({}),
            description: "".to_string(),
        };

        let err = run_build_step(&step, dir.path(), &empty_env()).unwrap_err();
        assert!(err.contains("invoke-ipc"));
        assert!(err.contains("some.ipc"));
    }

    #[test]
    fn build_step_deserializes_kebab_kind() {
        let json_str = r#"{
            "kind": "emit-file",
            "path": "a.txt",
            "contents": "x",
            "description": "d"
        }"#;
        let step: BuildStep = serde_json::from_str(json_str).unwrap();
        match step {
            BuildStep::EmitFile { path, .. } => assert_eq!(path, "a.txt"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn registry_invoke_emits_file_through_kernel() {
        let kernel = DaemonBuilder::new().with_build().build().unwrap();
        let dir = tempdir().unwrap();

        let out = kernel
            .invoke(
                "build.run_step",
                json!({
                    "step": {
                        "kind": "emit-file",
                        "path": "hello.txt",
                        "contents": "hi",
                        "description": "d"
                    },
                    "workingDir": dir.path().to_string_lossy(),
                    "env": {},
                }),
            )
            .unwrap();

        assert!(out["stdout"].as_str().unwrap().contains("wrote"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("hello.txt")).unwrap(),
            "hi"
        );
    }
}
