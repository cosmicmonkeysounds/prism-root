//! ACME certificates — Let's Encrypt HTTP-01 challenge management.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcmeChallenge {
    pub domain: String,
    pub token: String,
    pub key_authorization: String,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SslCertificate {
    pub domain: String,
    pub certificate: String,
    pub private_key: String,
    pub issued_at: String,
    pub expires_at: String,
    pub active: bool,
}

pub struct AcmeCertificateManager {
    challenges: RwLock<HashMap<String, AcmeChallenge>>,
    certificates: RwLock<HashMap<String, SslCertificate>>,
}

impl AcmeCertificateManager {
    pub fn new() -> Self {
        Self {
            challenges: RwLock::new(HashMap::new()),
            certificates: RwLock::new(HashMap::new()),
        }
    }

    pub fn add_challenge(&self, challenge: AcmeChallenge) {
        self.challenges
            .write()
            .unwrap()
            .insert(challenge.token.clone(), challenge);
    }

    pub fn get_challenge(&self, token: &str) -> Option<AcmeChallenge> {
        self.challenges.read().unwrap().get(token).cloned()
    }

    pub fn remove_challenge(&self, token: &str) -> bool {
        self.challenges.write().unwrap().remove(token).is_some()
    }

    pub fn set_certificate(&self, cert: SslCertificate) {
        self.certificates
            .write()
            .unwrap()
            .insert(cert.domain.clone(), cert);
    }

    pub fn get_certificate(&self, domain: &str) -> Option<SslCertificate> {
        self.certificates.read().unwrap().get(domain).cloned()
    }

    pub fn list_certificates(&self) -> Vec<SslCertificate> {
        self.certificates
            .read()
            .unwrap()
            .values()
            .map(|c| SslCertificate {
                private_key: String::new(), // Never expose private keys in listings
                ..c.clone()
            })
            .collect()
    }

    pub fn remove_certificate(&self, domain: &str) -> bool {
        self.certificates.write().unwrap().remove(domain).is_some()
    }

    pub fn evict_expired_challenges(&self, now_iso: &str) -> usize {
        let mut challenges = self.challenges.write().unwrap();
        let before = challenges.len();
        challenges.retain(|_, c| c.expires_at.as_str() > now_iso);
        before - challenges.len()
    }

    pub fn restore_certs(&self, certs: Vec<SslCertificate>) {
        let mut store = self.certificates.write().unwrap();
        for c in certs {
            store.insert(c.domain.clone(), c);
        }
    }
}

impl Default for AcmeCertificateManager {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AcmeCertificateModule;

impl RelayModule for AcmeCertificateModule {
    fn name(&self) -> &str {
        "acme-certificates"
    }
    fn description(&self) -> &str {
        "Let's Encrypt ACME HTTP-01 certificate management"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(capabilities::ACME, AcmeCertificateManager::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_get_challenge() {
        let mgr = AcmeCertificateManager::new();
        mgr.add_challenge(AcmeChallenge {
            domain: "example.com".into(),
            token: "tok-1".into(),
            key_authorization: "key-auth".into(),
            created_at: "2026-04-18T00:00:00Z".into(),
            expires_at: "2026-04-18T00:05:00Z".into(),
        });
        assert!(mgr.get_challenge("tok-1").is_some());
    }

    #[test]
    fn cert_listing_hides_private_key() {
        let mgr = AcmeCertificateManager::new();
        mgr.set_certificate(SslCertificate {
            domain: "example.com".into(),
            certificate: "CERT".into(),
            private_key: "SECRET".into(),
            issued_at: "2026-04-18T00:00:00Z".into(),
            expires_at: "2027-04-18T00:00:00Z".into(),
            active: true,
        });
        let list = mgr.list_certificates();
        assert_eq!(list.len(), 1);
        assert!(list[0].private_key.is_empty());
    }
}
