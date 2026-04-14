//! Permission tiers — the daemon-side security boundary for IPC.
//!
//! Every command the daemon exposes carries a minimum `Permission` the
//! caller must hold to invoke it. The kernel stamps a single permission
//! onto itself at startup (from the `--permission` CLI flag, or from the
//! host transport's build config) and every `invoke()` is checked
//! against the command's minimum before the handler runs.
//!
//! Two tiers:
//!
//!   - **`User`** — end-user running a published Flux / Lattice /
//!     Musica / Studio build. Only commands explicitly opted into this
//!     level are reachable: introspection (`daemon.admin`,
//!     `daemon.capabilities`, `daemon.modules`) plus any read-only
//!     slices a module chooses to expose.
//!
//!   - **`Dev`** — developers (and the default when no flag is set, so
//!     every existing test keeps working unchanged). Full access to
//!     everything the registered modules can do: `crdt.write`,
//!     `luau.exec`, `build.run_step`, `vfs.put`, `crypto.*`, etc.
//!
//! This is the authoritative check. The Studio UI's `lensBundle
//! .minPermission` filter is a UX affordance only — a compromised UI
//! (or a direct `kernel.invoke` from a published app) still hits this
//! gate. Mirrors `Permission` in `@prism/core/lens`'s `shell-mode.ts`.

use serde::{Deserialize, Serialize};

/// Caller permission tier.
///
/// Ordered so `Dev >= User`; use [`Permission::at_least`] to compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// End-user in a published build. Only commands registered with
    /// `Permission::User` (via [`CommandRegistry::register_with_permission`]
    /// or the shorthand `register_user`) are reachable.
    User,
    /// Developer permission. Every registered command is reachable,
    /// including every legacy `register()` handler (which defaults to
    /// `Dev`).
    Dev,
}

impl Permission {
    /// True when `self` grants at least the rights of `required`.
    ///
    /// `Dev.at_least(User)` → true, `User.at_least(Dev)` → false.
    pub fn at_least(self, required: Permission) -> bool {
        self >= required
    }

    /// Parse from the CLI string. Accepts `"user"` and `"dev"` (case-
    /// insensitive) for ergonomic `--permission=user` flag handling.
    pub fn parse(s: &str) -> Result<Permission, ParsePermissionError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "user" => Ok(Permission::User),
            "dev" => Ok(Permission::Dev),
            other => Err(ParsePermissionError(other.to_string())),
        }
    }

    /// Canonical short name used in logs, error messages, and the
    /// stdio banner. Matches the input accepted by [`Permission::parse`]
    /// so a round-trip is always lossless.
    pub fn as_str(self) -> &'static str {
        match self {
            Permission::User => "user",
            Permission::Dev => "dev",
        }
    }
}

impl Default for Permission {
    /// Defaults to `Dev` so existing tests and embedded callers that
    /// don't opt in to the permission system keep seeing full access.
    /// Published binaries must flip this explicitly.
    fn default() -> Self {
        Permission::Dev
    }
}

/// Returned by [`Permission::parse`] for an invalid CLI value.
#[derive(Debug, Clone, thiserror::Error)]
#[error("unknown permission '{0}', expected 'user' or 'dev'")]
pub struct ParsePermissionError(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_canonical_values() {
        assert_eq!(Permission::parse("user").unwrap(), Permission::User);
        assert_eq!(Permission::parse("dev").unwrap(), Permission::Dev);
    }

    #[test]
    fn parse_is_case_insensitive_and_trims() {
        assert_eq!(Permission::parse("  USER ").unwrap(), Permission::User);
        assert_eq!(Permission::parse("Dev").unwrap(), Permission::Dev);
    }

    #[test]
    fn parse_rejects_unknown_values() {
        let err = Permission::parse("root").unwrap_err();
        assert_eq!(err.0, "root");
    }

    #[test]
    fn at_least_honours_tier_ordering() {
        assert!(Permission::Dev.at_least(Permission::User));
        assert!(Permission::Dev.at_least(Permission::Dev));
        assert!(Permission::User.at_least(Permission::User));
        assert!(!Permission::User.at_least(Permission::Dev));
    }

    #[test]
    fn default_is_dev() {
        assert_eq!(Permission::default(), Permission::Dev);
    }

    #[test]
    fn as_str_roundtrips_parse() {
        for p in [Permission::User, Permission::Dev] {
            assert_eq!(Permission::parse(p.as_str()).unwrap(), p);
        }
    }
}
