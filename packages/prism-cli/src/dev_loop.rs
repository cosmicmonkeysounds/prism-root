//! Rebuild-and-respawn dev loop for `prism dev shell`.
//!
//! Phase 1 of the Slint migration plan (§7 + §11) splits hot-reload
//! into two orthogonal layers:
//!
//! 1. **`.slint` → Slint's native live-preview.** When the shell is
//!    built with `SLINT_LIVE_PREVIEW=1` + the
//!    `prism-shell/live-preview` feature, `slint-build` replaces the
//!    baked `AppWindow` codegen with a `LiveReloadingComponent` wrapper
//!    that parses `ui/app.slint` at runtime via `slint-interpreter`
//!    and reloads it automatically whenever the file changes. That
//!    leg is entirely in-process — the dev loop does not need to
//!    know anything about `.slint` files.
//! 2. **`.rs` → respawn.** Rust source changes can't be hot-swapped
//!    without something like `subsecond` (Phase 4). Until then the
//!    dev loop wraps the cargo child in a [`WatchLoop`], and any
//!    `.rs` change under the watched crate roots triggers a kill +
//!    cargo respawn. cargo's incremental compilation keeps iteration
//!    reasonably fast.
//!
//! This module is the second half. Callers build a [`DevLoop`] with
//! a child [`CommandBuilder`] and one or more paths to watch; the
//! loop spawns the child, routes its stdout/stderr through the same
//! [`LineSink`] contract `supervisor` uses, and on each filesystem
//! batch kills the child and respawns a fresh one.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::builder::CommandBuilder;
use crate::supervisor::{Color, Line, LineSink, StdoutSink, Stream};
use crate::watch::{WatchBatch, WatchLoop};

/// File-extension filter applied to each [`WatchBatch`]. Only paths
/// with one of these extensions survive the filter; the rest are
/// treated as filesystem noise (editor swap files, target/ artifacts
/// if the watch root straddles them, formatter side-writes, etc.)
/// and do not trigger a respawn.
pub const DEFAULT_EXTENSIONS: &[&str] = &["rs"];

/// Short interval the blocking watcher task uses when polling each
/// underlying [`WatchLoop`]. Kept deliberately small so shutdown
/// latency stays imperceptible — the tradeoff is a bit of wasted
/// wakeup work on an idle tree, which is fine for dev.
const WATCHER_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Handle to a single rebuild-and-respawn dev loop.
///
/// [`DevLoop::run`] blocks on `Ctrl+C`; [`DevLoop::run_with_shutdown`]
/// accepts an explicit shutdown future so tests can simulate
/// interruption on their own schedule (same pattern the `supervisor`
/// module uses).
pub struct DevLoop {
    cmd: CommandBuilder,
    watch_paths: Vec<PathBuf>,
    debounce: Duration,
    extensions: Vec<String>,
    sink: Arc<dyn LineSink>,
    color: Color,
}

/// Outcome of a [`DevLoop::run`] invocation.
#[derive(Debug, Clone)]
pub struct DevLoopOutcome {
    /// Exit code of the final child process. `0` when the loop was
    /// interrupted via the shutdown future (Ctrl+C).
    pub exit_code: u8,
    /// True when the loop tore down because of the shutdown future
    /// rather than a child exit.
    pub interrupted: bool,
    /// How many times the child was killed + respawned because of a
    /// filesystem batch. Exposed so tests can assert on the reload
    /// path without scraping log output.
    pub restart_count: usize,
}

impl DevLoop {
    /// Build a loop that runs `cmd` and watches `watch_paths` for
    /// `.rs` changes. Every path must already exist — construction
    /// fails early otherwise so dev-server surprises stay cheap.
    pub fn new(cmd: CommandBuilder, watch_paths: Vec<PathBuf>) -> Self {
        Self {
            cmd,
            watch_paths,
            debounce: WatchLoop::DEFAULT_DEBOUNCE,
            extensions: DEFAULT_EXTENSIONS
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            sink: Arc::new(StdoutSink),
            color: Color::Cyan,
        }
    }

    /// Route stdout/stderr lines through a caller-supplied sink.
    /// Used by tests to capture output with a [`VecSink`].
    pub fn with_sink(mut self, sink: Arc<dyn LineSink>) -> Self {
        self.sink = sink;
        self
    }

    /// Override the debounce window passed to each underlying
    /// [`WatchLoop`]. Defaults to [`WatchLoop::DEFAULT_DEBOUNCE`].
    pub fn with_debounce(mut self, debounce: Duration) -> Self {
        self.debounce = debounce;
        self
    }

