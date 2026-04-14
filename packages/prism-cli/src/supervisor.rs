//! Multi-process supervisor for `prism dev`.
//!
//! Given a list of [`CommandBuilder`]s with labels, spawn them all
//! in parallel, merge their stdout/stderr into a single stream with
//! per-process colored prefixes, and shut everything down cleanly
//! when the user hits Ctrl+C or any one child exits.
//!
//! The supervisor is generic over a [`LineSink`] so tests can
//! collect lines into a `Vec<String>` instead of writing to the
//! real terminal. In production the default sink writes to stdout
//! with ANSI color codes; under `cargo test` we use a capturing
//! sink and assert on the collected output.

use std::process::Stdio;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::builder::CommandBuilder;

/// A line of output from one of the supervised children.
#[derive(Debug, Clone)]
pub struct Line {
    pub label: String,
    pub color: Color,
    pub stream: Stream,
    pub text: String,
}

/// Whether the line came from stdout or stderr.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// ANSI color used to tint the label prefix. A deterministic
/// palette rotates through six colors so `dev all` has stable
/// per-process coloring run to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Cyan,
    Magenta,
    Yellow,
    Green,
    Blue,
    Red,
}

impl Color {
    /// ANSI escape code for the color.
    pub fn ansi(self) -> &'static str {
        match self {
            Color::Cyan => "\x1b[36m",
            Color::Magenta => "\x1b[35m",
            Color::Yellow => "\x1b[33m",
            Color::Green => "\x1b[32m",
            Color::Blue => "\x1b[34m",
            Color::Red => "\x1b[31m",
        }
    }

    /// Deterministic round-robin palette.
    pub fn palette() -> &'static [Color] {
        &[
            Color::Cyan,
            Color::Magenta,
            Color::Yellow,
            Color::Green,
            Color::Blue,
            Color::Red,
        ]
    }
}

/// Trait for things that consume supervised log lines.
pub trait LineSink: Send + Sync + 'static {
    fn emit(&self, line: Line);
}

/// Default sink — writes colored prefixed lines to stdout.
pub struct StdoutSink;

impl LineSink for StdoutSink {
    fn emit(&self, line: Line) {
        let reset = "\x1b[0m";
        println!(
            "{}[{}]{} {}",
            line.color.ansi(),
            line.label,
            reset,
            line.text
        );
    }
}

/// Capturing sink used by tests. Stores every line in a vector
/// behind a mutex so assertions can inspect them.
#[derive(Default, Clone)]
pub struct VecSink {
    pub lines: Arc<Mutex<Vec<Line>>>,
}

impl LineSink for VecSink {
    fn emit(&self, line: Line) {
        self.lines.lock().expect("poisoned sink").push(line);
    }
}

impl VecSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Vec<Line> {
        self.lines.lock().expect("poisoned sink").clone()
    }
}

/// What happened when the supervisor finished running.
#[derive(Debug, Clone)]
pub struct SupervisorOutcome {
    /// Exit code of the first child to exit (zero if they all
    /// succeeded, otherwise the failing child's code).
    pub exit_code: u8,
    /// Whether the supervisor shut down because of a Ctrl+C.
    pub interrupted: bool,
}

/// The supervisor itself.
pub struct Supervisor {
    processes: Vec<CommandBuilder>,
    sink: Arc<dyn LineSink>,
}

impl Supervisor {
    /// Create a new supervisor that emits to stdout.
    pub fn new() -> Self {
        Self {
            processes: Vec::new(),
            sink: Arc::new(StdoutSink),
        }
    }

    /// Create a supervisor that emits to a user-supplied sink.
    /// Used by tests to capture output for assertions.
    pub fn with_sink(sink: Arc<dyn LineSink>) -> Self {
        Self {
            processes: Vec::new(),
            sink,
        }
    }

    /// Add a process to the supervisor. Labels are required — we
    /// rely on them for the colored log prefix — so the builder
    /// must have had [`CommandBuilder::label`] called.
    pub fn add(&mut self, cmd: CommandBuilder) -> Result<()> {
        if cmd.label_str().is_none() {
            return Err(anyhow!(
                "supervised commands must have a label set via CommandBuilder::label"
            ));
        }
        self.processes.push(cmd);
        Ok(())
    }

