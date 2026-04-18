//! Password auth — username/password authentication.
//!
//! Thread-safe wrapper over PBKDF2-SHA256 password hashing for the relay server.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordRecord {
    pub username: String,
    pub did: Option<String>,
    pub salt: String,
    pub hash: String,
    pub created_at: String,
    pub metadata: Option<serde_json::Value>,
}

impl PasswordRecord {
    pub fn redacted(&self) -> serde_json::Value {
        serde_json::json!({
            "username": self.username,
            "did": self.did,
            "createdAt": self.created_at,
            "metadata": self.metadata,
        })
    }
}

pub struct RelayPasswordAuth {
    records: RwLock<HashMap<String, PasswordRecord>>,
    iterations: u32,
}

impl RelayPasswordAuth {
    pub fn new(iterations: u32) -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
            iterations,
        }
    }

    pub fn register(
        &self,
        username: &str,
        password: &str,
        did: Option<String>,
        metadata: Option<serde_json::Value>,
        now_iso: &str,
    ) -> Result<PasswordRecord, &'static str> {
        let key = username.to_lowercase();
        let mut records = self.records.write().unwrap();
        if records.contains_key(&key) {
            return Err("username already registered");
        }

        let salt = generate_salt();
        let hash = hash_password(password, &salt, self.iterations);

        let record = PasswordRecord {
            username: username.to_string(),
            did,
            salt,
            hash,
            created_at: now_iso.to_string(),
            metadata,
        };
        records.insert(key, record.clone());
        Ok(record)
    }

    pub fn login(&self, username: &str, password: &str) -> Result<PasswordRecord, &'static str> {
        let key = username.to_lowercase();
        let records = self.records.read().unwrap();
        let record = records.get(&key).ok_or("invalid credentials")?;
        let hash = hash_password(password, &record.salt, self.iterations);
        if hash != record.hash {
            return Err("invalid credentials");
        }
        Ok(record.clone())
    }

    pub fn change_password(
        &self,
        username: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), &'static str> {
        let key = username.to_lowercase();
        let mut records = self.records.write().unwrap();
        let record = records.get_mut(&key).ok_or("user not found")?;
        let old_hash = hash_password(old_password, &record.salt, self.iterations);
        if old_hash != record.hash {
            return Err("invalid credentials");
        }
        let new_salt = generate_salt();
        record.hash = hash_password(new_password, &new_salt, self.iterations);
        record.salt = new_salt;
        Ok(())
    }

    pub fn get(&self, username: &str) -> Option<PasswordRecord> {
        self.records
            .read()
            .unwrap()
            .get(&username.to_lowercase())
            .cloned()
    }

    pub fn remove(&self, username: &str, password: &str) -> Result<(), &'static str> {
        let key = username.to_lowercase();
        let mut records = self.records.write().unwrap();
        let record = records.get(&key).ok_or("user not found")?;
        let hash = hash_password(password, &record.salt, self.iterations);
        if hash != record.hash {
            return Err("invalid credentials");
        }
        records.remove(&key);
        Ok(())
    }

    pub fn restore(&self, records: Vec<PasswordRecord>) {
        let mut store = self.records.write().unwrap();
        for r in records {
            store.insert(r.username.to_lowercase(), r);
        }
    }
}

fn generate_salt() -> String {
    use rand::RngCore;
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, salt)
}

fn hash_password(password: &str, salt_b64: &str, iterations: u32) -> String {
    use hmac::Hmac;
    use sha2::Sha256;
    let salt = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, salt_b64)
        .unwrap_or_default();
    let mut result = [0u8; 32];
    pbkdf2::pbkdf2::<Hmac<Sha256>>(password.as_bytes(), &salt, iterations, &mut result).unwrap();
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, result)
}

pub struct PasswordAuthModule;

impl RelayModule for PasswordAuthModule {
    fn name(&self) -> &str {
        "password-auth"
    }
    fn description(&self) -> &str {
        "Username/password authentication (PBKDF2-SHA256)"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(capabilities::PASSWORD_AUTH, RelayPasswordAuth::new(600_000));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_login() {
        let auth = RelayPasswordAuth::new(1000); // Low iterations for tests
        auth.register("Alice", "secret123", None, None, "2026-04-18T00:00:00Z")
            .unwrap();
        assert!(auth.login("alice", "secret123").is_ok());
        assert!(auth.login("alice", "wrong").is_err());
    }

    #[test]
    fn duplicate_registration_rejected() {
        let auth = RelayPasswordAuth::new(1000);
        auth.register("alice", "pass1", None, None, "2026-04-18T00:00:00Z")
            .unwrap();
        assert!(auth
            .register("Alice", "pass2", None, None, "2026-04-18T00:00:00Z")
            .is_err());
    }

    #[test]
    fn change_password() {
        let auth = RelayPasswordAuth::new(1000);
        auth.register("alice", "old", None, None, "2026-04-18T00:00:00Z")
            .unwrap();
        auth.change_password("alice", "old", "new").unwrap();
        assert!(auth.login("alice", "old").is_err());
        assert!(auth.login("alice", "new").is_ok());
    }
}
