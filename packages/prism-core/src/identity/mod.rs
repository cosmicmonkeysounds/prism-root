//! `identity` — W3C DID identities, vault encryption, manifest, and
//! trust.
//!
//! Port of `packages/prism-core/src/identity/*` from the legacy TS tree.
//! Covers `did/` (Ed25519 DID identity, sign/verify, multi-sig,
//! import/export), `encryption/` (AES-GCM-256 vault key manager with
//! HKDF-derived keys, standalone encrypt/decrypt helpers), and
//! `manifest/` (on-disk `.prism.json` schema plus FileMaker-style
//! privilege sets).
//!
//! `trust/` is still TODO.

pub mod did;
pub mod encryption;
pub mod manifest;