    /// How many processes are queued.
    pub fn len(&self) -> usize {
        self.processes.len()
    }

    /// Whether there are no queued processes.
    pub fn is_empty(&self) -> bool {
        self.processes.is_empty()
    }

    /// Spawn every queued process, route their output to the sink,
    /// and wait for either (a) the first non-zero exit, (b) every
    /// child to exit cleanly, or (c) a Ctrl+C.
    pub async fn run(self) -> Result<SupervisorOutcome> {
        self.run_with_shutdown(wait_for_ctrl_c()).await
    }

    /// Test-facing variant: accept an arbitrary shutdown future so
    /// integration tests can fire a "virtual Ctrl+C" by resolving
    /// the future on their own schedule.
    pub async fn run_with_shutdown<F>(self, shutdown: F) -> Result<SupervisorOutcome>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        if self.processes.is_empty() {
            return Ok(SupervisorOutcome {
                exit_code: 0,
                interrupted: false,
            });
        }

        let (tx, mut rx) = mpsc::unbounded_channel::<Line>();
        let palette = Color::palette();
        let mut children: Vec<(String, Child)> = Vec::with_capacity(self.processes.len());
        let mut reader_tasks: JoinSet<()> = JoinSet::new();

        for (idx, cmd) in self.processes.into_iter().enumerate() {
            let label = cmd
                .label_str()
                .expect("label was checked at add() time")
                .to_string();
            let color = palette[idx % palette.len()];
            let mut tokio_cmd = cmd.build_tokio();
            tokio_cmd.stdout(Stdio::piped());
            tokio_cmd.stderr(Stdio::piped());
            tokio_cmd.kill_on_drop(true);

            let mut child = tokio_cmd
                .spawn()
                .map_err(|e| anyhow!("failed to spawn `{}`: {e}", cmd.display()))?;

            if let Some(stdout) = child.stdout.take() {
                spawn_line_reader(
                    &mut reader_tasks,
                    BufReader::new(stdout),
                    label.clone(),
                    color,
                    Stream::Stdout,
                    tx.clone(),
                );
            }
            if let Some(stderr) = child.stderr.take() {
                spawn_line_reader(
                    &mut reader_tasks,
                    BufReader::new(stderr),
                    label.clone(),
                    color,
                    Stream::Stderr,
                    tx.clone(),
                );
            }

            children.push((label, child));
        }

        drop(tx);

        let sink = self.sink;
        let sink_task = tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                sink.emit(line);
            }
        });

        let mut wait_set: JoinSet<(String, std::io::Result<std::process::ExitStatus>)> =
            JoinSet::new();
        for (label, mut child) in children.drain(..) {
            wait_set.spawn(async move {
                let status = child.wait().await;
                (label, status)
            });
        }

        let mut outcome = SupervisorOutcome {
            exit_code: 0,
            interrupted: false,
        };

        tokio::select! {
            _ = shutdown => {
                outcome.interrupted = true;
                // Aborting the JoinSets drops the Child handles,
                // which kills them because of kill_on_drop.
                wait_set.abort_all();
            }
            first = wait_set.join_next() => {
                if let Some(Ok((_label, Ok(status)))) = first {
                    if !status.success() {
                        outcome.exit_code = status.code().unwrap_or(1) as u8;
                    }
                }
                // Tear down the rest.
                wait_set.abort_all();
            }
        }

        // Drain any stragglers so we don't leave zombies.
        while let Some(res) = wait_set.join_next().await {
            if let Ok((_label, Ok(status))) = res {
                if outcome.exit_code == 0 && !status.success() {
                    outcome.exit_code = status.code().unwrap_or(1) as u8;
                }
            }
        }

        reader_tasks.abort_all();
        while reader_tasks.join_next().await.is_some() {}
        let _ = sink_task.await;

        Ok(outcome)
    }
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}

