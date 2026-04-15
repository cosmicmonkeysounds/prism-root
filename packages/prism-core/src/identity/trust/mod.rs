//! Trust & Safety layer — the Sovereign Immune System. Port of
//! `packages/prism-core/src/identity/trust/*.ts` from
//! `git show 8426588:…`. Each subsystem is pure in-memory and can be
//! used in isolation:
//!
//! - [`sandbox`] — Luau plugin capability enforcement (glob → regex).
//! - [`schema_validator`] — JSON poison-pill validator with composable
//!   rules (depth, string/array size, key count, disallowed keys).
//! - [`hashcash`] — SHA-256 proof-of-work minter + verifier.
//! - [`peer_trust_graph`] — web-of-trust reputation graph with an
//!   event bus.
//! - [`shamir`] — GF(256) Shamir secret sharing, interoperable with
//!   the TS bytes-in / hex-out layout.
//! - [`escrow`] — single-claim encrypted blob escrow with expiry.
//! - [`password_auth`] — PBKDF2-SHA256 password manager.
//!
//! Shared types live in [`types`].

pub mod escrow;
pub mod hashcash;
pub mod password_auth;
pub mod peer_trust_graph;
pub mod sandbox;
pub mod schema_validator;
pub mod shamir;
pub mod types;

pub use escrow::{create_escrow_manager, EscrowManager};
pub use hashcash::{
    create_hashcash_minter, create_hashcash_verifier, HashcashMinter, HashcashVerifier,
};
pub use password_auth::{
    create_password_auth_manager, PasswordAuthError, PasswordAuthManager, PasswordRegisterInput,
};
pub use peer_trust_graph::{create_peer_trust_graph, PeerTrustGraph, Subscription};
pub use sandbox::{create_luau_sandbox, LuauSandbox};
pub use schema_validator::{create_schema_validator, SchemaValidationRule, SchemaValidator};
pub use shamir::{create_shamir_splitter, ShamirError, ShamirSplitter};
pub use types::{
    default_disallowed_keys, ContentHash, EscrowDeposit, HashcashChallenge, HashcashProof,
    PasswordAuthManagerOptions, PasswordAuthRecord, PasswordAuthResult, PeerReputation,
    SandboxCapability, SandboxPolicy, SandboxViolation, SchemaValidationIssue,
    SchemaValidationResult, SchemaValidationSeverity, SchemaValidatorOptions, ShamirConfig,
    ShamirShare, TrustGraphEvent, TrustGraphOptions, TrustLevel,
};
