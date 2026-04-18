//! Full relay router — wires all API routes, SSR pages, WebSocket, and middleware.

use std::sync::Arc;

use axum::{
    middleware as axum_mw,
    routing::{delete, get, post, put},
    Router,
};

use crate::middleware::{body_limit, csrf};
use crate::relay_state::FullRelayState;
use crate::routes;
use crate::ws;

pub fn build_full_router(state: Arc<FullRelayState>) -> Router {
    let api = Router::new()
        // Status / health
        .route("/status", get(routes::status::api_status))
        .route("/modules", get(routes::status::api_modules))
        .route("/health", get(routes::status::api_health))
        // Portals
        .route("/portals", get(routes::portals::list_portals))
        .route("/portals", post(routes::portals::create_portal))
        .route("/portals/{id}", get(routes::portals::get_portal))
        .route("/portals/{id}", delete(routes::portals::delete_portal))
        .route("/portals/{id}/export", get(routes::portals::export_portal))
        // Collections
        .route("/collections", get(routes::collections::list_collections))
        .route("/collections", post(routes::collections::create_collection))
        .route(
            "/collections/{id}/snapshot",
            get(routes::collections::get_snapshot),
        )
        .route(
            "/collections/{id}/snapshot",
            post(routes::collections::import_snapshot),
        )
        .route(
            "/collections/{id}",
            delete(routes::collections::delete_collection),
        )
        // AutoREST
        .route("/rest/{collection_id}", get(routes::autorest::list_objects))
        .route(
            "/rest/{collection_id}",
            post(routes::autorest::create_object),
        )
        .route(
            "/rest/{collection_id}/{object_id}",
            get(routes::autorest::get_object),
        )
        .route(
            "/rest/{collection_id}/{object_id}",
            put(routes::autorest::update_object),
        )
        .route(
            "/rest/{collection_id}/{object_id}",
            delete(routes::autorest::delete_object),
        )
        // Auth (OAuth stubs)
        .route("/auth/providers", get(routes::auth::list_providers))
        .route("/auth/google/redirect", get(routes::auth::google_redirect))
        .route("/auth/github/redirect", get(routes::auth::github_redirect))
        .route("/auth/google/callback", post(routes::auth::google_callback))
        .route("/auth/github/callback", post(routes::auth::github_callback))
        .route("/auth/escrow/derive", post(routes::auth::escrow_derive))
        .route("/auth/escrow/recover", post(routes::auth::escrow_recover))
        // Password auth
        .route(
            "/auth/password/register",
            post(routes::auth_password::register),
        )
        .route("/auth/password/login", post(routes::auth_password::login))
        .route(
            "/auth/password/change",
            post(routes::auth_password::change_password),
        )
        .route(
            "/auth/password/user/{username}",
            get(routes::auth_password::get_user),
        )
        .route(
            "/auth/password/user/{username}",
            delete(routes::auth_password::delete_user),
        )
        // Webhooks
        .route("/webhooks", get(routes::webhooks::list_webhooks))
        .route("/webhooks", post(routes::webhooks::create_webhook))
        .route("/webhooks/{id}", delete(routes::webhooks::delete_webhook))
        .route(
            "/webhooks/{id}/deliveries",
            get(routes::webhooks::get_deliveries),
        )
        .route("/webhooks/{id}/test", post(routes::webhooks::test_webhook))
        // Capability tokens
        .route("/tokens", get(routes::tokens::list_tokens))
        .route("/tokens/issue", post(routes::tokens::issue_token))
        .route("/tokens/verify", post(routes::tokens::verify_token))
        .route("/tokens/revoke", post(routes::tokens::revoke_token))
        // Trust
        .route("/trust", get(routes::trust::list_trust))
        .route("/trust/{did}", get(routes::trust::get_peer_trust))
        .route("/trust/{did}/ban", post(routes::trust::ban_peer))
        .route("/trust/{did}/unban", post(routes::trust::unban_peer))
        // Safety
        .route("/safety/report", post(routes::safety::report))
        .route("/safety/hashes", get(routes::safety::list_hashes))
        .route("/safety/hashes/import", post(routes::safety::import_hashes))
        .route("/safety/hashes/check", post(routes::safety::check_hashes))
        .route("/safety/hashes/gossip", post(routes::safety::gossip_hashes))
        // Escrow
        .route("/escrow/deposit", post(routes::escrow::deposit))
        .route("/escrow/claim", post(routes::escrow::claim))
        .route("/escrow/{depositor_id}", get(routes::escrow::list_deposits))
        // Hashcash
        .route(
            "/hashcash/challenge",
            post(routes::hashcash::create_challenge),
        )
        .route("/hashcash/verify", post(routes::hashcash::verify_proof))
        // Federation
        .route("/federation/announce", post(routes::federation::announce))
        .route("/federation/peers", get(routes::federation::list_peers))
        .route("/federation/forward", post(routes::federation::forward))
        .route("/federation/sync", post(routes::federation::sync_receive))
        // Pings
        .route("/pings/register", post(routes::pings::register_device))
        .route(
            "/pings/unregister/{did}",
            post(routes::pings::unregister_device),
        )
        .route("/pings/devices", get(routes::pings::list_devices))
        .route("/pings/send", post(routes::pings::send_ping))
        .route("/pings/wake", post(routes::pings::wake))
        // Presence
        .route("/presence", get(routes::presence::get_presence))
        // Signaling
        .route("/signaling/rooms", get(routes::signaling::list_rooms))
        .route(
            "/signaling/rooms/{room_id}/peers",
            get(routes::signaling::get_peers),
        )
        .route(
            "/signaling/rooms/{room_id}/join",
            post(routes::signaling::join_room),
        )
        .route(
            "/signaling/rooms/{room_id}/leave",
            post(routes::signaling::leave_room),
        )
        .route(
            "/signaling/rooms/{room_id}/signal",
            post(routes::signaling::relay_signal),
        )
        // Templates
        .route("/templates", get(routes::templates::list_templates))
        .route("/templates", post(routes::templates::create_template))
        .route("/templates/{id}", get(routes::templates::get_template))
        .route(
            "/templates/{id}",
            delete(routes::templates::delete_template),
        )
        // Vaults
        .route("/vaults", get(routes::vaults::list_vaults))
        .route("/vaults", post(routes::vaults::publish_vault))
        .route("/vaults/{id}", get(routes::vaults::get_vault))
        .route(
            "/vaults/{id}/collections",
            get(routes::vaults::get_vault_collections),
        )
        .route(
            "/vaults/{id}/collections/{coll_id}",
            get(routes::vaults::get_vault_collection),
        )
        .route(
            "/vaults/{id}/collections",
            put(routes::vaults::update_vault_collections),
        )
        .route("/vaults/{id}/download", get(routes::vaults::download_vault))
        .route("/vaults/{id}", delete(routes::vaults::delete_vault))
        // Backup
        .route("/backup", get(routes::backup::export_backup))
        .route("/backup", post(routes::backup::import_backup))
        // Logs
        .route("/logs", get(routes::logs::get_logs))
        .route("/logs", delete(routes::logs::clear_logs))
        // Email
        .route("/email/status", get(routes::email::email_status))
        .route("/email/send", post(routes::email::send_email))
        // Directory
        .route("/directory", get(routes::directory::directory));

    Router::new()
        // ACME challenge (outside /api prefix, no CSRF)
        .route(
            "/.well-known/acme-challenge/{token}",
            get(routes::acme::acme_challenge_response),
        )
        // Admin (outside /api prefix, no CSRF)
        .route("/admin", get(routes::admin::admin_dashboard))
        .route("/admin/api/snapshot", get(routes::admin::admin_snapshot))
        // Metrics (outside /api prefix)
        .route("/metrics", get(routes::metrics::prometheus_metrics))
        // WebSocket
        .route("/ws", get(ws::ws_upgrade))
        // API routes under /api prefix
        .nest("/api", api)
        // ACME certificate management (under /api)
        .route("/api/acme/challenges", post(routes::acme::add_challenge))
        .route(
            "/api/acme/challenges/{token}",
            delete(routes::acme::remove_challenge),
        )
        .route(
            "/api/acme/certificates",
            get(routes::acme::list_certificates),
        )
        .route(
            "/api/acme/certificates",
            post(routes::acme::add_certificate),
        )
        .route(
            "/api/acme/certificates/{domain}",
            get(routes::acme::get_certificate),
        )
        // Middleware
        .layer(axum_mw::from_fn(csrf::csrf_middleware))
        .layer(axum_mw::from_fn(body_limit::body_limit_middleware))
        .with_state(state)
}
