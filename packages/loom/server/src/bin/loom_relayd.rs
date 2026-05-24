//! `loom-relayd` — Loom multi-user backbone server.
//!
//! Boots the trimmed relay (collection-host + password-auth +
//! capability-tokens), binds an axum HTTP listener, and serves until
//! shutdown. See `docs/dev/loom-multiuser.md` for the phased roadmap.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use loom_server::{build_router, LoomRelayState};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "loom-relayd",
    about = "Loom multi-user backbone — CRDT sync + auth + capability tokens",
    version
)]
struct Cli {
    /// Address to bind the HTTP listener on.
    #[arg(long, default_value = "127.0.0.1:7878")]
    bind: SocketAddr,

    /// DID the relay identifies itself with. Used as the issuer for
    /// capability tokens and the `relayDid` field of `/api/health`.
    #[arg(long, default_value = "did:key:loom-relay")]
    relay_did: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();

    let state = Arc::new(LoomRelayState::new(cli.relay_did));
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(cli.bind)
        .await
        .with_context(|| format!("binding {}", cli.bind))?;

    tracing::info!(addr = %cli.bind, "loom-relayd listening");
    axum::serve(listener, app)
        .await
        .context("axum::serve exited")?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,loom_server=debug"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
