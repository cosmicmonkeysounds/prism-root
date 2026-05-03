//! Shared transport adapter — translates a [`CommandError`] into the
//! transport's native response shape via the [`CommandErrorMapper`] trait.
//!
//! Each transport (HTTP, gRPC, IPC, …) used to hand-roll its own match
//! on every `CommandError` variant. The variant set is stable across
//! transports, but the response types are not — `axum::Response`,
//! `tonic::Status`, and `IpcResponse` carry incompatible shapes — so a
//! single trait with a method per variant lets each transport implement
//! the mapping once and reuse it.
//!
//! Transports call [`CommandErrorMapper::from_command_error`] (the
//! provided method) which fans out by variant; only the per-variant
//! constructors need to be supplied.

use crate::registry::CommandError;

/// Build a transport-native response from a [`CommandError`].
///
/// Implementors supply one constructor per variant and inherit the
/// fan-out via the provided [`from_command_error`] method.
///
/// `command` is the originating command name, repeated as a separate
/// argument because most transports want it visible in the response
/// envelope (HTTP JSON body, IPC error string, gRPC status message).
pub trait CommandErrorMapper: Sized {
    fn not_found(command: &str, message: &str) -> Self;
    fn already_registered(command: &str, message: &str) -> Self;
    fn handler_error(command: &str, message: &str) -> Self;
    fn lock_poisoned(command: &str, message: &str) -> Self;
    fn permission_denied(command: &str, message: &str) -> Self;

    fn from_command_error(command: &str, err: CommandError) -> Self {
        let message = err.to_string();
        match err {
            CommandError::NotFound(_) => Self::not_found(command, &message),
            CommandError::AlreadyRegistered { .. } => Self::already_registered(command, &message),
            CommandError::Handler { .. } => Self::handler_error(command, &message),
            CommandError::LockPoisoned => Self::lock_poisoned(command, &message),
            CommandError::PermissionDenied { .. } => Self::permission_denied(command, &message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiny test mapper that captures the variant + command + message
    /// so we can assert the trait fans out correctly.
    #[derive(Debug, PartialEq, Eq)]
    enum Captured {
        NotFound(String, String),
        AlreadyRegistered(String, String),
        Handler(String, String),
        Lock(String, String),
        Permission(String, String),
    }

    impl CommandErrorMapper for Captured {
        fn not_found(command: &str, message: &str) -> Self {
            Captured::NotFound(command.into(), message.into())
        }
        fn already_registered(command: &str, message: &str) -> Self {
            Captured::AlreadyRegistered(command.into(), message.into())
        }
        fn handler_error(command: &str, message: &str) -> Self {
            Captured::Handler(command.into(), message.into())
        }
        fn lock_poisoned(command: &str, message: &str) -> Self {
            Captured::Lock(command.into(), message.into())
        }
        fn permission_denied(command: &str, message: &str) -> Self {
            Captured::Permission(command.into(), message.into())
        }
    }

    #[test]
    fn dispatches_each_variant() {
        let nf = Captured::from_command_error("a.b", CommandError::NotFound("a.b".into()));
        assert!(matches!(nf, Captured::NotFound(ref c, _) if c == "a.b"));

        let ar = Captured::from_command_error(
            "a.b",
            CommandError::AlreadyRegistered {
                command: "a.b".into(),
            },
        );
        assert!(matches!(ar, Captured::AlreadyRegistered(_, _)));

        let h = Captured::from_command_error(
            "a.b",
            CommandError::Handler {
                command: "a.b".into(),
                message: "boom".into(),
            },
        );
        assert!(matches!(h, Captured::Handler(_, ref m) if m.contains("boom")));

        let l = Captured::from_command_error("a.b", CommandError::LockPoisoned);
        assert!(matches!(l, Captured::Lock(_, _)));

        let p = Captured::from_command_error(
            "a.b",
            CommandError::PermissionDenied {
                command: "a.b".into(),
                required: "User",
                caller: "Public",
            },
        );
        assert!(matches!(p, Captured::Permission(_, _)));
    }
}
