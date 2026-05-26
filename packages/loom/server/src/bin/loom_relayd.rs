//! `loom-relayd` — Loom multi-user backbone server.
//!
//! Boots the trimmed relay (collection-host + password-auth +
//! capability-tokens), binds an axum HTTP listener, and serves until
//! shutdown. Phase 8 — also serves the React editor's `dist/` build
//! when `--editor-dist` (or `LOOM_EDITOR_DIST`) is set, so a single
//! binary delivers both the editor and the API. See
//! `docs/dev/loom-multiuser.md` for the phased roadmap.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, ValueEnum};
use loom_server::{build_router_with, CorsMode, LoomRelayState, LoomServeConfig};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "loom-relayd",
    about = "Loom multi-user backbone — CRDT sync + auth + capability tokens + static editor",
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

    /// Path to the React editor's `dist/` directory (the output of
    /// `pnpm build` inside `packages/loom/editor`). When set, the
    /// server serves the editor at `/` with SPA-style fallback so
    /// deep links resolve to `index.html`. Also reads
    /// `LOOM_EDITOR_DIST` when the flag is absent.
    #[arg(long, env = "LOOM_EDITOR_DIST")]
    editor_dist: Option<PathBuf>,

    /// CORS posture for the API + WS routes. `same-origin` (the
    /// default) ships no `Access-Control-*` headers — appropriate for
    /// the single-binary deployment. `permissive` enables
    /// cross-origin requests so the Vite dev server on `:5173` can
    /// drive the relay on `:7878` during local development.
    #[arg(long, value_enum, default_value_t = CorsArg::SameOrigin)]
    cors: CorsArg,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CorsArg {
    SameOrigin,
    Permissive,
}

impl From<CorsArg> for CorsMode {
    fn from(value: CorsArg) -> Self {
        match value {
            CorsArg::SameOrigin => CorsMode::SameOrigin,
            CorsArg::Permissive => CorsMode::Permissive,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();

    let state = Arc::new(LoomRelayState::new(cli.relay_did));

    if let Some(dist) = &cli.editor_dist {
        if !dist.is_dir() {
            anyhow::bail!(
                "--editor-dist {} does not point at a directory (build the editor first \
                 with `prism loom build`)",
                dist.display()
            );
        }
        if !dist.join("index.html").is_file() {
            anyhow::bail!(
                "--editor-dist {} is missing index.html — is this really a Vite build output?",
                dist.display()
            );
        }
    }

    let config = LoomServeConfig {
        editor_dist: cli.editor_dist.clone(),
        cors: cli.cors.into(),
    };
    let app = build_router_with(state, config);

    let listener = tokio::net::TcpListener::bind(cli.bind)
        .await
        .with_context(|| format!("binding {}", cli.bind))?;

    match &cli.editor_dist {
        Some(dist) => tracing::info!(
            addr = %cli.bind,
            editor_dist = %dist.display(),
            "loom-relayd listening (editor served at /)"
        ),
        None => tracing::info!(addr = %cli.bind, "loom-relayd listening (API only)"),
    }
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
