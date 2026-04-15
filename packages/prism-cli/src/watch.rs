//! Notify-driven filesystem watcher scaffold.
//!
//! Phase 1 of the Slint migration plan (§11) calls for a
//! "notify-driven watch loop scaffold" — the piece later phases will
//! grow into `subsecond` hot-reload and the `slint-interpreter`-based
//! builder re-parse. This module is that scaffold. It is deliberately
//! small: wrap `notify::RecommendedWatcher`, debounce raw filesystem
//! events into batches, and hand the caller an iterator-style
//! `next_batch` API so the dev server can act on them.
//!
//! The scaffold is a pure library module — `prism dev` does not yet
//! wire a rebuild reaction because that belongs to Phase 2/3 once the
//! shell has an in-process reload path. Until then the watcher ships
//! with tests (a tempfile round-trip) so the module stays honest.
//!
//! Why roll our own debouncer instead of pulling in
//! `notify-debouncer-full`? The scaffold only needs millisecond-range
//! coalescing and we already pin a matching `notify` version at the
//! workspace level — one less crate in the dep graph keeps iteration
//! fast.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// A debounced batch of filesystem changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatchBatch {
    /// Sorted, deduplicated paths touched inside the debounce window.
    pub paths: Vec<PathBuf>,
}

impl WatchBatch {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

/// Scaffold file watcher. Wraps `notify::RecommendedWatcher` and
/// exposes a polling API that drains raw events into `WatchBatch`es.
///
/// Keep the struct `Send` + `Sync` hostile — the embedded `Receiver`
/// is single-consumer by construction, so the watcher is meant to be
/// owned by a single dev-server thread.
pub struct WatchLoop {
    _watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<Event>>,
    debounce: Duration,
}

impl WatchLoop {
    /// Default debounce window for the scaffold. Short enough to feel
    /// live, long enough to coalesce a save that touches multiple
    /// sibling files at once (common with formatters + LSP writers).
    pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(150);

    /// Build a recursive watcher rooted at `path`. The returned loop
    /// starts collecting events immediately; callers should drain it
    /// with [`WatchLoop::next_batch`] on their own cadence.
    pub fn new(path: impl AsRef<Path>) -> notify::Result<Self> {
        Self::with_debounce(path, Self::DEFAULT_DEBOUNCE)
    }

    /// Same as [`WatchLoop::new`], with a caller-supplied debounce
    /// window. Exposed for tests that need a tight window so the
    /// suite does not spend seconds sleeping.
    pub fn with_debounce(path: impl AsRef<Path>, debounce: Duration) -> notify::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                // Channel closure means the consumer went away —
                // swallow the error; the watcher thread will wind
                // down as its handle drops.
                let _ = tx.send(res);
            },
            Config::default(),
        )?;
        watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;
        Ok(Self {
            _watcher: watcher,
            rx,
            debounce,
        })
    }

    /// Block for up to `timeout` waiting for the first event, then
    /// drain any additional events that arrive inside `debounce`
    /// (starting from the first event). Returns `None` if no event
    /// was observed inside `timeout`.
    ///
    /// Filtered to `Create`/`Modify`/`Remove` — access-only events
    /// (e.g. `stat`) are dropped so the scaffold does not fire on
    /// pure reads.
    pub fn next_batch(&self, timeout: Duration) -> Option<WatchBatch> {
        let first = self.rx.recv_timeout(timeout).ok()?;
        let mut touched: BTreeSet<PathBuf> = BTreeSet::new();
        push_event(&mut touched, first);

        let deadline = Instant::now() + self.debounce;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match self.rx.recv_timeout(remaining) {
                Ok(next) => push_event(&mut touched, next),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        if touched.is_empty() {
            None
        } else {
            Some(WatchBatch {
                paths: touched.into_iter().collect(),
            })
        }
    }

    /// Non-blocking drain. Useful for integrating the watcher into a
    /// host loop that already runs its own select!/poll cycle.
    pub fn try_next_batch(&self) -> Option<WatchBatch> {
        let first = self.rx.try_recv().ok()?;
        let mut touched: BTreeSet<PathBuf> = BTreeSet::new();
        push_event(&mut touched, first);
        while let Ok(next) = self.rx.try_recv() {
            push_event(&mut touched, next);
        }
        if touched.is_empty() {
            None
        } else {
            Some(WatchBatch {
                paths: touched.into_iter().collect(),
            })
        }
    }
}

fn push_event(touched: &mut BTreeSet<PathBuf>, event: notify::Result<Event>) {
    let Ok(event) = event else { return };
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
            for p in event.paths {
                touched.insert(p);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn batch_reports_written_file() {
        let dir = tempdir().expect("tempdir");
        let loop_ =
            WatchLoop::with_debounce(dir.path(), Duration::from_millis(75)).expect("watcher");

        // Give notify time to arm its backend before we touch the fs.
        thread::sleep(Duration::from_millis(50));
        let target = dir.path().join("hello.txt");
        fs::write(&target, "hi").expect("write");

        let batch = loop_
            .next_batch(Duration::from_secs(3))
            .expect("batch observed");
        let canon_target = target.canonicalize().unwrap_or(target.clone());
        let saw_target = batch.paths.iter().any(|p| {
            let canon_p = p.canonicalize().unwrap_or(p.clone());
            canon_p == canon_target || p == &target
        });
        assert!(
            saw_target,
            "expected batch to contain {target:?}, got {:?}",
            batch.paths
        );
    }

    #[test]
    fn try_next_batch_is_none_when_idle() {
        let dir = tempdir().expect("tempdir");
        let loop_ = WatchLoop::new(dir.path()).expect("watcher");
        assert!(loop_.try_next_batch().is_none());
    }

    #[test]
    fn next_batch_times_out_on_quiet_dir() {
        let dir = tempdir().expect("tempdir");
        let loop_ = WatchLoop::new(dir.path()).expect("watcher");
        // Some backends (notably macOS FSEvents) replay a priming
        // event as the watcher attaches — drain anything the kernel
        // had queued before asserting quiet.
        thread::sleep(Duration::from_millis(100));
        while loop_.try_next_batch().is_some() {}

        let start = Instant::now();
        assert!(loop_.next_batch(Duration::from_millis(120)).is_none());
        // Sanity: we actually waited ≈ the timeout.
        assert!(start.elapsed() >= Duration::from_millis(100));
    }
}
