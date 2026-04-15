//! Fluent builder for every external command the CLI shells out to.
//!
//! A [`CommandBuilder`] is the single place that knows how to turn a
//! high-level description ("run `cargo test` for `prism-core`" or
//! "serve `prism-shell` via trunk") into a concrete argv. The
//! subcommand modules compose these builders; the supervisor and
//! integration tests execute them.
//!
//! Two design goals drove this module:
//!
//! 1. **Inspectable.** Every builder exposes [`CommandBuilder::argv`]
//!    which returns the final `(program, args)` pair without
//!    running anything. Unit tests assert on argv directly, and the
//!    `--dry-run` flag on the CLI surfaces the argv to the user
//!    instead of executing.
//! 2. **No placeholder defaults.** Every method mutates explicit
//!    state; nothing magic happens at `build()` time. Makes it
//!    trivial to audit what `prism test --rust -p prism-core` will
//!    actually run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The external tool the builder wraps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Program {
    /// `cargo` — rustc + the wider cargo ecosystem. Covers every
    /// Rust-side target, including the packaged Studio binary and
    /// the emscripten-based web build.
    Cargo,
    /// `pnpm` — used to dispatch into the relay package.
    Pnpm,
    /// `python3` — ad-hoc static-file server for the web dev loop
    /// (`python3 -m http.server`). Pragmatic choice because the
    /// binary is already on every macOS/Linux dev machine we care
    /// about; swapped for a Rust static server if that becomes a
    /// portability problem.
    Python3,
}

impl Program {
    /// The executable name as it appears on `PATH`.
    pub fn executable(self) -> &'static str {
        match self {
            Program::Cargo => "cargo",
            Program::Pnpm => "pnpm",
            Program::Python3 => "python3",
        }
    }
}

/// Fluent builder for an external command.
#[derive(Debug, Clone)]
pub struct CommandBuilder {
    program: Program,
    /// Optional executable override. When set, [`Self::argv`] uses
    /// this as the program name instead of [`Program::executable`]
    /// and skips the program's prefix args. Only used by tests to
    /// spawn `/bin/sh` without leaking a new enum variant into the
    /// production API surface.
    program_override: Option<String>,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    env: BTreeMap<String, String>,
    label: Option<String>,
}

impl CommandBuilder {
    /// Start a new builder wrapping `program`.
    pub fn new(program: Program) -> Self {
        Self {
            program,
            program_override: None,
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            label: None,
        }
    }

    /// Shortcut: `cargo` builder.
    pub fn cargo() -> Self {
        Self::new(Program::Cargo)
    }

    /// Shortcut: `pnpm` builder.
    pub fn pnpm() -> Self {
        Self::new(Program::Pnpm)
    }

    /// Shortcut: `python3` builder.
    pub fn python3() -> Self {
        Self::new(Program::Python3)
    }

    /// Append a positional argument.
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Append multiple positional arguments in order.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set `--package <name>`. Cargo-only convenience.
    pub fn package(self, name: impl Into<String>) -> Self {
        self.arg("--package").arg(name)
    }

    /// Set `--workspace`. Cargo-only convenience.
    pub fn workspace(self) -> Self {
        self.arg("--workspace")
    }

    /// Set `--release`. Cargo-only convenience.
    pub fn release(self) -> Self {
        self.arg("--release")
    }

