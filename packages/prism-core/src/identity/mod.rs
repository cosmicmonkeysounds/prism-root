//! `identity` — W3C DID identities, vault encryption, manifest, and
//! trust.
//!
//! Port of `packages/prism-core/src/identity/*` from the legacy TS tree.
//! Covers `did/` (Ed25519 DID identity, sign/verify, multi-sig,
//! import/export), `encryption/` (AES-GCM-256 vault key manager with
//! HKDF-derived keys, standalone encrypt/decrypt helpers),
//! `manifest/` (on-disk `.prism.json` schema plus FileMaker-style
//! privilege sets), and `trust/` (Layer-1 sovereign immune system:
//! sandbox, schema validator, hashcash, web-of-trust, Shamir,
//! escrow, password auth).

pub mod did;
pub mod encryption;
pub mod manifest;
pub mod trust;