    /// Override the extension filter. Empty means "respawn on any
    /// filesystem change" — not recommended, but useful for tests
    /// that touch sentinel files.
    pub fn with_extensions<I, S>(mut self, extensions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extensions = extensions.into_iter().map(Into::into).collect();
        self
    }

    /// Override the prefix color used for log lines. Defaults to
    /// [`Color::Cyan`] so `prism dev shell` lines match the
    /// supervisor's first-slot color for a single-target dev.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Run the loop until Ctrl+C, the child exits on its own, or the
    /// watcher task dies unexpectedly.
    pub async fn run(self) -> Result<DevLoopOutcome> {
        self.run_with_shutdown(wait_for_ctrl_c()).await
    }

    /// Same as [`DevLoop::run`] but with a caller-supplied shutdown
    /// future so integration tests can fire a "virtual Ctrl+C".
    pub async fn run_with_shutdown<F>(self, shutdown: F) -> Result<DevLoopOutcome>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        if self.watch_paths.is_empty() {
            return Err(anyhow!("DevLoop needs at least one watch path"));
        }

        // Eagerly construct every watcher so path errors bubble up
        // at `run` time rather than silently degrading into a
        // no-reload loop. Each WatchLoop owns its own notify backend
        // because notify's recursive watcher can only straddle a
        // single root.
        let loops: Vec<WatchLoop> = self
            .watch_paths
            .iter()
            .map(|p| build_watch_loop(p, self.debounce))
            .collect::<Result<Vec<_>>>()?;

        let label = self.cmd.label_str().unwrap_or("dev").to_string();
        let color = self.color;
        let sink = self.sink.clone();

        // Bridge the blocking notify watcher into the async loop via
        // a tokio mpsc channel. The blocking task polls each
        // [`WatchLoop`] in round-robin with a short timeout so it
        // can react to stop requests quickly.
        let (batch_tx, mut batch_rx) = mpsc::unbounded_channel::<WatchBatch>();
        let (stop_watcher_tx, stop_watcher_rx) = std::sync::mpsc::channel::<()>();
        let extensions = self.extensions.clone();
        let watcher_task = tokio::task::spawn_blocking(move || {
            run_watcher_bridge(loops, extensions, batch_tx, stop_watcher_rx);
        });

        let mut outcome = DevLoopOutcome {
            exit_code: 0,
            interrupted: false,
            restart_count: 0,
        };

        let mut shutdown = Box::pin(shutdown);

        loop {
            let mut tokio_cmd = self.cmd.build_tokio();
            tokio_cmd.stdout(Stdio::piped());
            tokio_cmd.stderr(Stdio::piped());
            tokio_cmd.kill_on_drop(true);
            let mut child: Child = tokio_cmd
                .spawn()
                .map_err(|e| anyhow!("failed to spawn `{}`: {e}", self.cmd.display()))?;

            let (tx, mut rx) = mpsc::unbounded_channel::<Line>();
            let mut reader_tasks: JoinSet<()> = JoinSet::new();
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
            drop(tx);

            let sink_clone = sink.clone();
            let sink_task = tokio::spawn(async move {
                while let Some(line) = rx.recv().await {
                    sink_clone.emit(line);
                }
            });

            enum InnerEvent {
                ChildDone(std::io::Result<std::process::ExitStatus>),
                Restart(WatchBatch),
                WatcherDied,
                Shutdown,
            }

            let event = tokio::select! {
                status = child.wait() => InnerEvent::ChildDone(status),
                maybe_batch = batch_rx.recv() => match maybe_batch {
                    Some(batch) => InnerEvent::Restart(batch),
                    None => InnerEvent::WatcherDied,
                },
                _ = &mut shutdown => InnerEvent::Shutdown,
            };

            match event {
                InnerEvent::ChildDone(status) => {
                    drain_child_io(&mut reader_tasks, sink_task).await;
                    outcome.exit_code = status.map(|s| s.code().unwrap_or(1) as u8).unwrap_or(1);
                    break;
                }
                InnerEvent::Restart(batch) => {
                    emit_notice(
                        &sink,
                        &label,
                        color,
                        format!(
                            "change detected ({} file{}) — restarting",
                            batch.paths.len(),
                            if batch.paths.len() == 1 { "" } else { "s" }
                        ),
                    );
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    drain_child_io(&mut reader_tasks, sink_task).await;
                    outcome.restart_count += 1;
                    continue;
                }
                InnerEvent::WatcherDied => {
                    // Watcher thread failed silently. Don't leave
                    // the dev loop hanging — surface it and wait for
                    // the child so the user still gets the child's
                    // exit code instead of a phantom success.
                    emit_notice(
                        &sink,
                        &label,
                        color,
                        "watcher thread exited — continuing without hot-reload".to_string(),
                    );
                    let status = child.wait().await;
                    drain_child_io(&mut reader_tasks, sink_task).await;
                    outcome.exit_code = status.map(|s| s.code().unwrap_or(1) as u8).unwrap_or(1);
                    break;
                }
                InnerEvent::Shutdown => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    drain_child_io(&mut reader_tasks, sink_task).await;
                    outcome.interrupted = true;
                    break;
                }
            }
        }

        let _ = stop_watcher_tx.send(());
        // `spawn_blocking` tasks can't be cancelled — the blocking
        // thread exits on its own at the next poll. Give it a short
        // deadline so `run` doesn't hang indefinitely if the watcher
        // is stuck inside notify.
        let _ = tokio::time::timeout(Duration::from_millis(500), watcher_task).await;

        Ok(outcome)
    }
}

