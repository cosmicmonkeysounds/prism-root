//! `prism-relayd` — the Sovereign Portal SSR server binary.
//!
//! Thin clap wrapper around [`prism_relay::build_router`]. Boots
//! with a seeded set of sample portals so `cargo run -p prism-relay`
//! immediately serves something crawlable; real persistence lands
//! in a follow-on phase.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use prism_relay::{build_router, AppState};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "prism-relayd",
    about = "Prism Sovereign Portal SSR server",
    version
)]
struct Cli {
    /// Address to bind the HTTP server to. Defaults to the legacy
    /// relay port so existing dev tooling (`prism dev relay`) keeps
    /// pointing at `127.0.0.1:1420`.
    #[arg(long, default_value = "127.0.0.1:1420")]
    bind: SocketAddr,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();

    let state = Arc::new(AppState::with_sample_portals());
    let app = build_router(state);

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
