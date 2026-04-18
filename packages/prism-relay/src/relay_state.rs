//! Expanded relay state — wraps the module system for HTTP handlers.
//!
//! The relay instance holds all 17 modules' capabilities behind
//! `Arc<RelayInstance>`. Route handlers extract capabilities from
//! the instance via `get_capability<T>(name)`.

use std::sync::Arc;

use prism_core::network::relay::module_system::{
    capabilities, RelayBuilder, RelayInstance, RelayServerConfig,
};
use prism_core::network::relay::modules::{
    acme::AcmeCertificateManager, blind_mailbox::BlindMailbox, blind_ping::BlindPinger,
    capability_tokens::CapabilityTokenManager, collection_host::CollectionHost,
    escrow::RelayEscrowManager, federation::FederationRegistry, hashcash::HashcashGate,
    password_auth::RelayPasswordAuth, peer_trust::RelayTrustGraph,
    portal_templates::PortalTemplateRegistry, relay_router::RelayRouter, signaling::SignalingHub,
    sovereign_portals::PortalRegistry, timestamper::RelayTimestamper, vault_host::VaultHost,
    webhooks::WebhookEmitter,
};

use crate::config::RelayConfig;
use crate::middleware::metrics::RequestMetrics;
use crate::middleware::rate_limit::RateLimiter;

/// Full relay application state, shared across all handlers.
pub struct FullRelayState {
    pub relay: Arc<RelayInstance>,
    pub config: RelayConfig,
    pub metrics: RequestMetrics,
    pub rate_limiter: RateLimiter,
    pub relay_did: String,
    pub started_at: String,
}

impl FullRelayState {
    pub fn new(config: RelayConfig, relay_did: String) -> Self {
        let server_config = RelayServerConfig {
            relay_did: relay_did.clone(),
            default_ttl_ms: config.default_ttl_ms,
            max_envelope_size_bytes: config.max_envelope_size_bytes,
            eviction_interval_ms: config.eviction_interval_ms,
        };

        use prism_core::network::relay::modules::*;

        let relay = RelayBuilder::new(server_config)
            .use_module(blind_mailbox::BlindMailboxModule)
            .use_module(relay_router::RelayRouterModule)
            .use_module(timestamper::RelayTimestampModule)
            .use_module(blind_ping::BlindPingModule)
            .use_module(capability_tokens::CapabilityTokenModule)
            .use_module(webhooks::WebhookModule)
            .use_module(sovereign_portals::SovereignPortalModule)
            .use_module(signaling::WebrtcSignalingModule)
            .use_module(collection_host::CollectionHostModule)
            .use_module(vault_host::VaultHostModule)
            .use_module(hashcash::HashcashModule::new(config.hashcash_bits))
            .use_module(peer_trust::PeerTrustModule)
            .use_module(escrow::EscrowModule)
            .use_module(federation::FederationModule)
            .use_module(password_auth::PasswordAuthModule)
            .use_module(acme::AcmeCertificateModule)
            .use_module(portal_templates::PortalTemplateModule)
            .build()
            .expect("relay module build must succeed");

        Self {
            relay: Arc::new(relay),
            config,
            metrics: RequestMetrics::new(),
            rate_limiter: RateLimiter::new(100, 20, 10_000),
            relay_did,
            started_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    // ── Capability accessors ───────────────────────────────────────

    pub fn mailbox(&self) -> Arc<BlindMailbox> {
        self.relay.get_capability(capabilities::MAILBOX).unwrap()
    }

    pub fn router(&self) -> Arc<RelayRouter> {
        self.relay.get_capability(capabilities::ROUTER).unwrap()
    }

    pub fn timestamper(&self) -> Arc<RelayTimestamper> {
        self.relay
            .get_capability(capabilities::TIMESTAMPER)
            .unwrap()
    }

    pub fn pinger(&self) -> Arc<BlindPinger> {
        self.relay.get_capability(capabilities::PINGER).unwrap()
    }

    pub fn tokens(&self) -> Arc<CapabilityTokenManager> {
        self.relay.get_capability(capabilities::TOKENS).unwrap()
    }

    pub fn webhooks(&self) -> Arc<WebhookEmitter> {
        self.relay.get_capability(capabilities::WEBHOOKS).unwrap()
    }

    pub fn portal_registry(&self) -> Arc<PortalRegistry> {
        self.relay.get_capability(capabilities::PORTALS).unwrap()
    }

    pub fn collections(&self) -> Arc<CollectionHost> {
        self.relay
            .get_capability(capabilities::COLLECTIONS)
            .unwrap()
    }

    pub fn hashcash(&self) -> Arc<HashcashGate> {
        self.relay.get_capability(capabilities::HASHCASH).unwrap()
    }

    pub fn trust(&self) -> Arc<RelayTrustGraph> {
        self.relay.get_capability(capabilities::TRUST).unwrap()
    }

    pub fn escrow(&self) -> Arc<RelayEscrowManager> {
        self.relay.get_capability(capabilities::ESCROW).unwrap()
    }

    pub fn federation(&self) -> Arc<FederationRegistry> {
        self.relay.get_capability(capabilities::FEDERATION).unwrap()
    }

    pub fn password_auth(&self) -> Arc<RelayPasswordAuth> {
        self.relay
            .get_capability(capabilities::PASSWORD_AUTH)
            .unwrap()
    }

    pub fn acme(&self) -> Arc<AcmeCertificateManager> {
        self.relay.get_capability(capabilities::ACME).unwrap()
    }

    pub fn templates(&self) -> Arc<PortalTemplateRegistry> {
        self.relay.get_capability(capabilities::TEMPLATES).unwrap()
    }

    pub fn signaling(&self) -> Arc<SignalingHub> {
        self.relay.get_capability(capabilities::SIGNALING).unwrap()
    }

    pub fn vaults(&self) -> Arc<VaultHost> {
        self.relay.get_capability(capabilities::VAULT_HOST).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_with_all_modules() {
        let config = RelayConfig::default();
        let state = FullRelayState::new(config, "did:key:test".into());
        assert_eq!(state.relay.modules().len(), 17);
    }

    #[test]
    fn all_capabilities_accessible() {
        let state = FullRelayState::new(RelayConfig::default(), "did:key:test".into());
        // Verify every accessor works without panic
        let _ = state.mailbox();
        let _ = state.router();
        let _ = state.timestamper();
        let _ = state.pinger();
        let _ = state.tokens();
        let _ = state.webhooks();
        let _ = state.portal_registry();
        let _ = state.collections();
        let _ = state.hashcash();
        let _ = state.trust();
        let _ = state.escrow();
        let _ = state.federation();
        let _ = state.password_auth();
        let _ = state.acme();
        let _ = state.templates();
        let _ = state.signaling();
        let _ = state.vaults();
    }
}
