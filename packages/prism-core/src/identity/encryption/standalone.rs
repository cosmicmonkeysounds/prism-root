//! Standalone AES-GCM-256 encrypt / decrypt helpers for one-off use
//! without spinning up a [`super::VaultKeyManager`].
//!
//! Port of the TS `encryptSnapshot` / `decryptSnapshot` top-level
//! helpers in `identity/encryption/encryption.ts`.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;

use super::error::EncryptionError;

fn base64url_encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn base64url_decode(input: &str) -> Result<Vec<u8>, EncryptionError> {
    URL_SAFE_NO_PAD
        .decode(input)
        .map_err(|e| EncryptionError::Base64Decode(e.to_string()))
}

/// Result of a standalone [`encrypt_snapshot`] call.
#[derive(Debug, Clone)]
pub struct StandaloneCiphertext {
    /// Base64url-encoded 12-byte IV.
    pub iv: String,
    /// Ciphertext bytes (includes the 16-byte AES-GCM tag).
    pub ciphertext: Vec<u8>,
}

/// Encrypt `data` with a raw 32-byte AES-GCM-256 key.
pub fn encrypt_snapshot(
    data: &[u8],
    raw_key: &[u8],
    aad: Option<&str>,
) -> Result<StandaloneCiphertext, EncryptionError> {
    if raw_key.len() != 32 {
        return Err(EncryptionError::InvalidKeyLength {
            expected: 32,
            got: raw_key.len(),
        });
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(raw_key);
    let cipher = Aes256Gcm::new((&key).into());

    let mut iv = [0u8; 12];
    OsRng.fill_bytes(&mut iv);
    let nonce = Nonce::from_slice(&iv);

    let ciphertext = match aad {
        Some(a) => cipher
            .encrypt(
                nonce,
                Payload {
                    msg: data,
                    aad: a.as_bytes(),
                },
            )
            .map_err(|_| EncryptionError::EncryptFailed)?,
        None => cipher
            .encrypt(nonce, data)
            .map_err(|_| EncryptionError::EncryptFailed)?,
    };

    Ok(StandaloneCiphertext {
        iv: base64url_encode(&iv),
        ciphertext,
    })
}

/// Decrypt with a raw 32-byte AES-GCM-256 key.
pub fn decrypt_snapshot(
    iv: &str,
    ciphertext: &[u8],
    raw_key: &[u8],
    aad: Option<&str>,
) -> Result<Vec<u8>, EncryptionError> {
    if raw_key.len() != 32 {
        return Err(EncryptionError::InvalidKeyLength {
            expected: 32,
            got: raw_key.len(),
        });
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(raw_key);
    let cipher = Aes256Gcm::new((&key).into());

    let iv_bytes = base64url_decode(iv)?;
    let nonce = Nonce::from_slice(&iv_bytes);

    match aad {
        Some(a) => cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad: a.as_bytes(),
                },
            )
            .map_err(|_| EncryptionError::DecryptFailed),
        None => cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| EncryptionError::DecryptFailed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        OsRng.fill_bytes(&mut k);
        k
    }

    #[test]
    fn encrypts_and_decrypts_with_a_raw_key() {
        let raw_key = random_key();
        let plaintext = b"standalone-test";

        let ct = encrypt_snapshot(plaintext, &raw_key, None).unwrap();
        assert!(!ct.iv.is_empty());
        assert!(!ct.ciphertext.is_empty());

        let decrypted = decrypt_snapshot(&ct.iv, &ct.ciphertext, &raw_key, None).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn standalone_encrypt_with_aad() {
        let raw_key = random_key();
        let plaintext = b"aad-test";

        let ct = encrypt_snapshot(plaintext, &raw_key, Some("my-aad")).unwrap();
        let decrypted = decrypt_snapshot(&ct.iv, &ct.ciphertext, &raw_key, Some("my-aad")).unwrap();
        assert_eq!(decrypted, plaintext);

        // Wrong AAD fails.
        assert!(decrypt_snapshot(&ct.iv, &ct.ciphertext, &raw_key, Some("wrong")).is_err());
    }

    #[test]
    fn fails_with_wrong_key() {
        let key1 = random_key();
        let key2 = random_key();

        let plaintext = b"key-mismatch";
        let ct = encrypt_snapshot(plaintext, &key1, None).unwrap();

        assert!(decrypt_snapshot(&ct.iv, &ct.ciphertext, &key2, None).is_err());
    }
}
