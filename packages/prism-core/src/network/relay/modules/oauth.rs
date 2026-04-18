//! OAuth 2.0 / OIDC — provider config, session state, identity linking.
//!
//! Thread-safe state manager for OAuth flows. The relay generates authorization
//! URLs and tracks CSRF state tokens; the client handles the provider redirect
//! and token exchange, then posts the profile back for identity linking.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OAuthProviderKind {
    Google,
    GitHub,
}

impl OAuthProviderKind {
    pub fn default_auth_url(&self) -> &'static str {
        match self {
            Self::Google => "https://accounts.google.com/o/oauth2/v2/auth",
            Self::GitHub => "https://github.com/login/oauth/authorize",
        }
    }

    pub fn default_scopes(&self) -> &'static str {
        match self {
            Self::Google => "openid email profile",
            Self::GitHub => "read:user user:email",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::GitHub => "github",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthProviderConfig {
    pub kind: OAuthProviderKind,
    pub client_id: String,
    pub auth_url: String,
    pub scopes: String,
    pub redirect_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthSession {
    pub state: String,
    pub provider: OAuthProviderKind,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthIdentity {
    pub provider: OAuthProviderKind,
    pub provider_user_id: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub did: String,
    pub linked_at: String,
}

pub struct RelayOAuth {
    providers: RwLock<HashMap<OAuthProviderKind, OAuthProviderConfig>>,
    sessions: RwLock<HashMap<String, OAuthSession>>,
    identities: RwLock<Vec<OAuthIdentity>>,
}

impl RelayOAuth {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            identities: RwLock::new(Vec::new()),
        }
    }

    pub fn add_provider(&self, config: OAuthProviderConfig) {
        self.providers.write().unwrap().insert(config.kind, config);
    }

    pub fn get_provider(&self, kind: OAuthProviderKind) -> Option<OAuthProviderConfig> {
        self.providers.read().unwrap().get(&kind).cloned()
    }

    pub fn list_providers(&self) -> Vec<OAuthProviderKind> {
        self.providers.read().unwrap().keys().copied().collect()
    }

    pub fn create_session(&self, provider: OAuthProviderKind, now_iso: &str) -> OAuthSession {
        use rand::RngCore;
        let mut state_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut state_bytes);
        let state = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            state_bytes,
        );

        let session = OAuthSession {
            state: state.clone(),
            provider,
            created_at: now_iso.to_string(),
        };
        self.sessions
            .write()
            .unwrap()
            .insert(state, session.clone());
        session
    }

    pub fn validate_session(&self, state: &str) -> Option<OAuthSession> {
        self.sessions.write().unwrap().remove(state)
    }

    pub fn build_auth_url(&self, provider: OAuthProviderKind, state: &str) -> Option<String> {
        let config = self.get_provider(provider)?;
        Some(format!(
            "{}?client_id={}&redirect_uri={}&scope={}&state={}&response_type=code",
            config.auth_url,
            percent_encode(&config.client_id),
            percent_encode(&config.redirect_uri),
            percent_encode(&config.scopes),
            percent_encode(state),
        ))
    }

    pub fn link_identity(&self, identity: OAuthIdentity) {
        let mut identities = self.identities.write().unwrap();
        identities.retain(|i| {
            !(i.provider == identity.provider && i.provider_user_id == identity.provider_user_id)
        });
        identities.push(identity);
    }

    pub fn get_identity_by_provider(
        &self,
        provider: OAuthProviderKind,
        provider_user_id: &str,
    ) -> Option<OAuthIdentity> {
        self.identities
            .read()
            .unwrap()
            .iter()
            .find(|i| i.provider == provider && i.provider_user_id == provider_user_id)
            .cloned()
    }

    pub fn get_identities_for_did(&self, did: &str) -> Vec<OAuthIdentity> {
        self.identities
            .read()
            .unwrap()
            .iter()
            .filter(|i| i.did == did)
            .cloned()
            .collect()
    }

    pub fn restore_identities(&self, identities: Vec<OAuthIdentity>) {
        let mut store = self.identities.write().unwrap();
        store.extend(identities);
    }
}

