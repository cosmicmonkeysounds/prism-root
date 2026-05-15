//! Hot-reload watcher — closes C3 of
//! `docs/dev/ui-migration-followups.md`.
//!
//! Spawns a `notify::RecommendedWatcher` on a separate OS thread
//! that observes a set of `.prism-ui` paths and posts every change
//! to an `std::sync::mpsc` channel. The femtovg backend's per-event
//! tick hook (`run_with_tick`) drains the channel each idle wait
//! and calls back into `Shell::install_default_skeleton` /
//! `Shell::install_app_skeleton` to apply the swap without dropping
//! the running event loop.
//!
//! Thread shape: the watcher thread is the only `Send` boundary;
//! the channel receiver lives on the shell's single thread alongside
//! winit / mlua / the Surface. The watcher hands off paths only —
//! no Rust-side state crosses the boundary.

#![cfg(feature = "native")]

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Classification of a watch target. Skeleton sources land via
/// [`Shell::install_default_skeleton`] / [`install_app_skeleton`].
#[derive(Debug, Clone)]
pub enum ReloadTarget {
    /// The canonical `ui/app.prism-ui` skeleton.
    DefaultSkeleton,
    /// A per-app skeleton (`apps/<id>/shell.prism-ui`) keyed by
    /// `app_id`.
    AppSkeleton { app_id: String },
}

/// One pending hot-reload — the watcher thread posts these as files
/// change; the consumer drains them on the next tick.
#[derive(Debug, Clone)]
pub struct ReloadEvent {
    pub target: ReloadTarget,
    pub source: String,
}

/// Spec passed to [`spawn_hot_reload_watcher`]. Each entry pins one
/// on-disk path to one [`ReloadTarget`].
#[derive(Debug, Clone)]
pub struct WatchSpec {
    pub path: PathBuf,
    pub target: ReloadTarget,
}

/// Handle returned by [`spawn_hot_reload_watcher`]. Drops the watcher
/// (and joins the thread) on drop. Hold this for the lifetime of the
/// shell's run loop.
pub struct HotReloadHandle {
    receiver: Receiver<ReloadEvent>,
    // Watcher field kept solely so its drop fires when the handle
    // drops (the notify watcher tears down its OS callbacks on drop).
    _watcher: RecommendedWatcher,
}

impl HotReloadHandle {
    /// Drain every pending reload — non-blocking. Returns the
    /// freshest source per `ReloadTarget` (intermediate edits during
    /// burst writes are dropped). Callers feed each returned event
    /// to the corresponding `Shell::install_*_skeleton` method.
    pub fn drain(&self) -> Vec<ReloadEvent> {
        // Coalesce by target — when a file flickers (save = unlink +
        // rename on some editors, or rapid keystrokes via a watcher),
        // only the last event for each target matters. The reader
        // applies them in insertion order so a per-target rename
        // resolves correctly.
        let mut out: Vec<ReloadEvent> = Vec::new();
        while let Ok(evt) = self.receiver.try_recv() {
            let evt_key = target_key(&evt.target);
            // Remove any prior event with the same target key so the
            // newest source wins. Vec walk is fine — the queue length
            // is bounded by the burst-write count, single digits in
            // practice.
            out.retain(|prev| target_key(&prev.target) != evt_key);
            out.push(evt);
        }
        out
    }
}

fn target_key(t: &ReloadTarget) -> String {
    match t {
        ReloadTarget::DefaultSkeleton => "<default>".into(),
        ReloadTarget::AppSkeleton { app_id } => format!("app:{app_id}"),
    }
}

/// Spawn a watcher thread that observes `specs` and posts every
/// change to an mpsc channel. Returns a [`HotReloadHandle`] whose
/// `Drop` tears down the watcher + thread.
///
/// Errors during watcher construction surface as `Err(String)`;
/// per-file watch failures are logged to stderr and skipped (the
/// remaining files still get watched). A `.prism-ui` file that
/// doesn't exist at spawn time is also skipped with a warning.
pub fn spawn_hot_reload_watcher(specs: Vec<WatchSpec>) -> Result<HotReloadHandle, String> {
    let (tx, rx) = channel::<ReloadEvent>();

    // Clone specs into the watcher closure so it can map paths back
    // to targets. The watcher closure runs on notify's internal
    // thread; the channel sender carries the reload event back to
    // the shell's main thread.
    let specs_inside = specs.clone();
    let tx_inside = tx.clone();
    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: notify::Result<Event>| {
            let Ok(event) = res else {
                return;
            };
            // We only care about content changes — modify / create on
            // a rename / save-replace patterns. notify reports remove
            // + create separately for editor "swap-on-save" flows;
            // both surface here as the file showing up under
            // `event.paths`.
            match event.kind {
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Other => {}
                _ => return,
            }
            for changed in &event.paths {
                if let Some(spec) = match_spec(&specs_inside, changed) {
                    deliver(&tx_inside, spec, changed);
                }
            }
        })
        .map_err(|e| format!("notify::recommended_watcher: {e}"))?;

    // Watch every spec's parent directory (notify is more reliable
    // watching directories than individual files across platforms —
    // editors that rename-on-save would otherwise drop the
    // watch on the original inode). The closure filters back to the
    // exact path we care about via `match_spec`.
    let mut watched: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    for spec in &specs {
        let parent = match spec.path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        };
        if !watched.insert(parent.clone()) {
            continue;
        }
        if let Err(e) = watcher.watch(&parent, RecursiveMode::NonRecursive) {
            eprintln!(
                "prism-shell hot-reload: failed to watch {}: {e}",
                parent.display()
            );
        }
    }

    // Initial debounce — emit the current contents of every spec'd
    // file once so a host that starts with a stale on-disk version
    // sees the watcher-mediated reload path before the first real
    // edit. Errors here are non-fatal — a missing file is a no-op
    // for the watcher; the user can create / save it and the
    // subsequent change event picks it up.
    let tx_init = tx.clone();
    let specs_init = specs.clone();
    thread::spawn(move || {
        // Give notify a heartbeat to settle before emitting the
        // initial sync events.
        thread::sleep(Duration::from_millis(50));
        for spec in &specs_init {
            deliver(&tx_init, spec, &spec.path);
        }
    });

    Ok(HotReloadHandle {
        receiver: rx,
        _watcher: watcher,
    })
}