fn build_watch_loop(path: &Path, debounce: Duration) -> Result<WatchLoop> {
    WatchLoop::with_debounce(path, debounce)
        .map_err(|e| anyhow!("failed to watch `{}`: {e}", path.display()))
}

fn run_watcher_bridge(
    loops: Vec<WatchLoop>,
    extensions: Vec<String>,
    batch_tx: mpsc::UnboundedSender<WatchBatch>,
    stop_rx: std::sync::mpsc::Receiver<()>,
) {
    loop {
        if stop_rx.try_recv().is_ok() {
            return;
        }
        for loop_ in &loops {
            if let Some(batch) = loop_.next_batch(WATCHER_POLL_INTERVAL) {
                let filtered = filter_batch(&batch, &extensions);
                if !filtered.is_empty() && batch_tx.send(WatchBatch { paths: filtered }).is_err() {
                    return;
                }
            }
        }
    }
}

fn filter_batch(batch: &WatchBatch, extensions: &[String]) -> Vec<PathBuf> {
    if extensions.is_empty() {
        return batch.paths.clone();
    }
    batch
        .paths
        .iter()
        .filter(|p| {
            p.extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| extensions.iter().any(|e| e == ext))
        })
        .cloned()
        .collect()
}

fn emit_notice(sink: &Arc<dyn LineSink>, label: &str, color: Color, text: String) {
    sink.emit(Line {
        label: label.to_string(),
        color,
        stream: Stream::Stdout,
        text: format!("prism dev: {text}"),
    });
}

