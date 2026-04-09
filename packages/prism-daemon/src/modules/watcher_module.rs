//! Filesystem watcher module — wraps the `notify` crate behind three
//! commands:
//!
//! | Command          | Payload                | Result                   |
//! |------------------|------------------------|--------------------------|
//! | `watcher.watch`  | `{ path }`             | `{ id }`                 |
//! | `watcher.poll`   | `{ id }`               | `{ events: [...] }`      |
//! | `watcher.stop`   | `{ id }`               | `null`                   |
//!
//! Watcher state is shared across handlers via [`WatcherManager`], which
//! the kernel also exposes directly for hot paths.

use crate::builder::DaemonBuilder;
use crate::module::DaemonModule;
use crate::registry::CommandError;
use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// A file system change event returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeEvent {
    /// "create" | "modify" | "remove"
    pub kind: String,
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

/// One active watcher.
pub struct FileWatcherHandle {
    _watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<NotifyResult<Event>>,
}

impl FileWatcherHandle {
    /// Poll for pending events (non-blocking).
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

    /// Wait for the next event, with a timeout.
    pub fn wait_event(&self, timeout: Duration) -> Option<FileChangeEvent> {
        self.receiver
            .recv_timeout(timeout)
            .ok()
            .and_then(|r| r.ok())
            .and_then(|e| FileChangeEvent::from_notify_event(&e))
    }
}

/// Start watching a directory and return a handle.
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

/// Multi-watcher registry exposed on the kernel.
pub struct WatcherManager {
    next_id: Mutex<u64>,
    watchers: Mutex<HashMap<u64, FileWatcherHandle>>,
}

impl WatcherManager {
    pub fn new() -> Self {
        Self {
            next_id: Mutex::new(1),
            watchers: Mutex::new(HashMap::new()),
        }
    }

    pub fn watch(&self, path: &Path) -> Result<u64, String> {
        let handle = watch_directory(path)?;
        let mut next = self.next_id.lock().map_err(|_| "lock poisoned")?;
        let id = *next;
        *next += 1;
        drop(next);
        self.watchers
            .lock()
            .map_err(|_| "lock poisoned")?
            .insert(id, handle);
        Ok(id)
    }

    pub fn poll(&self, id: u64) -> Result<Vec<FileChangeEvent>, String> {
        let map = self.watchers.lock().map_err(|_| "lock poisoned")?;
        let handle = map
            .get(&id)
            .ok_or_else(|| format!("unknown watcher id: {id}"))?;
        Ok(handle.poll_events())
    }

    pub fn stop(&self, id: u64) -> Result<(), String> {
        let mut map = self.watchers.lock().map_err(|_| "lock poisoned")?;
        map.remove(&id);
        Ok(())
    }
}

impl Default for WatcherManager {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WatcherModule;

impl DaemonModule for WatcherModule {
    fn id(&self) -> &str {
        "prism.watcher"
    }

    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        let mgr = builder
            .watcher_manager_slot()
            .get_or_insert_with(|| Arc::new(WatcherManager::new()))
            .clone();
        let registry = builder.registry().clone();

        let m = mgr.clone();
        registry.register("watcher.watch", move |payload| {
            let args: WatchArgs = serde_json::from_value(payload)
                .map_err(|e| CommandError::handler("watcher.watch", e.to_string()))?;
            let id = m
                .watch(Path::new(&args.path))
                .map_err(|e| CommandError::handler("watcher.watch", e))?;
            Ok(json!({ "id": id }))
        })?;

        let m = mgr.clone();
        registry.register("watcher.poll", move |payload| {
            let args: IdArgs = serde_json::from_value(payload)
                .map_err(|e| CommandError::handler("watcher.poll", e.to_string()))?;
            let events = m
                .poll(args.id)
                .map_err(|e| CommandError::handler("watcher.poll", e))?;
            Ok(json!({ "events": events }))
        })?;

        let m = mgr;
        registry.register("watcher.stop", move |payload| {
            let args: IdArgs = serde_json::from_value(payload)
                .map_err(|e| CommandError::handler("watcher.stop", e.to_string()))?;
            m.stop(args.id)
                .map_err(|e| CommandError::handler("watcher.stop", e))?;
            Ok(JsonValue::Null)
        })?;

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct WatchArgs {
    path: String,
}

#[derive(Debug, Deserialize)]
struct IdArgs {
    id: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;
    use std::fs;
    use std::thread;

    #[test]
    fn watcher_module_registers_three_commands() {
        let kernel = DaemonBuilder::new().with_watcher().build().unwrap();
        let caps = kernel.capabilities();
        assert!(caps.contains(&"watcher.watch".to_string()));
        assert!(caps.contains(&"watcher.poll".to_string()));
        assert!(caps.contains(&"watcher.stop".to_string()));
    }

    #[test]
    fn test_watch_directory_detects_file_create() {
        let dir = tempfile::tempdir().unwrap();
        let handle = watch_directory(dir.path()).unwrap();

        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello").unwrap();

        thread::sleep(Duration::from_millis(200));

        let events = handle.poll_events();
        assert!(
            !events.is_empty(),
            "Expected at least one event after file creation"
        );

        let has_create_or_modify = events
            .iter()
            .any(|e| e.kind == "create" || e.kind == "modify");
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

        thread::sleep(Duration::from_millis(100));
        fs::write(&file_path, "modified").unwrap();

        thread::sleep(Duration::from_millis(200));

        let events = handle.poll_events();
        assert!(!events.is_empty());
    }

    #[test]
    fn watcher_manager_issues_sequential_ids_and_allows_stop() {
        let mgr = WatcherManager::new();
        let dir = tempfile::tempdir().unwrap();
        let id_a = mgr.watch(dir.path()).unwrap();
        let id_b = mgr.watch(dir.path()).unwrap();
        assert_eq!(id_a, 1);
        assert_eq!(id_b, 2);

        mgr.stop(id_a).unwrap();
        assert!(mgr.poll(id_a).is_err());
        assert!(mgr.poll(id_b).is_ok());
    }

    #[test]
    fn kernel_invoke_drives_the_whole_watch_cycle() {
        let kernel = DaemonBuilder::new().with_watcher().build().unwrap();
        let dir = tempfile::tempdir().unwrap();

        let start = kernel
            .invoke(
                "watcher.watch",
                json!({ "path": dir.path().to_string_lossy() }),
            )
            .unwrap();
        let id = start["id"].as_u64().unwrap();

        fs::write(dir.path().join("poke.txt"), "hi").unwrap();
        thread::sleep(Duration::from_millis(200));

        let polled = kernel.invoke("watcher.poll", json!({ "id": id })).unwrap();
        let events = polled["events"].as_array().unwrap();
        assert!(!events.is_empty());

        kernel.invoke("watcher.stop", json!({ "id": id })).unwrap();
    }
}