fn percent_encode(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0x0f) as usize] as char);
            }
        }
    }
    out
}

impl Default for RelayOAuth {
    fn default() -> Self {
        Self::new()
    }
}

pub struct OAuthModule;

impl RelayModule for OAuthModule {
    fn name(&self) -> &str {
        "oauth"
    }
    fn description(&self) -> &str {
        "OAuth 2.0 / OIDC provider integration (Google, GitHub)"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(capabilities::OAUTH, RelayOAuth::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn google_config() -> OAuthProviderConfig {
        OAuthProviderConfig {
            kind: OAuthProviderKind::Google,
            client_id: "test-client-id".into(),
            auth_url: OAuthProviderKind::Google.default_auth_url().into(),
            scopes: OAuthProviderKind::Google.default_scopes().into(),
            redirect_uri: "http://localhost:1420/auth/callback".into(),
        }
    }

    #[test]
    fn add_and_list_providers() {
        let oauth = RelayOAuth::new();
        assert!(oauth.list_providers().is_empty());
        oauth.add_provider(google_config());
        assert_eq!(oauth.list_providers().len(), 1);
    }

    #[test]
    fn create_and_validate_session() {
        let oauth = RelayOAuth::new();
        let session = oauth.create_session(OAuthProviderKind::Google, "2026-04-18T00:00:00Z");
        assert_eq!(session.provider, OAuthProviderKind::Google);

        let validated = oauth.validate_session(&session.state).unwrap();
        assert_eq!(validated.state, session.state);

        assert!(oauth.validate_session(&session.state).is_none());
    }

    #[test]
    fn build_auth_url() {
        let oauth = RelayOAuth::new();
        oauth.add_provider(google_config());
        let session = oauth.create_session(OAuthProviderKind::Google, "2026-04-18T00:00:00Z");
        let url = oauth
            .build_auth_url(OAuthProviderKind::Google, &session.state)
            .unwrap();
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("client_id=test-client-id"));
        assert!(url.contains("response_type=code"));
    }

    #[test]
    fn build_auth_url_missing_provider() {
        let oauth = RelayOAuth::new();
        assert!(oauth
            .build_auth_url(OAuthProviderKind::GitHub, "state")
            .is_none());
    }

    #[test]
    fn link_and_lookup_identity() {
        let oauth = RelayOAuth::new();
        oauth.link_identity(OAuthIdentity {
            provider: OAuthProviderKind::Google,
            provider_user_id: "12345".into(),
            email: Some("user@example.com".into()),
            display_name: Some("Test User".into()),
            did: "did:key:test".into(),
            linked_at: "2026-04-18T00:00:00Z".into(),
        });

        let identity = oauth
            .get_identity_by_provider(OAuthProviderKind::Google, "12345")
            .unwrap();
        assert_eq!(identity.did, "did:key:test");

        let by_did = oauth.get_identities_for_did("did:key:test");
        assert_eq!(by_did.len(), 1);
    }

    #[test]
    fn link_replaces_existing() {
        let oauth = RelayOAuth::new();
        oauth.link_identity(OAuthIdentity {
            provider: OAuthProviderKind::Google,
            provider_user_id: "12345".into(),
            email: None,
            display_name: None,
            did: "did:key:old".into(),
            linked_at: "2026-04-18T00:00:00Z".into(),
        });
        oauth.link_identity(OAuthIdentity {
            provider: OAuthProviderKind::Google,
            provider_user_id: "12345".into(),
            email: None,
            display_name: None,
            did: "did:key:new".into(),
            linked_at: "2026-04-18T00:00:00Z".into(),
        });

        let identity = oauth
            .get_identity_by_provider(OAuthProviderKind::Google, "12345")
            .unwrap();
        assert_eq!(identity.did, "did:key:new");
    }

    #[test]
    fn percent_encode_special_chars() {
        assert_eq!(percent_encode("hello world"), "hello%20world");
        assert_eq!(percent_encode("a&b=c"), "a%26b%3Dc");
        assert_eq!(percent_encode("safe-_.~"), "safe-_.~");
    }
}
