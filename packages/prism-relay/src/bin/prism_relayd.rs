//! `prism-relayd` — the Prism relay server binary.
//!
//! Boots the full 17-module relay with config from env/CLI, binds
//! the HTTP + WebSocket server, and serves until shutdown.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use prism_relay::config::{RelayConfig, RelayMode};
use prism_relay::{build_full_router, FullRelayState};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "prism-relayd",
    about = "Prism relay server — 17-module relay protocol + Sovereign Portal SSR",
    version
)]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:1420")]
    bind: SocketAddr,

    #[arg(long, default_value = "dev")]
    mode: String,

    #[arg(long, default_value = "did:key:relay")]
    relay_did: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();

    let mode = match cli.mode.as_str() {
        "server" => RelayMode::Server,
        "p2p" => RelayMode::P2p,
        _ => RelayMode::Dev,
    };

    let config = RelayConfig::for_mode(mode).from_env();
    let state = Arc::new(FullRelayState::new(config, cli.relay_did));
    let app = build_full_router(state);

    let listener = tokio::net::TcpListener::bind(cli.bind)
        .await
        .with_context(|| format!("binding {}", cli.bind))?;

    tracing::info!(addr = %cli.bind, "prism-relayd listening");
    axum::serve(listener, app)
        .await
        .context("axum::serve exited")?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,prism_relay=debug"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
