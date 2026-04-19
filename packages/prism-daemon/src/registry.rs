//! Command registry — the transport-agnostic IPC surface of the daemon.
//!
//! Every capability the daemon exposes (CRDT writes, Luau exec, build steps,
//! filesystem watchers, …) is registered here as a named JSON-in / JSON-out
//! handler. Transport adapters (local IPC, mobile FFI, HTTP, stdio CLI, …)
//! are all thin wrappers over [`CommandRegistry::invoke`], which is how the
//! same kernel ends up running on every device.
//!
//! This mirrors Studio's `LensRegistry` + `PluginRegistry` pattern: modules
//! self-register their contributions, the kernel owns the registry, and no
//! module needs to know how it will ultimately be reached from the outside.
//!
//! Each registered command also carries a minimum [`Permission`]. The
//! plain [`CommandRegistry::register`] stores `Permission::Dev` (the strict
//! default); a module that wants an end-user-reachable command uses
//! [`CommandRegistry::register_with_permission`] (or the
//! [`CommandRegistry::register_user`] shorthand). [`CommandRegistry::invoke`]
//! keeps its historical "caller is trusted" semantics and always runs;
//! [`CommandRegistry::invoke_with_permission`] is the one transport adapters
//! funnel through when they need to gate access by caller tier.

use crate::permission::Permission;
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

    #[error("permission denied: command '{command}' requires '{required}', caller has '{caller}'")]
    PermissionDenied {
        command: String,
        required: &'static str,
        caller: &'static str,
    },
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
///
/// Permissions are tracked in a parallel map so that [`Self::register`] can
/// retain its historical single-arg signature (every existing caller stays
/// source-compatible) while [`Self::register_with_permission`] opts into the
/// new gate. Missing entries default to [`Permission::Dev`] on lookup — the
/// strictest tier, safest if a handler ever sneaks in without an explicit
/// minimum.
#[derive(Default)]
pub struct CommandRegistry {
    handlers: RwLock<HashMap<String, CommandHandler>>,
    permissions: RwLock<HashMap<String, Permission>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler at the default `Permission::Dev` tier. Returns
    /// an error if the name is already taken — modules should pick unique
    /// dotted names to avoid collisions.
    pub fn register<F>(&self, name: impl Into<String>, handler: F) -> Result<(), CommandError>
    where
        F: Fn(JsonValue) -> Result<JsonValue, CommandError> + Send + Sync + 'static,
    {
        self.register_with_permission(name, Permission::Dev, handler)
    }

    /// Register a handler with an explicit minimum caller permission.
    /// Use [`Permission::User`] for read-only introspection commands that
    /// are safe to reach from a published end-user build; use
    /// [`Permission::Dev`] (or the plain [`Self::register`]) for anything
    /// that mutates state, spawns processes, or touches sensitive data.
    pub fn register_with_permission<F>(
        &self,
        name: impl Into<String>,
        min: Permission,
        handler: F,
    ) -> Result<(), CommandError>
    where
        F: Fn(JsonValue) -> Result<JsonValue, CommandError> + Send + Sync + 'static,
    {
        let name = name.into();
        {
            let mut map = self
                .handlers
                .write()
                .map_err(|_| CommandError::LockPoisoned)?;
            if map.contains_key(&name) {
                return Err(CommandError::AlreadyRegistered { command: name });
            }
            map.insert(name.clone(), Arc::new(handler));
        }
        let mut perms = self
            .permissions
            .write()
            .map_err(|_| CommandError::LockPoisoned)?;
        perms.insert(name, min);
        Ok(())
    }

    /// Shorthand for `register_with_permission(name, Permission::User, ...)`.
    pub fn register_user<F>(&self, name: impl Into<String>, handler: F) -> Result<(), CommandError>
    where
        F: Fn(JsonValue) -> Result<JsonValue, CommandError> + Send + Sync + 'static,
    {
        self.register_with_permission(name, Permission::User, handler)
    }

    /// Unregister a handler (used on module uninstall / kernel dispose).
    pub fn unregister(&self, name: &str) -> Result<(), CommandError> {
        {
            let mut map = self
                .handlers
                .write()
                .map_err(|_| CommandError::LockPoisoned)?;
            map.remove(name);
        }
        if let Ok(mut perms) = self.permissions.write() {
            perms.remove(name);
        }
        Ok(())
    }

    /// Invoke a handler by name. **No permission check** — use this from
    /// trusted embedders (tests, direct in-process callers) where the
    /// caller tier is implicit. Transport adapters should prefer
    /// [`Self::invoke_with_permission`] so a compromised UI or a published
    /// build can't reach past its declared tier.
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

