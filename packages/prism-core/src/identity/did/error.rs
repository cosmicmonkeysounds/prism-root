//! `IdentityError` — fallible operations across the DID module.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("invalid did:key format: {0}")]
    InvalidDidKey(String),

    #[error("unsupported multibase encoding (expected 'z' prefix): {0}")]
    UnsupportedMultibase(String),

    #[error("not an Ed25519 multicodec key")]
    NotEd25519Multicodec,

    #[error("invalid base58 character: {0}")]
    InvalidBase58Char(char),

    #[error("did:web requires a domain")]
    DidWebMissingDomain,

    #[error("did:web resolution requires a network resolver (not yet implemented)")]
    DidWebNotImplemented,

    #[error("unsupported DID method: {0}")]
    UnsupportedDidMethod(String),

    #[error("signature verification failed")]
    SignatureInvalid,

    #[error("invalid ed25519 key: {0}")]
    InvalidKey(String),

    #[error("invalid ed25519 signature: {0}")]
    InvalidSignature(String),

    #[error("threshold must be at least 1")]
    ThresholdTooSmall,

    #[error("threshold ({threshold}) exceeds number of signers ({signers})")]
    ThresholdExceedsSigners { threshold: usize, signers: usize },

    #[error("duplicate signers not allowed")]
    DuplicateSigners,

    #[error("signer {0} is not in the multi-sig config")]
    SignerNotInConfig(String),

    #[error("duplicate partial signatures from the same signer")]
    DuplicatePartial,

    #[error("base64 decode error: {0}")]
    Base64Decode(String),

    #[error("serde error: {0}")]
    Serde(String),
}