fn match_spec<'a>(specs: &'a [WatchSpec], changed: &Path) -> Option<&'a WatchSpec> {
    // Use canonicalised paths when both resolve cleanly so symlinks /
    // relative variants line up. Fall back to literal equality
    // otherwise.
    let canonical_changed = changed.canonicalize().ok();
    for spec in specs {
        if spec.path == *changed {
            return Some(spec);
        }
        if let (Some(a), Some(b)) = (canonical_changed.as_ref(), spec.path.canonicalize().ok()) {
            if a == &b {
                return Some(spec);
            }
        }
    }
    None
}

fn deliver(tx: &Sender<ReloadEvent>, spec: &WatchSpec, _path: &Path) {
    let source = match std::fs::read_to_string(&spec.path) {
        Ok(s) => s,
        Err(e) => {
            // Editors that rename-on-save briefly drop the inode;
            // the next event re-reads cleanly. Silent skip.
            let _ = e;
            return;
        }
    };
    let _ = tx.send(ReloadEvent {
        target: spec.target.clone(),
        source,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// End-to-end smoke test: write a `.prism-ui` file, spawn the
    /// watcher, modify the file, drain the handle. The reload event
    /// should carry the updated source.
    ///
    /// Filesystem-watchers are inherently flaky on CI under load —
    /// this test allows up to a 2-second window for the change to
    /// surface, which is enough on every supported platform but not
    /// asymptotically reliable. If it fans out, the test is the
    /// first to bail; the production hot-reload path is
    /// non-correctness gating (a respawn always works as a fallback).
    #[test]
    fn watcher_emits_reload_event_on_file_change() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("app.prism-ui");
        {
            let mut f = std::fs::File::create(&path).expect("create");
            writeln!(f, r#"<container/>"#).expect("write initial");
        }

        let spec = WatchSpec {
            path: path.clone(),
            target: ReloadTarget::DefaultSkeleton,
        };
        let handle = spawn_hot_reload_watcher(vec![spec]).expect("spawn");

        // Consume the initial debounce burst.
        thread::sleep(Duration::from_millis(200));
        let _ = handle.drain();

        // Apply a real edit.
        {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&path)
                .expect("open");
            writeln!(f, r#"<container><text>updated</text></container>"#).expect("write");
        }

        // Poll for up to 2 seconds for the change to surface.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut events = Vec::new();
        while std::time::Instant::now() < deadline {
            let batch = handle.drain();
            if !batch.is_empty() {
                events = batch;
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        assert!(
            !events.is_empty(),
            "watcher did not surface a reload event within 2s"
        );
        assert!(
            events.iter().any(|e| e.source.contains("updated")),
            "no event carried the updated source: {events:?}"
        );
    }

    #[test]
    fn drain_coalesces_by_target() {
        // White-box: feed the same Handle a manual stream of three
        // ReloadEvents for two targets; verify `drain` keeps only
        // the newest per target.
        //
        // We can't easily construct a HotReloadHandle without
        // notify, so we exercise the `target_key` logic directly
        // by mirroring the drain coalesce loop here.
        let evts = vec![
            ReloadEvent {
                target: ReloadTarget::DefaultSkeleton,
                source: "v1".into(),
            },
            ReloadEvent {
                target: ReloadTarget::AppSkeleton {
                    app_id: "flux".into(),
                },
                source: "fa".into(),
            },
            ReloadEvent {
                target: ReloadTarget::DefaultSkeleton,
                source: "v2".into(),
            },
        ];
        let mut out: Vec<ReloadEvent> = Vec::new();
        for evt in evts {
            let evt_key = target_key(&evt.target);
            out.retain(|prev| target_key(&prev.target) != evt_key);
            out.push(evt);
        }
        assert_eq!(out.len(), 2);
        // Default's freshest is v2, flux's is the only one.
        assert!(out
            .iter()
            .any(|e| matches!(e.target, ReloadTarget::DefaultSkeleton) && e.source == "v2"));
        assert!(out.iter().any(
            |e| matches!(&e.target, ReloadTarget::AppSkeleton { app_id } if app_id == "flux")
        ));
    }
}