    /// Set the working directory the command runs in.
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Set an environment variable on the child process.
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Attach a human-readable label used by the supervisor for
    /// prefixed log output ("[shell] …").
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Read the label set via [`Self::label`], if any.
    pub fn label_str(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Read the program the builder wraps.
    pub fn program(&self) -> Program {
        self.program
    }

    /// Read the resolved working directory, if any.
    pub fn cwd_path(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    /// Return `(program, argv)` as the child process will see them.
    pub fn argv(&self) -> (&str, Vec<String>) {
        if let Some(override_program) = &self.program_override {
            return (override_program.as_str(), self.args.clone());
        }
        (self.program.executable(), self.args.clone())
    }

    /// Render the command as a shell-style string for `--dry-run`
    /// output and error messages. Not a real shell escaper — we
    /// wrap args containing whitespace in double quotes and leave
    /// the rest alone, which is sufficient for operator-facing
    /// diagnostics.
    pub fn display(&self) -> String {
        let (program, argv) = self.argv();
        let mut out = String::from(program);
        for arg in argv {
            out.push(' ');
            if arg.contains(char::is_whitespace) {
                out.push('"');
                out.push_str(&arg);
                out.push('"');
            } else {
                out.push_str(&arg);
            }
        }
        out
    }

    /// Materialise a synchronous [`std::process::Command`].
    pub fn build(&self) -> Command {
        let (program, argv) = self.argv();
        let mut cmd = Command::new(program);
        cmd.args(argv);
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        cmd
    }

    /// Materialise an async [`tokio::process::Command`].
    pub fn build_tokio(&self) -> tokio::process::Command {
        let (program, argv) = self.argv();
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(argv);
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        cmd
    }
}

/// Test-only construction helpers. Exposed `pub(crate)` so
/// sibling modules' `#[cfg(test)]` blocks can spawn short-lived
/// `/bin/sh` processes for the supervisor tests without leaking a
/// dedicated [`Program`] variant into the production API.
#[cfg(test)]
pub(crate) mod test_only {
    use super::*;

    /// Build a [`CommandBuilder`] that spawns `/bin/sh -c <script>`.
    /// Every supervisor integration test uses this to exercise
    /// real process lifecycle without depending on cargo.
    pub fn sh_builder(script: &str) -> CommandBuilder {
        CommandBuilder {
            program: Program::Cargo, // placeholder; overridden below
            program_override: Some("/bin/sh".to_string()),
            args: vec!["-c".to_string(), script.to_string()],
            cwd: None,
            env: BTreeMap::new(),
            label: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_test_workspace() {
        let cmd = CommandBuilder::cargo().arg("test").workspace();
        let (program, argv) = cmd.argv();
        assert_eq!(program, "cargo");
        assert_eq!(argv, vec!["test", "--workspace"]);
    }

    #[test]
    fn cargo_test_single_package() {
        let cmd = CommandBuilder::cargo().arg("test").package("prism-core");
        assert_eq!(cmd.argv().1, vec!["test", "--package", "prism-core"]);
    }

    #[test]
    fn cargo_build_release() {
        let cmd = CommandBuilder::cargo().arg("build").workspace().release();
        assert_eq!(cmd.argv().1, vec!["build", "--workspace", "--release"]);
    }

    #[test]
    fn python3_http_server_with_directory() {
        let cmd = CommandBuilder::python3()
            .arg("-m")
            .arg("http.server")
            .arg("1420")
            .arg("--directory")
            .arg("packages/prism-shell/web");
        let (program, argv) = cmd.argv();
        assert_eq!(program, "python3");
        assert_eq!(
            argv,
            vec![
                "-m",
                "http.server",
                "1420",
                "--directory",
                "packages/prism-shell/web"
            ]
        );
    }

    #[test]
    fn pnpm_filter_relay_dev() {
        let cmd = CommandBuilder::pnpm()
            .arg("--filter")
            .arg("@prism/relay")
            .arg("dev");
        let (program, argv) = cmd.argv();
        assert_eq!(program, "pnpm");
        assert_eq!(argv, vec!["--filter", "@prism/relay", "dev"]);
    }

    #[test]
    fn cwd_and_env_round_trip() {
        let cmd = CommandBuilder::cargo()
            .arg("test")
            .cwd("/tmp/fake")
            .env("RUST_LOG", "debug");
        assert_eq!(cmd.cwd_path(), Some(Path::new("/tmp/fake")));
        let built = cmd.build();
        assert_eq!(built.get_program(), "cargo");
        let envs: Vec<_> = built
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().to_string(),
                    v.map(|v| v.to_string_lossy().to_string()),
                )
            })
            .collect();
        assert!(envs
            .iter()
            .any(|(k, v)| k == "RUST_LOG" && v.as_deref() == Some("debug")));
    }

    #[test]
    fn label_is_optional_and_retained() {
        let cmd = CommandBuilder::cargo().arg("run").label("shell");
        assert_eq!(cmd.label_str(), Some("shell"));
        let unlabelled = CommandBuilder::cargo();
        assert_eq!(unlabelled.label_str(), None);
    }

    #[test]
    fn display_quotes_whitespace_args() {
        let cmd = CommandBuilder::cargo().arg("test").arg("has space");
        assert_eq!(cmd.display(), r#"cargo test "has space""#);
    }

    #[test]
    fn display_leaves_plain_args_bare() {
        let cmd = CommandBuilder::cargo().arg("test").package("prism-core");
        assert_eq!(cmd.display(), "cargo test --package prism-core");
    }

    #[test]
    fn args_iterator_accepts_mixed_types() {
        let cmd = CommandBuilder::cargo()
            .arg("test")
            .args(["--", "--nocapture"]);
        assert_eq!(cmd.argv().1, vec!["test", "--", "--nocapture"]);
    }
}
