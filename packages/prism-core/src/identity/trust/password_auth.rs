//! Password authentication with PBKDF2-SHA256. Port of
//! `createPasswordAuthManager` in `trust/trust.ts`. Uses the `pbkdf2`
//! crate (SHA-256 via `hmac` + `sha2`) instead of `crypto.subtle` so
//! the same record format survives round-trips between the TS and
//! Rust hosts.

use std::collections::BTreeMap;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use chrono::{SecondsFormat, Utc};
use hmac::Hmac;
use indexmap::IndexMap;
use pbkdf2::pbkdf2;
use rand::RngCore;
use sha2::Sha256;
use thiserror::Error;

use super::types::{PasswordAuthManagerOptions, PasswordAuthRecord, PasswordAuthResult};

#[derive(Debug, Error)]
pub enum PasswordAuthError {
    #[error("username is required")]
    UsernameRequired,
    #[error("password is required")]
    PasswordRequired,
    #[error("newPassword is required")]
    NewPasswordRequired,
    #[error("username \"{0}\" is already registered")]
    AlreadyRegistered(String),
    #[error("invalid stored salt: {0}")]
    InvalidSalt(String),
}

/// Input bundle for [`PasswordAuthManager::register`]. Matches the
/// TS object literal field-for-field.
#[derive(Debug, Clone, Default)]
pub struct PasswordRegisterInput {
    pub username: String,
    pub password: String,
    pub did: Option<String>,
    pub metadata: Option<BTreeMap<String, String>>,
}

/// In-memory password manager. Records are keyed by normalised
/// (lower-cased, trimmed) username so lookups are case-insensitive.
#[derive(Debug, Default)]
pub struct PasswordAuthManager {
    iterations: u32,
    salt_bytes: usize,
    records: IndexMap<String, PasswordAuthRecord>,
}

pub fn create_password_auth_manager(options: PasswordAuthManagerOptions) -> PasswordAuthManager {
    PasswordAuthManager {
        iterations: options.iterations,
        salt_bytes: options.salt_bytes,
        records: IndexMap::new(),
    }
}

impl PasswordAuthManager {
    pub fn register(
        &mut self,
        input: PasswordRegisterInput,
    ) -> Result<PasswordAuthRecord, PasswordAuthError> {
        let username = normalize_username(&input.username);
        if username.is_empty() {
            return Err(PasswordAuthError::UsernameRequired);
        }
        if input.password.is_empty() {
            return Err(PasswordAuthError::PasswordRequired);
        }
        if self.records.contains_key(&username) {
            return Err(PasswordAuthError::AlreadyRegistered(username));
        }
        let salt = fresh_salt(self.salt_bytes);
        let password_hash = pbkdf2_hash(&input.password, &salt, self.iterations);
        let now = now_iso();
        let record = PasswordAuthRecord {
            username: username.clone(),
            did: input
                .did
                .unwrap_or_else(|| format!("did:password:{username}")),
            salt: BASE64.encode(&salt),
            password_hash,
            iterations: self.iterations,
            created_at: now.clone(),
            updated_at: now,
            metadata: input.metadata,
        };
        self.records.insert(username, record.clone());
        Ok(record)
    }

    pub fn verify(&self, username: &str, password: &str) -> PasswordAuthResult {
        let key = normalize_username(username);
        let Some(record) = self.records.get(&key) else {
            return PasswordAuthResult::UnknownUser;
        };
        let salt = match BASE64.decode(record.salt.as_bytes()) {
            Ok(bytes) => bytes,
            Err(_) => return PasswordAuthResult::WrongPassword,
        };
        let candidate = pbkdf2_hash(password, &salt, record.iterations);
        if constant_time_equal(candidate.as_bytes(), record.password_hash.as_bytes()) {
            PasswordAuthResult::Ok(record.clone())
        } else {
            PasswordAuthResult::WrongPassword
        }
    }