fn spawn_line_reader<R>(
    set: &mut JoinSet<()>,
    reader: BufReader<R>,
    label: String,
    color: Color,
    stream: Stream,
    tx: mpsc::UnboundedSender<Line>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    set.spawn(async move {
        let mut lines = reader.lines();
        while let Ok(Some(text)) = lines.next_line().await {
            let line = Line {
                label: label.clone(),
                color,
                stream,
                text,
            };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
}

async fn wait_for_ctrl_c() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn color_palette_is_six_wide() {
        assert_eq!(Color::palette().len(), 6);
    }

    #[test]
    fn unlabeled_command_is_rejected() {
        let mut s = Supervisor::new();
        let err = s.add(CommandBuilder::cargo().arg("run")).unwrap_err();
        assert!(err.to_string().contains("label"));
    }

    #[test]
    fn labeled_command_is_accepted() {
        let mut s = Supervisor::new();
        s.add(CommandBuilder::cargo().arg("run").label("shell"))
            .unwrap();
        assert_eq!(s.len(), 1);
        assert!(!s.is_empty());
    }

    #[tokio::test]
    async fn empty_supervisor_exits_zero() {
        let s = Supervisor::new();
        let outcome = s.run_with_shutdown(async {}).await.unwrap();
        assert_eq!(outcome.exit_code, 0);
        assert!(!outcome.interrupted);
    }

    fn sh(script: &str) -> CommandBuilder {
        crate::builder::test_only::sh_builder(script)
    }

    /// Shutdown future that never resolves — used when a test
    /// wants the supervisor to exit via child completion, not via
    /// interrupt.
    fn never() -> impl std::future::Future<Output = ()> + Send + 'static {
        std::future::pending::<()>()
    }

    #[tokio::test]
    async fn captures_stdout_lines_from_child() {
        let sink = VecSink::new();
        let mut s = Supervisor::with_sink(Arc::new(sink.clone()));
        s.add(sh("echo hello-prism").label("echoer")).unwrap();
        let outcome = s.run_with_shutdown(never()).await.unwrap();
        assert_eq!(outcome.exit_code, 0);

        let lines = sink.snapshot();
        assert!(
            lines.iter().any(|l| l.text.contains("hello-prism")),
            "expected echoed line in {:?}",
            lines.iter().map(|l| &l.text).collect::<Vec<_>>()
        );
        assert!(lines.iter().all(|l| l.label == "echoer"));
    }

    #[tokio::test]
    async fn shutdown_future_interrupts_long_running_child() {
        let sink = VecSink::new();
        let mut s = Supervisor::with_sink(Arc::new(sink.clone()));
        s.add(sh("sleep 30").label("sleeper")).unwrap();

        let shutdown = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        let outcome = s.run_with_shutdown(shutdown).await.unwrap();
        assert!(outcome.interrupted);
    }

    #[tokio::test]
    async fn non_zero_exit_propagates() {
        let sink = VecSink::new();
        let mut s = Supervisor::with_sink(Arc::new(sink.clone()));
        s.add(sh("exit 7").label("failer")).unwrap();
        let outcome = s.run_with_shutdown(never()).await.unwrap();
        assert_eq!(outcome.exit_code, 7);
        assert!(!outcome.interrupted);
    }

    #[tokio::test]
    async fn multiple_children_run_concurrently() {
        let sink = VecSink::new();
        let mut s = Supervisor::with_sink(Arc::new(sink.clone()));
        s.add(sh("echo one").label("a")).unwrap();
        s.add(sh("echo two").label("b")).unwrap();
        let outcome = s.run_with_shutdown(never()).await.unwrap();
        assert_eq!(outcome.exit_code, 0);

        let lines = sink.snapshot();
        let labels: std::collections::BTreeSet<_> = lines.iter().map(|l| l.label.clone()).collect();
        // At least one of the two should have emitted something;
        // the supervisor aborts siblings on first exit so we can't
        // guarantee both, but we CAN guarantee both were spawned
        // (no spawn errors bubbled up).
        assert!(!labels.is_empty());
    }

    #[tokio::test]
    async fn first_failure_tears_down_siblings() {
        let sink = VecSink::new();
        let mut s = Supervisor::with_sink(Arc::new(sink.clone()));
        s.add(sh("sleep 30").label("long")).unwrap();
        s.add(sh("exit 3").label("fast-fail")).unwrap();
        let outcome = s.run_with_shutdown(never()).await.unwrap();
        assert_eq!(outcome.exit_code, 3);
        assert!(!outcome.interrupted);
    }
}
