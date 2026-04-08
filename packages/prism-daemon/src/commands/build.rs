//! Build step execution — the daemon side of Studio's self-replicating
//! build pipeline.
//!
//! Studio's BuilderManager composes a BuildPlan (a deterministic, JSON-
//! serializable list of BuildSteps) from an AppProfile + BuildTarget and
//! dispatches each step to the daemon via Tauri IPC. The daemon executes
//! the step in the real filesystem / process environment and returns a
//! structured result.
//!
//! Three step kinds are supported:
//!   - `emit-file`: write a file (creating parent dirs as needed).
//!   - `run-command`: spawn a child process, capture stdout/stderr,
//!     fail on non-zero exit code.
//!   - `invoke-ipc`: placeholder for cross-command chaining; currently
//!     returns an error since no plan emits these yet.
//!
//! Path resolution: every relative path in a step is resolved against the
//! `working_dir` passed by Studio (which comes from `BuildPlan.workingDir`).
//! Absolute paths are used as-is.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── Wire types (mirror @prism/core/builder) ─────────────────────────────

/// A single step in a BuildPlan. Deserialized from the JSON shape that
/// @prism/core/builder emits. The `kind` tag discriminates variants.
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
        payload: serde_json::Value,
        description: String,
    },
}

/// Structured result returned to the Tauri frontend. Matches the shape
/// the TS-side `createTauriExecutor` expects: `{ stdout?, stderr? }`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildStepOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

// ── Entry point ─────────────────────────────────────────────────────────

/// Execute a single BuildStep in the context of `working_dir`.
///
/// `working_dir` is the plan-level working directory (relative to the
/// monorepo root, or absolute). It's used to resolve relative paths for
/// `emit-file` and as the default cwd for `run-command`. `env` is an
/// extra set of environment variables applied to child processes.
pub fn run_build_step(
    step: &BuildStep,
    working_dir: &Path,
    env: &HashMap<String, String>,
) -> Result<BuildStepOutput, String> {
    match step {
        BuildStep::EmitFile { path, contents, .. } => emit_file(working_dir, path, contents),
        BuildStep::RunCommand {
            command,
            args,
            cwd,
            ..
        } => run_command(working_dir, command, args, cwd.as_deref(), env),
        BuildStep::InvokeIpc { name, .. } => Err(format!(
            "invoke-ipc step '{}' is not yet supported by the daemon",
            name
        )),
    }
}

// ── emit-file ───────────────────────────────────────────────────────────

fn emit_file(working_dir: &Path, path: &str, contents: &str) -> Result<BuildStepOutput, String> {
    let resolved = resolve_path(working_dir, path);
    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
    }
    std::fs::write(&resolved, contents)
        .map_err(|e| format!("failed to write {}: {}", resolved.display(), e))?;
    Ok(BuildStepOutput {
        stdout: Some(format!("wrote {} ({} bytes)", resolved.display(), contents.len())),
        stderr: None,
    })
}

// ── run-command ─────────────────────────────────────────────────────────

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
        stdout: if stdout.is_empty() { None } else { Some(stdout) },
        stderr: if stderr.is_empty() { None } else { Some(stderr) },
    })
}

// ── path resolution ─────────────────────────────────────────────────────

fn resolve_path(working_dir: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        working_dir.join(p)
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn empty_env() -> HashMap<String, String> {
        HashMap::new()
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

        // `ls` in the sub dir should list marker.txt
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
            payload: serde_json::json!({}),
            description: "".to_string(),
        };

        let err = run_build_step(&step, dir.path(), &empty_env()).unwrap_err();
        assert!(err.contains("invoke-ipc"));
        assert!(err.contains("some.ipc"));
    }

    #[test]
    fn build_step_deserializes_kebab_kind() {
        let json = r#"{
            "kind": "emit-file",
            "path": "a.txt",
            "contents": "x",
            "description": "d"
        }"#;
        let step: BuildStep = serde_json::from_str(json).unwrap();
        match step {
            BuildStep::EmitFile { path, .. } => assert_eq!(path, "a.txt"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_step_deserializes_run_command_without_cwd() {
        let json = r#"{
            "kind": "run-command",
            "command": "pnpm",
            "args": ["build"],
            "description": "d"
        }"#;
        let step: BuildStep = serde_json::from_str(json).unwrap();
        match step {
            BuildStep::RunCommand { command, args, cwd, .. } => {
                assert_eq!(command, "pnpm");
                assert_eq!(args, vec!["build"]);
                assert_eq!(cwd, None);
            }
            _ => panic!("wrong variant"),
        }
    }
}