async fn drain_child_io(reader_tasks: &mut JoinSet<()>, sink_task: tokio::task::JoinHandle<()>) {
    reader_tasks.abort_all();
    while reader_tasks.join_next().await.is_some() {}
    let _ = sink_task.await;
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
    use crate::builder::test_only::sh_builder;
    use crate::supervisor::VecSink;
    use std::fs;
    use std::time::Duration;
    use tempfile::tempdir;

    fn sh(script: &str) -> CommandBuilder {
        sh_builder(script).label("dev-loop-test")
    }

    fn never() -> impl std::future::Future<Output = ()> + Send + 'static {
        std::future::pending::<()>()
    }

    #[tokio::test]
    async fn empty_watch_paths_rejected() {
        let cmd = sh("echo hi");
        let loop_ = DevLoop::new(cmd, vec![]);
        let err = loop_.run_with_shutdown(never()).await.unwrap_err();
        assert!(err.to_string().contains("at least one watch path"));
    }

    #[tokio::test]
    async fn child_exit_propagates_exit_code() {
        let dir = tempdir().expect("tempdir");
        let sink = VecSink::new();
        let loop_ = DevLoop::new(sh("exit 5"), vec![dir.path().to_path_buf()])
            .with_sink(Arc::new(sink.clone()));
        let outcome = loop_.run_with_shutdown(never()).await.unwrap();
        assert_eq!(outcome.exit_code, 5);
        assert_eq!(outcome.restart_count, 0);
        assert!(!outcome.interrupted);
    }

    #[tokio::test]
    async fn shutdown_future_interrupts_long_running_child() {
        let dir = tempdir().expect("tempdir");
        let sink = VecSink::new();
        let loop_ = DevLoop::new(sh("sleep 30"), vec![dir.path().to_path_buf()])
            .with_sink(Arc::new(sink.clone()));
        let shutdown = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        let outcome = loop_.run_with_shutdown(shutdown).await.unwrap();
        assert!(outcome.interrupted);
        assert_eq!(outcome.restart_count, 0);
    }

    #[tokio::test]
    async fn stdout_is_routed_through_the_sink() {
        let dir = tempdir().expect("tempdir");
        let sink = VecSink::new();
        let loop_ = DevLoop::new(sh("echo hot-reload-hello"), vec![dir.path().to_path_buf()])
            .with_sink(Arc::new(sink.clone()));
        let outcome = loop_.run_with_shutdown(never()).await.unwrap();
        assert_eq!(outcome.exit_code, 0);
        let lines = sink.snapshot();
        assert!(
            lines.iter().any(|l| l.text.contains("hot-reload-hello")),
            "expected child stdout in sink, got {:?}",
            lines.iter().map(|l| &l.text).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn touching_a_rust_file_respawns_the_child() {
        let dir = tempdir().expect("tempdir");
        let counter = dir.path().join("count.txt");
        // Long-running child that records its own PID on each
        // launch, so the respawn is observable from the test.
        let script = format!("echo launched >> {}; sleep 10", counter.to_string_lossy());
        let sink = VecSink::new();
        let loop_ = DevLoop::new(sh(&script), vec![dir.path().to_path_buf()])
            .with_sink(Arc::new(sink.clone()))
            .with_debounce(Duration::from_millis(50));

        // Give the first child a moment to start and record itself,
        // touch a .rs file to force a restart, wait for the second
        // launch, then fire the shutdown future.
        let target = dir.path().join("trigger.rs");
        let shutdown = async move {
            tokio::time::sleep(Duration::from_millis(600)).await;
            fs::write(&target, "fn main() {}").expect("write rs");
            tokio::time::sleep(Duration::from_millis(1500)).await;
        };
        let outcome = loop_.run_with_shutdown(shutdown).await.unwrap();
        assert!(outcome.interrupted, "loop should end via shutdown");
        assert!(
            outcome.restart_count >= 1,
            "expected at least one restart, got {} — sink: {:?}",
            outcome.restart_count,
            sink.snapshot().iter().map(|l| &l.text).collect::<Vec<_>>()
        );

        let launches = fs::read_to_string(&counter).unwrap_or_default();
        let launch_count = launches.lines().filter(|l| *l == "launched").count();
        assert!(
            launch_count >= 2,
            "expected at least 2 launches in {:?}, got {launch_count}",
            counter
        );
    }

    #[tokio::test]
    async fn non_rust_change_does_not_respawn() {
        let dir = tempdir().expect("tempdir");
        let counter = dir.path().join("count.txt");
        let script = format!("echo launched >> {}; sleep 10", counter.to_string_lossy());
        let sink = VecSink::new();
        let loop_ = DevLoop::new(sh(&script), vec![dir.path().to_path_buf()])
            .with_sink(Arc::new(sink.clone()))
            .with_debounce(Duration::from_millis(50));

        let target = dir.path().join("README.md");
        let shutdown = async move {
            tokio::time::sleep(Duration::from_millis(600)).await;
            fs::write(&target, "noise").expect("write md");
            tokio::time::sleep(Duration::from_millis(900)).await;
        };
        let outcome = loop_.run_with_shutdown(shutdown).await.unwrap();
        assert!(outcome.interrupted);
        assert_eq!(
            outcome.restart_count, 0,
            ".md changes must not trigger a respawn"
        );
    }

    #[test]
    fn filter_batch_keeps_only_configured_extensions() {
        let paths = vec![
            PathBuf::from("/ws/src/lib.rs"),
            PathBuf::from("/ws/README.md"),
            PathBuf::from("/ws/ui/app.slint"),
        ];
        let batch = WatchBatch { paths };
        let filtered = filter_batch(&batch, &["rs".to_string()]);
        assert_eq!(filtered, vec![PathBuf::from("/ws/src/lib.rs")]);
    }

    #[test]
    fn filter_batch_with_empty_extensions_passes_everything() {
        let paths = vec![
            PathBuf::from("/ws/src/lib.rs"),
            PathBuf::from("/ws/README.md"),
        ];
        let batch = WatchBatch {
            paths: paths.clone(),
        };
        assert_eq!(filter_batch(&batch, &[]), paths);
    }
}
