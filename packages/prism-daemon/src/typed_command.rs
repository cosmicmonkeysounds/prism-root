//! Typed-handler ergonomics for [`CommandRegistry`].
//!
//! Every existing daemon module hand-rolls the same three-line glue for
//! every command: `serde_json::from_value` on the payload, dispatch to a
//! typed function, `serde_json::to_value` on the result, map every JSON
//! error into [`CommandError`]. This module is the single shared layer
//! that eats that boilerplate so module authors write *only* the typed
//! handler.
//!
//! ```ignore
//! use prism_daemon::typed_command::CommandRegistryExt;
//!
//! #[derive(serde::Deserialize)] struct FooReq { name: String }
//! #[derive(serde::Serialize)]   struct FooResp { greeting: String }
//!
//! builder.registry().register_typed("foo.greet", |req: FooReq| {
//!     Ok::<_, std::convert::Infallible>(FooResp {
//!         greeting: format!("hi {}", req.name),
//!     })
//! })?;
//! ```
//!
//! `#[daemon_command]` (in `prism-luau-derive`) is the preferred
//! authoring surface — it desugars to `register_typed_with_permission`
//! against this same trait, so the wire behaviour is identical. Reach
//! for the trait directly only when you need to register a closure
//! that captures non-state data, or in tests.
//!
//! See `docs/dev/declarative-refactorings.md` for the broader context.

use crate::permission::Permission;
use crate::registry::{CommandError, CommandRegistry};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt::Display;

/// Shared no-payload request marker for typed handlers that take no
/// input. Deserializes from an empty JSON object (`{}`), matching the
/// legacy per-module `struct EmptyArgs {}` that this replaces.
#[derive(Debug, Default, Deserialize)]
pub struct EmptyArgs {}

/// Extension trait that adds typed-handler registration to
/// [`CommandRegistry`]. Imported via
/// `use prism_daemon::typed_command::CommandRegistryExt;`.
pub trait CommandRegistryExt {
    /// Register a typed handler at the default `Permission::Dev` tier.
    /// `Req` and `Resp` are serde'd transparently; handler errors that
    /// implement [`Display`] are wrapped in [`CommandError::Handler`]
    /// with the command name attached.
    fn register_typed<Req, Resp, E, F>(
        &self,
        name: impl Into<String>,
        handler: F,
    ) -> Result<(), CommandError>
    where
        Req: DeserializeOwned,
        Resp: Serialize,
        E: Display,
        F: Fn(Req) -> Result<Resp, E> + Send + Sync + 'static;

    /// Like [`Self::register_typed`] but accepts an explicit minimum
    /// caller permission.
    fn register_typed_with_permission<Req, Resp, E, F>(
        &self,
        name: impl Into<String>,
        min: Permission,
        handler: F,
    ) -> Result<(), CommandError>
    where
        Req: DeserializeOwned,
        Resp: Serialize,
        E: Display,
        F: Fn(Req) -> Result<Resp, E> + Send + Sync + 'static;

    /// Shorthand for registering a typed handler at `Permission::User`.
    fn register_typed_user<Req, Resp, E, F>(
        &self,
        name: impl Into<String>,
        handler: F,
    ) -> Result<(), CommandError>
    where
        Req: DeserializeOwned,
        Resp: Serialize,
        E: Display,
        F: Fn(Req) -> Result<Resp, E> + Send + Sync + 'static,
    {
        self.register_typed_with_permission(name, Permission::User, handler)
    }
}

impl CommandRegistryExt for CommandRegistry {
    fn register_typed<Req, Resp, E, F>(
        &self,
        name: impl Into<String>,
        handler: F,
    ) -> Result<(), CommandError>
    where
        Req: DeserializeOwned,
        Resp: Serialize,
        E: Display,
        F: Fn(Req) -> Result<Resp, E> + Send + Sync + 'static,
    {
        self.register_typed_with_permission(name, Permission::Dev, handler)
    }

    fn register_typed_with_permission<Req, Resp, E, F>(
        &self,
        name: impl Into<String>,
        min: Permission,
        handler: F,
    ) -> Result<(), CommandError>
    where
        Req: DeserializeOwned,
        Resp: Serialize,
        E: Display,
        F: Fn(Req) -> Result<Resp, E> + Send + Sync + 'static,
    {
        let name = name.into();
        let owned = name.clone();
        self.register_with_permission(name, min, move |payload: JsonValue| {
            let req: Req = serde_json::from_value(payload)
                .map_err(|e| CommandError::handler(owned.clone(), e.to_string()))?;
            let resp =
                handler(req).map_err(|e| CommandError::handler(owned.clone(), e.to_string()))?;
            serde_json::to_value(resp)
                .map_err(|e| CommandError::handler(owned.clone(), e.to_string()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Deserialize)]
    struct EchoReq {
        msg: String,
    }

    #[derive(Serialize)]
    struct EchoResp {
        echoed: String,
    }

    #[test]
    fn typed_handler_round_trips() {
        let reg = CommandRegistry::default();
        reg.register_typed("echo.go", |req: EchoReq| {
            Ok::<_, std::convert::Infallible>(EchoResp { echoed: req.msg })
        })
        .unwrap();
        let out = reg.invoke("echo.go", json!({ "msg": "hi" })).unwrap();
        assert_eq!(out, json!({ "echoed": "hi" }));
    }

    #[test]
    fn typed_handler_propagates_handler_error() {
        let reg = CommandRegistry::default();
        reg.register_typed("fail.go", |_: EchoReq| Err::<EchoResp, _>("nope"))
            .unwrap();
        let err = reg.invoke("fail.go", json!({ "msg": "x" })).unwrap_err();
        match err {
            CommandError::Handler { command, message } => {
                assert_eq!(command, "fail.go");
                assert_eq!(message, "nope");
            }
            other => panic!("unexpected err: {other:?}"),
        }
    }

    #[test]
    fn typed_handler_reports_deserialize_error_under_command_name() {
        let reg = CommandRegistry::default();
        reg.register_typed("parse.go", |_req: EchoReq| {
            Ok::<_, std::convert::Infallible>(EchoResp {
                echoed: String::new(),
            })
        })
        .unwrap();
        let err = reg.invoke("parse.go", json!({ "wrong": 1 })).unwrap_err();
        match err {
            CommandError::Handler { command, .. } => assert_eq!(command, "parse.go"),
            other => panic!("unexpected err: {other:?}"),
        }
    }

    #[test]
    fn typed_user_shorthand_uses_user_permission() {
        let reg = CommandRegistry::default();
        reg.register_typed_user("u.go", |_: EchoReq| {
            Ok::<_, std::convert::Infallible>(EchoResp {
                echoed: String::new(),
            })
        })
        .unwrap();
        // Caller at User tier can invoke; Guest cannot.
        assert!(reg
            .invoke_with_permission("u.go", json!({ "msg": "" }), Permission::User)
            .is_ok());
    }
}
