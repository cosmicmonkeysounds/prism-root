//! Threshold multi-signature config and aggregation.
//!
//! Port of the `multi-sig` section of `identity/did/identity.ts`.
//! Each partial signature is individually verified against the
//! signer's DID (resolved via [`super::identity::resolve_identity`]).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::error::IdentityError;
use super::identity::{resolve_identity, PrismIdentity};
use super::types::Did;

/// A partial signature contribution from one signer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialSignature {
    /// DID of the signer.
    pub signer_did: Did,
    /// Ed25519 signature bytes (64 bytes).
    pub signature: Vec<u8>,
}

/// Aggregated multi-signature with threshold verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiSignature {
    /// Minimum number of valid signatures required.
    pub threshold: usize,
    /// All partial signatures collected.
    pub signatures: Vec<PartialSignature>,
}

/// Configuration for multi-sig vault ownership.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiSigConfig {
    /// Required number of signatures to authorise an action.
    pub threshold: usize,
    /// DIDs of all authorised signers.
    pub signers: Vec<Did>,
}

/// Create a multi-sig configuration for shared vault ownership.
pub fn create_multi_sig_config(
    threshold: usize,
    signers: Vec<Did>,
) -> Result<MultiSigConfig, IdentityError> {
    if threshold < 1 {
        return Err(IdentityError::ThresholdTooSmall);
    }
    if threshold > signers.len() {
        return Err(IdentityError::ThresholdExceedsSigners {
            threshold,
            signers: signers.len(),
        });
    }
    let unique: HashSet<&Did> = signers.iter().collect();
    if unique.len() != signers.len() {
        return Err(IdentityError::DuplicateSigners);
    }
    Ok(MultiSigConfig { threshold, signers })
}

/// Collect a partial signature from one signer.
pub fn create_partial_signature(identity: &PrismIdentity, data: &[u8]) -> PartialSignature {
    let sig = identity.sign_payload(data);
    PartialSignature {
        signer_did: identity.did.clone(),
        signature: sig.to_vec(),
    }
}

/// Assemble partial signatures into a [`MultiSignature`].
pub fn assemble_multi_signature(
    config: &MultiSigConfig,
    partials: Vec<PartialSignature>,
) -> Result<MultiSignature, IdentityError> {
    for partial in &partials {
        if !config.signers.contains(&partial.signer_did) {
            return Err(IdentityError::SignerNotInConfig(partial.signer_did.clone()));
        }
    }

    let mut seen: HashSet<&Did> = HashSet::new();
    for partial in &partials {
        if !seen.insert(&partial.signer_did) {
            return Err(IdentityError::DuplicatePartial);
        }
    }

    Ok(MultiSignature {
        threshold: config.threshold,
        signatures: partials,
    })
}

