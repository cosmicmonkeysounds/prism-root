//! File system watcher using the `notify` crate.
//!
//! Watches directories for changes and emits events that can be
//! forwarded to the frontend via Tauri IPC events. When a watched
//! file changes, the daemon can update the corresponding Loro doc.

use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// Represents a file system change event for the frontend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileChangeEvent {
    /// The type of change: "create", "modify", "remove".
    pub kind: String,
    /// Absolute paths of affected files.
    pub paths: Vec<PathBuf>,
}

impl FileChangeEvent {
    fn from_notify_event(event: &Event) -> Option<Self> {
        let kind = match event.kind {
            EventKind::Create(_) => "create",
            EventKind::Modify(_) => "modify",
            EventKind::Remove(_) => "remove",
            _ => return None,
        };

        Some(FileChangeEvent {
            kind: kind.to_string(),
            paths: event.paths.clone(),
        })
    }
}

/// A file watcher that monitors a directory and sends change events.
pub struct FileWatcherHandle {
    _watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<NotifyResult<Event>>,
}

/// Start watching a directory for file changes.
///
/// Returns a handle with a receiver channel for consuming events.
/// The watcher runs in a background thread and survives until the
/// handle is dropped.
pub fn watch_directory(path: &Path) -> Result<FileWatcherHandle, String> {
    let (tx, rx) = mpsc::channel();

    let mut watcher = RecommendedWatcher::new(tx, Config::default())
        .map_err(|e| format!("Failed to create watcher: {e}"))?;

    watcher
        .watch(path, RecursiveMode::Recursive)
        .map_err(|e| format!("Failed to watch path {}: {e}", path.display()))?;

    Ok(FileWatcherHandle {
        _watcher: watcher,
        receiver: rx,
    })
}

impl FileWatcherHandle {
    /// Poll for pending file change events (non-blocking).
    /// Returns all events that have accumulated since the last poll.
    pub fn poll_events(&self) -> Vec<FileChangeEvent> {
        let mut events = Vec::new();
        while let Ok(result) = self.receiver.try_recv() {
            if let Ok(event) = result {
                if let Some(change) = FileChangeEvent::from_notify_event(&event) {
                    events.push(change);
                }
            }
        }
        events
    }

    /// Wait for the next file change event (blocking, with timeout).
    pub fn wait_event(&self, timeout: Duration) -> Option<FileChangeEvent> {
        self.receiver
            .recv_timeout(timeout)
            .ok()
            .and_then(|r| r.ok())
            .and_then(|e| FileChangeEvent::from_notify_event(&e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;

    #[test]
    fn test_watch_directory_detects_file_create() {
        let dir = tempfile::tempdir().unwrap();
        let handle = watch_directory(dir.path()).unwrap();

        // Create a file
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello").unwrap();

        // Give the watcher time to detect
        thread::sleep(Duration::from_millis(200));

        let events = handle.poll_events();
        assert!(
            !events.is_empty(),
            "Expected at least one event after file creation"
        );

        let has_create_or_modify = events.iter().any(|e| e.kind == "create" || e.kind == "modify");
        assert!(
            has_create_or_modify,
            "Expected a create or modify event, got: {:?}",
            events.iter().map(|e| &e.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_watch_directory_detects_file_modify() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("existing.txt");
        fs::write(&file_path, "initial").unwrap();

        let handle = watch_directory(dir.path()).unwrap();

        // Modify the file
        thread::sleep(Duration::from_millis(100));
        fs::write(&file_path, "modified").unwrap();

        thread::sleep(Duration::from_millis(200));

        let events = handle.poll_events();
        assert!(
            !events.is_empty(),
            "Expected at least one event after file modification"
        );
    }

    #[test]
    fn test_watch_directory_detects_file_remove() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("to_delete.txt");
        fs::write(&file_path, "delete me").unwrap();

        let handle = watch_directory(dir.path()).unwrap();

        thread::sleep(Duration::from_millis(100));
        fs::remove_file(&file_path).unwrap();

        thread::sleep(Duration::from_millis(200));

        let events = handle.poll_events();
        assert!(
            !events.is_empty(),
            "Expected at least one event after file deletion"
        );
    }
}