    /// Invoke a handler by name, gated by the caller's permission tier.
    /// Returns [`CommandError::PermissionDenied`] when the caller doesn't
    /// hold at least the registered minimum. Missing entries default to
    /// [`Permission::Dev`] — so an unregistered command (or one registered
    /// before the permission system existed) is treated as dev-only.
    pub fn invoke_with_permission(
        &self,
        name: &str,
        payload: JsonValue,
        caller: Permission,
    ) -> Result<JsonValue, CommandError> {
        let (handler, required) = {
            let map = self
                .handlers
                .read()
                .map_err(|_| CommandError::LockPoisoned)?;
            let handler = map
                .get(name)
                .cloned()
                .ok_or_else(|| CommandError::NotFound(name.to_string()))?;
            let perms = self
                .permissions
                .read()
                .map_err(|_| CommandError::LockPoisoned)?;
            let required = perms.get(name).copied().unwrap_or(Permission::Dev);
            (handler, required)
        };
        if !caller.at_least(required) {
            return Err(CommandError::PermissionDenied {
                command: name.to_string(),
                required: required.as_str(),
                caller: caller.as_str(),
            });
        }
        handler(payload)
    }

    /// The minimum permission required to invoke `name`, or `None` when
    /// the command isn't registered. Missing permission entries fall back
    /// to [`Permission::Dev`] (the strict default).
    pub fn permission_of(&self, name: &str) -> Option<Permission> {
        let map = self.handlers.read().ok()?;
        if !map.contains_key(name) {
            return None;
        }
        let perms = self.permissions.read().ok()?;
        Some(perms.get(name).copied().unwrap_or(Permission::Dev))
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

    // ── Permission gate ─────────────────────────────────────────────────

    #[test]
    fn register_defaults_to_dev_permission() {
        let reg = CommandRegistry::new();
        reg.register("legacy.ping", |_| Ok(json!("pong"))).unwrap();
        assert_eq!(reg.permission_of("legacy.ping"), Some(Permission::Dev));
    }

    #[test]
    fn register_user_shorthand_tags_command_as_user() {
        let reg = CommandRegistry::new();
        reg.register_user("intro.ping", |_| Ok(json!("pong")))
            .unwrap();
        assert_eq!(reg.permission_of("intro.ping"), Some(Permission::User));
    }

    #[test]
    fn invoke_with_permission_allows_user_tier_for_user_command() {
        let reg = CommandRegistry::new();
        reg.register_user("intro.ping", |_| Ok(json!("pong")))
            .unwrap();
        let out = reg
            .invoke_with_permission("intro.ping", JsonValue::Null, Permission::User)
            .unwrap();
        assert_eq!(out, json!("pong"));
    }

    #[test]
    fn invoke_with_permission_allows_dev_tier_for_user_command() {
        let reg = CommandRegistry::new();
        reg.register_user("intro.ping", |_| Ok(json!("pong")))
            .unwrap();
        let out = reg
            .invoke_with_permission("intro.ping", JsonValue::Null, Permission::Dev)
            .unwrap();
        assert_eq!(out, json!("pong"));
    }

    #[test]
    fn invoke_with_permission_rejects_user_tier_on_dev_command() {
        let reg = CommandRegistry::new();
        reg.register("crdt.write", |_| Ok(JsonValue::Null)).unwrap();
        let err = reg
            .invoke_with_permission("crdt.write", JsonValue::Null, Permission::User)
            .unwrap_err();
        match err {
            CommandError::PermissionDenied {
                command,
                required,
                caller,
            } => {
                assert_eq!(command, "crdt.write");
                assert_eq!(required, "dev");
                assert_eq!(caller, "user");
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn invoke_with_permission_still_returns_not_found_for_unknown() {
        let reg = CommandRegistry::new();
        let err = reg
            .invoke_with_permission("nope", JsonValue::Null, Permission::Dev)
            .unwrap_err();
        assert!(matches!(err, CommandError::NotFound(_)));
    }

    #[test]
    fn permission_of_returns_none_for_unknown() {
        let reg = CommandRegistry::new();
        assert!(reg.permission_of("missing").is_none());
    }

    #[test]
    fn unregister_clears_permission_entry() {
        let reg = CommandRegistry::new();
        reg.register_user("intro.ping", |_| Ok(json!("pong")))
            .unwrap();
        reg.unregister("intro.ping").unwrap();
        assert!(reg.permission_of("intro.ping").is_none());
    }
}
