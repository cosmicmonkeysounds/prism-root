//! `identity::did` — W3C DID identities backed by Ed25519 keypairs.
//!
//! Port of `packages/prism-core/src/identity/did/` from the legacy TS
//! tree. The shape of the public API (DID document JSON, exported
//! identity JWK blobs, multi-sig structure) is preserved so wire
//! formats stay cross-compatible with anything the legacy package
//! might have written to disk.
//!
//! Submodules mirror the conceptual layers in the TS source:
//!
//! - [`base58`]   — base58btc encode/decode used by `did:key`
//! - [`did_key`]  — multicodec + `did:key:z…` encoding
//! - [`document`] — DID document construction (`buildDIDDocument`)
//! - [`error`]    — the `IdentityError` enum
//! - [`identity`] — [`PrismIdentity`], create/resolve/sign/verify, import/export
//! - [`multisig`] — threshold multi-sig config and signature aggregation
//! - [`types`]    — shared type aliases and plain-data structs

pub mod base58;
pub mod did_key;
pub mod document;
pub mod error;
pub mod identity;
pub mod multisig;
pub mod types;

pub use base58::{decode_base58, encode_base58};
pub use did_key::{did_key_to_public_key, public_key_to_did_key};
pub use document::build_did_document;
pub use error::IdentityError;
pub use identity::{
    base64url_encode, create_identity, export_identity, import_identity, resolve_identity,
    sign_payload, verify_signature, CreateIdentityOptions, ExportedIdentity, PrismIdentity,
    ResolvedIdentity,
};
pub use multisig::{
    assemble_multi_signature, create_multi_sig_config, create_partial_signature,
    verify_multi_signature, MultiSigConfig, MultiSignature, PartialSignature,
};
pub use types::{Did, DidDocument, DidMethod, VerificationMethod};