/// Verify a multi-signature meets the threshold requirement. Each
/// partial signature is individually verified against its signer's DID.
pub fn verify_multi_signature(
    config: &MultiSigConfig,
    multi_sig: &MultiSignature,
    data: &[u8],
) -> Result<bool, IdentityError> {
    if multi_sig.signatures.len() < config.threshold {
        return Ok(false);
    }

    let mut valid_count = 0usize;
    for partial in &multi_sig.signatures {
        if !config.signers.contains(&partial.signer_did) {
            continue;
        }
        let resolved = resolve_identity(&partial.signer_did)?;
        if resolved.verify_signature(data, &partial.signature) {
            valid_count += 1;
        }
        if valid_count >= config.threshold {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::did::identity::{create_identity, CreateIdentityOptions};

    #[test]
    fn creates_a_valid_multi_sig_config() {
        let config = create_multi_sig_config(
            2,
            vec![
                "did:key:z1".to_string(),
                "did:key:z2".to_string(),
                "did:key:z3".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(config.threshold, 2);
        assert_eq!(config.signers.len(), 3);
    }

    #[test]
    fn rejects_threshold_greater_than_signers() {
        let err =
            create_multi_sig_config(3, vec!["did:key:z1".to_string(), "did:key:z2".to_string()])
                .unwrap_err();
        assert!(matches!(err, IdentityError::ThresholdExceedsSigners { .. }));
    }

    #[test]
    fn rejects_threshold_less_than_one() {
        let err = create_multi_sig_config(0, vec!["did:key:z1".to_string()]).unwrap_err();
        assert!(matches!(err, IdentityError::ThresholdTooSmall));
    }

    #[test]
    fn rejects_duplicate_signers() {
        let err =
            create_multi_sig_config(1, vec!["did:key:z1".to_string(), "did:key:z1".to_string()])
                .unwrap_err();
        assert!(matches!(err, IdentityError::DuplicateSigners));
    }

    #[test]
    fn full_multi_sig_sign_and_verify_flow_2_of_3() {
        let alice = create_identity(CreateIdentityOptions::default()).unwrap();
        let bob = create_identity(CreateIdentityOptions::default()).unwrap();
        let charlie = create_identity(CreateIdentityOptions::default()).unwrap();

        let config = create_multi_sig_config(
            2,
            vec![alice.did.clone(), bob.did.clone(), charlie.did.clone()],
        )
        .unwrap();
        let data = b"vault-action";

        let alice_sig = create_partial_signature(&alice, data);
        let bob_sig = create_partial_signature(&bob, data);

        let multi_sig = assemble_multi_signature(&config, vec![alice_sig, bob_sig]).unwrap();
        assert_eq!(multi_sig.threshold, 2);
        assert_eq!(multi_sig.signatures.len(), 2);

        let valid = verify_multi_signature(&config, &multi_sig, data).unwrap();
        assert!(valid);
    }

    #[test]
    fn rejects_multi_sig_below_threshold() {
        let alice = create_identity(CreateIdentityOptions::default()).unwrap();
        let bob = create_identity(CreateIdentityOptions::default()).unwrap();

        let config = create_multi_sig_config(2, vec![alice.did.clone(), bob.did.clone()]).unwrap();
        let data = b"need-two";

        let alice_sig = create_partial_signature(&alice, data);
        let multi_sig = assemble_multi_signature(&config, vec![alice_sig]).unwrap();

        let valid = verify_multi_signature(&config, &multi_sig, data).unwrap();
        assert!(!valid);
    }

    #[test]
    fn rejects_partial_signature_from_non_signer() {
        let alice = create_identity(CreateIdentityOptions::default()).unwrap();
        let outsider = create_identity(CreateIdentityOptions::default()).unwrap();

        let config = create_multi_sig_config(1, vec![alice.did.clone()]).unwrap();
        let data = b"restricted";

        let outsider_sig = create_partial_signature(&outsider, data);
        let err = assemble_multi_signature(&config, vec![outsider_sig]).unwrap_err();
        assert!(matches!(err, IdentityError::SignerNotInConfig(_)));
    }

    #[test]
    fn rejects_duplicate_partial_signatures() {
        let alice = create_identity(CreateIdentityOptions::default()).unwrap();
        let config = create_multi_sig_config(1, vec![alice.did.clone()]).unwrap();
        let data = b"no-dupes";

        let sig1 = create_partial_signature(&alice, data);
        let sig2 = create_partial_signature(&alice, data);

        let err = assemble_multi_signature(&config, vec![sig1, sig2]).unwrap_err();
        assert!(matches!(err, IdentityError::DuplicatePartial));
    }

    #[test]
    fn rejects_multi_sig_with_tampered_data() {
        let alice = create_identity(CreateIdentityOptions::default()).unwrap();
        let bob = create_identity(CreateIdentityOptions::default()).unwrap();

        let config = create_multi_sig_config(2, vec![alice.did.clone(), bob.did.clone()]).unwrap();
        let data = b"original-data";
        let tampered = b"tampered-data";

        let alice_sig = create_partial_signature(&alice, data);
        let bob_sig = create_partial_signature(&bob, data);

        let multi_sig = assemble_multi_signature(&config, vec![alice_sig, bob_sig]).unwrap();

        let valid = verify_multi_signature(&config, &multi_sig, tampered).unwrap();
        assert!(!valid);
    }
}