    pub fn change_password(
        &mut self,
        username: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<PasswordAuthResult, PasswordAuthError> {
        let result = self.verify(username, old_password);
        if !result.is_ok() {
            return Ok(result);
        }
        if new_password.is_empty() {
            return Err(PasswordAuthError::NewPasswordRequired);
        }
        let key = normalize_username(username);
        let Some(existing) = self.records.get(&key) else {
            return Ok(PasswordAuthResult::UnknownUser);
        };
        let salt = fresh_salt(self.salt_bytes);
        let password_hash = pbkdf2_hash(new_password, &salt, self.iterations);
        let next = PasswordAuthRecord {
            salt: BASE64.encode(&salt),
            password_hash,
            iterations: self.iterations,
            updated_at: now_iso(),
            ..existing.clone()
        };
        self.records.insert(key, next.clone());
        Ok(PasswordAuthResult::Ok(next))
    }

    pub fn get(&self, username: &str) -> Option<&PasswordAuthRecord> {
        self.records.get(&normalize_username(username))
    }

    pub fn list(&self) -> Vec<PasswordAuthRecord> {
        self.records.values().cloned().collect()
    }

    pub fn restore(&mut self, record: PasswordAuthRecord) {
        let key = normalize_username(&record.username);
        self.records.insert(key, record);
    }

    pub fn remove(&mut self, username: &str) -> bool {
        self.records
            .shift_remove(&normalize_username(username))
            .is_some()
    }

    pub fn size(&self) -> usize {
        self.records.len()
    }
}

fn normalize_username(username: &str) -> String {
    username.trim().to_lowercase()
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn fresh_salt(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}

fn pbkdf2_hash(password: &str, salt: &[u8], iterations: u32) -> String {
    let mut out = [0u8; 32];
    pbkdf2::<Hmac<Sha256>>(password.as_bytes(), salt, iterations, &mut out)
        .expect("HMAC can take any key length");
    BASE64.encode(out)
}

fn constant_time_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast_opts() -> PasswordAuthManagerOptions {
        PasswordAuthManagerOptions {
            iterations: 1000,
            salt_bytes: 16,
        }
    }

    fn register(
        auth: &mut PasswordAuthManager,
        username: &str,
        password: &str,
    ) -> PasswordAuthRecord {
        auth.register(PasswordRegisterInput {
            username: username.to_string(),
            password: password.to_string(),
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn registers_a_new_user_without_leaking_plaintext() {
        let mut auth = create_password_auth_manager(fast_opts());
        let record = auth
            .register(PasswordRegisterInput {
                username: "Alice".into(),
                password: "correct horse battery staple".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(record.username, "alice");
        assert_eq!(record.did, "did:password:alice");
        assert!(!record.password_hash.is_empty());
        assert!(!record.password_hash.contains("correct horse"));
        assert_eq!(record.iterations, 1000);
        assert_eq!(auth.size(), 1);
    }

    #[test]
    fn verifies_correct_password_and_rejects_wrong() {
        let mut auth = create_password_auth_manager(fast_opts());
        register(&mut auth, "alice", "secret");
        assert!(auth.verify("alice", "secret").is_ok());
        assert_eq!(
            auth.verify("alice", "wrong"),
            PasswordAuthResult::WrongPassword
        );
    }

    #[test]
    fn returns_unknown_user_for_missing_accounts() {
        let auth = create_password_auth_manager(fast_opts());
        assert_eq!(
            auth.verify("ghost", "anything"),
            PasswordAuthResult::UnknownUser
        );
    }

    #[test]
    fn rejects_duplicate_registration() {
        let mut auth = create_password_auth_manager(fast_opts());
        register(&mut auth, "alice", "secret");
        let err = auth
            .register(PasswordRegisterInput {
                username: "ALICE".into(),
                password: "again".into(),
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(err, PasswordAuthError::AlreadyRegistered(_)));
    }

    #[test]
    fn change_password_rotates_hash_and_invalidates_old() {
        let mut auth = create_password_auth_manager(fast_opts());
        register(&mut auth, "alice", "old-pass");
        let change = auth
            .change_password("alice", "old-pass", "new-pass")
            .unwrap();
        assert!(change.is_ok());
        assert_eq!(
            auth.verify("alice", "old-pass"),
            PasswordAuthResult::WrongPassword
        );
        assert!(auth.verify("alice", "new-pass").is_ok());
    }

    #[test]
    fn change_password_refuses_with_wrong_old() {
        let mut auth = create_password_auth_manager(fast_opts());
        register(&mut auth, "alice", "secret");
        let result = auth.change_password("alice", "guess", "new").unwrap();
        assert!(!result.is_ok());
    }

    #[test]
    fn remove_deletes_a_user() {
        let mut auth = create_password_auth_manager(fast_opts());
        register(&mut auth, "alice", "secret");
        assert!(auth.remove("alice"));
        assert!(auth.get("alice").is_none());
        assert!(!auth.remove("alice"));
    }

    #[test]
    fn restore_round_trips_a_record() {
        let mut auth = create_password_auth_manager(fast_opts());
        let record = register(&mut auth, "alice", "secret");
        let mut auth2 = create_password_auth_manager(fast_opts());
        auth2.restore(record);
        assert!(auth2.verify("alice", "secret").is_ok());
    }

    #[test]
    fn uses_unique_salt_per_user() {
        let mut auth = create_password_auth_manager(fast_opts());
        let a = register(&mut auth, "alice", "samepass");
        let b = register(&mut auth, "bob", "samepass");
        assert_ne!(a.salt, b.salt);
        assert_ne!(a.password_hash, b.password_hash);
    }

    #[test]
    fn accepts_custom_did_at_registration() {
        let mut auth = create_password_auth_manager(fast_opts());
        let record = auth
            .register(PasswordRegisterInput {
                username: "alice".into(),
                password: "secret".into(),
                did: Some("did:key:zCustom".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(record.did, "did:key:zCustom");
    }

    #[test]
    fn stores_caller_supplied_metadata() {
        let mut auth = create_password_auth_manager(fast_opts());
        let mut meta = BTreeMap::new();
        meta.insert("email".to_string(), "alice@example.com".to_string());
        let record = auth
            .register(PasswordRegisterInput {
                username: "alice".into(),
                password: "secret".into(),
                metadata: Some(meta.clone()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(record.metadata, Some(meta));
    }
}
