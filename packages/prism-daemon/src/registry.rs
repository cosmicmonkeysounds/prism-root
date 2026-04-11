//! Command registry — the transport-agnostic IPC surface of the daemon.
//!
//! Every capability the daemon exposes (CRDT writes, Lua exec, build steps,
//! filesystem watchers, …) is registered here as a named JSON-in / JSON-out
//! handler. Transport adapters (Tauri IPC, mobile FFI, HTTP, stdio CLI, …)
//! are all thin wrappers over [`CommandRegistry::invoke`], which is how the
//! same kernel ends up running on every device.
//!
//! This mirrors Studio's `LensRegistry` + `PluginRegistry` pattern: modules
//! self-register their contributions, the kernel owns the registry, and no
//! module needs to know how it will ultimately be reached from the outside.

use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Error returned from a command handler or from the registry itself.
#[derive(Debug, Clone, thiserror::Error)]
pub enum CommandError {
    #[error("command not found: {0}")]
    NotFound(String),

    #[error("command '{command}' already registered")]
    AlreadyRegistered { command: String },

    #[error("command '{command}' failed: {message}")]
    Handler { command: String, message: String },

    #[error("registry lock poisoned")]
    LockPoisoned,
}

impl CommandError {
    /// Helper for modules that want to surface a handler error without
    /// having to name the command twice.
    pub fn handler(command: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Handler {
            command: command.into(),
            message: message.into(),
        }
    }
}

/// Boxed handler signature. Handlers own their captured state (Arc'd
/// services from the kernel), take a JSON payload and return a JSON result.
pub type CommandHandler = Arc<dyn Fn(JsonValue) -> Result<JsonValue, CommandError> + Send + Sync>;

/// Registry of command handlers keyed by fully-qualified name
/// (e.g. `crdt.write`, `luau.exec`, `build.run_step`).
#[derive(Default)]
pub struct CommandRegistry {
    handlers: RwLock<HashMap<String, CommandHandler>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler. Returns an error if the name is already taken —
    /// modules should pick unique dotted names to avoid collisions.
    pub fn register<F>(&self, name: impl Into<String>, handler: F) -> Result<(), CommandError>
    where
        F: Fn(JsonValue) -> Result<JsonValue, CommandError> + Send + Sync + 'static,
    {
        let name = name.into();
        let mut map = self
            .handlers
            .write()
            .map_err(|_| CommandError::LockPoisoned)?;
        if map.contains_key(&name) {
            return Err(CommandError::AlreadyRegistered { command: name });
        }
        map.insert(name, Arc::new(handler));
        Ok(())
    }

    /// Unregister a handler (used on module uninstall / kernel dispose).
    pub fn unregister(&self, name: &str) -> Result<(), CommandError> {
        let mut map = self
            .handlers
            .write()
            .map_err(|_| CommandError::LockPoisoned)?;
        map.remove(name);
        Ok(())
    }

    /// Invoke a handler by name. This is the single entry point every
    /// transport adapter funnels through.
    pub fn invoke(&self, name: &str, payload: JsonValue) -> Result<JsonValue, CommandError> {
        let handler = {
            let map = self
                .handlers
                .read()
                .map_err(|_| CommandError::LockPoisoned)?;
            map.get(name)
                .cloned()
                .ok_or_else(|| CommandError::NotFound(name.to_string()))?
        };
        handler(payload)
    }

    /// List every registered command name (sorted). Handy for `--help`
    /// output from the standalone CLI or a mobile "capabilities" query.
    pub fn list(&self) -> Vec<String> {
        let map = match self.handlers.read() {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        let mut names: Vec<String> = map.keys().cloned().collect();
        names.sort();
        names
    }

    /// Number of registered handlers.
    pub fn len(&self) -> usize {
        self.handlers.read().map(|m| m.len()).unwrap_or(0)
    }

    /// True when no handlers are registered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// True when the given command is registered.
    pub fn has(&self, name: &str) -> bool {
        self.handlers
            .read()
            .map(|m| m.contains_key(name))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn register_and_invoke_roundtrips_json() {
        let reg = CommandRegistry::new();
        reg.register("echo", Ok).unwrap();

        let out = reg.invoke("echo", json!({"a": 1})).unwrap();
        assert_eq!(out, json!({"a": 1}));
    }

    #[test]
    fn invoke_unknown_command_returns_not_found() {
        let reg = CommandRegistry::new();
        let err = reg.invoke("nope", JsonValue::Null).unwrap_err();
        assert!(matches!(err, CommandError::NotFound(ref n) if n == "nope"));
    }

    #[test]
    fn double_register_is_rejected() {
        let reg = CommandRegistry::new();
        reg.register("dup", |_| Ok(JsonValue::Null)).unwrap();
        let err = reg.register("dup", |_| Ok(JsonValue::Null)).unwrap_err();
        assert!(matches!(err, CommandError::AlreadyRegistered { .. }));
    }

    #[test]
    fn unregister_removes_handler() {
        let reg = CommandRegistry::new();
        reg.register("gone", |_| Ok(JsonValue::Null)).unwrap();
        assert!(reg.has("gone"));
        reg.unregister("gone").unwrap();
        assert!(!reg.has("gone"));
    }

    #[test]
    fn list_is_sorted() {
        let reg = CommandRegistry::new();
        reg.register("c.z", |_| Ok(JsonValue::Null)).unwrap();
        reg.register("a.x", |_| Ok(JsonValue::Null)).unwrap();
        reg.register("b.y", |_| Ok(JsonValue::Null)).unwrap();
        assert_eq!(reg.list(), vec!["a.x", "b.y", "c.z"]);
    }

    #[test]
    fn handler_can_close_over_shared_state() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let reg = CommandRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        reg.register("bump", move |_| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(JsonValue::from(c.load(Ordering::SeqCst)))
        })
        .unwrap();

        assert_eq!(
            reg.invoke("bump", JsonValue::Null).unwrap(),
            JsonValue::from(1)
        );
        assert_eq!(
            reg.invoke("bump", JsonValue::Null).unwrap(),
            JsonValue::from(2)
        );
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn handler_error_propagates_as_command_error() {
        let reg = CommandRegistry::new();
        reg.register("boom", |_| Err(CommandError::handler("boom", "kaboom")))
            .unwrap();
        let err = reg.invoke("boom", JsonValue::Null).unwrap_err();
        match err {
            CommandError::Handler { command, message } => {
                assert_eq!(command, "boom");
                assert_eq!(message, "kaboom");
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }
}
