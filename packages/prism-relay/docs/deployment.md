# Prism Relay Deployment Guide

Production deployment guide for the Prism Relay server.

## Prerequisites

- **Node.js 22+** (LTS recommended)
- **pnpm 9+** (package manager)
- **~100 MB disk** for relay state, identity, and logs
- **Network**: port 4444 (configurable), WebSocket support required
- Outbound HTTPS if federation or webhook delivery is enabled

## Building

From the monorepo root:

```bash
pnpm install
pnpm --filter @prism/relay build
node packages/prism-relay/dist/cli.js --mode server
```

Or with environment variables:

```bash
PRISM_RELAY_MODE=server \
PRISM_RELAY_PORT=4444 \
PRISM_RELAY_HOST=0.0.0.0 \
  node packages/prism-relay/dist/cli.js
```

The relay generates a persistent Ed25519 identity on first run, saved to `~/.prism/relay/identity.json`. Subsequent starts reuse the same DID.

## Docker Deployment

The Dockerfile (`packages/prism-relay/Dockerfile`) is a multi-stage build with a slim production image:

- **Build stage**: installs deps, compiles TypeScript, prunes dev dependencies
- **Production stage**: non-root `prism` user, built-in HEALTHCHECK, VOLUME for persistent data
- **`.dockerignore`**: excludes node_modules, dist, tests, legacy packages, and git history

```bash
# Build image (from monorepo root)
docker build -f packages/prism-relay/Dockerfile -t prism-relay .

# Run
docker run -d \
  -p 4444:4444 \
  -v prism-relay-data:/home/prism/.prism/relay \
  --name prism-relay \
  prism-relay
```

The container runs as a non-root `prism` user. Identity and state are stored in the volume at `/home/prism/.prism/relay`, so the relay keeps its DID and data across container restarts.

### Docker Compose (Single Relay)

Use the provided `docker-compose.yml`:

```bash
cd packages/prism-relay
docker compose up -d --build
```

### Docker Compose (Federation Mesh)

For a two-relay federated mesh, use `docker-compose.federation.yml`:

```bash
cd packages/prism-relay
docker compose -f docker-compose.federation.yml up -d --build
```

This starts Relay A on port 4444 and Relay B on port 4445, connected via a shared Docker network. Relay B waits for Relay A to be healthy before starting.

### Environment Template

Copy `.env.example` to `.env` for local customization:

```bash
cp .env.example .env
# Edit .env with your values
docker compose --env-file .env up -d --build
```

## Configuration

Config is resolved with 4-layer priority: **CLI flags > environment variables > config file > mode defaults**.

### Full Config File Example (Server Mode)

Place a `relay.config.json` in the working directory, or pass `--config path/to/config.json`:

```json
{
  "mode": "server",
  "host": "0.0.0.0",
  "port": 4444,
  "dataDir": "/var/lib/prism-relay",
  "didMethod": "key",
  "hashcashBits": 16,
  "corsOrigins": ["https://app.example.com"],
  "modules": [
    "blind-mailbox",
    "relay-router",
    "relay-timestamp",
    "blind-ping",
    "capability-tokens",
    "webhooks",
    "sovereign-portals",
    "collection-host",
    "hashcash",
    "peer-trust",
    "escrow",
    "federation",
    "acme-certificates",
    "portal-templates",
    "webrtc-signaling"
  ],
  "federation": {
    "enabled": true,
    "publicUrl": "https://relay.example.com",
    "bootstrapPeers": [
      { "relayDid": "did:key:z6MkPeer1...", "url": "https://peer1.example.com" }
    ]
  },
  "relay": {
    "defaultTtlMs": 604800000,
    "maxEnvelopeSizeBytes": 1048576,
    "evictionIntervalMs": 60000
  },
  "logging": {
    "level": "info",
    "format": "json"
  }
}
```

Generate a starter config with:

```bash
prism-relay init --mode server -o relay.config.json
```

### Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `PRISM_RELAY_MODE` | Deployment mode | `server`, `p2p`, `dev` |
| `PRISM_RELAY_HOST` | Bind address | `0.0.0.0` |
| `PRISM_RELAY_PORT` | Listen port | `4444` |
| `PRISM_RELAY_DATA_DIR` | Data directory | `/var/lib/prism-relay` |
| `PRISM_RELAY_PUBLIC_URL` | Public URL for federation | `https://relay.example.com` |
| `PRISM_RELAY_LOG_LEVEL` | Log level | `debug`, `info`, `warn`, `error` |

## Deployment Modes

| Feature | Server | P2P | Dev |
|---------|--------|-----|-----|
| **Modules** | All 15 | Core + federation | All 15 |
| **Hashcash bits** | 16 | 12 | 4 |
| **CORS** | Explicit origins only | Explicit origins only | `*` (open) |
| **CSRF** | Enabled (`X-Prism-CSRF: 1` required) | Enabled | Disabled |
| **Logging format** | JSON (structured) | JSON | Text (human-readable) |
| **Log level** | `info` | `info` | `debug` |
| **Federation** | Opt-in | Enabled by default | Opt-in |
| **Use case** | Always-on production relay | Home server, VPS peer | Local development |

## TLS/SSL

### Option 1: ACME (Let's Encrypt) via Built-in Module

The relay includes an `acme-certificates` module that handles HTTP-01 challenges:

```bash
prism-relay start --mode server --port 80 \
  --public-url https://relay.example.com
```

Then register a challenge and complete the ACME flow via the API:

```bash
# Register an ACME challenge token
curl -X POST http://relay.example.com/api/acme/challenges \
  -H "Content-Type: application/json" \
  -H "X-Prism-CSRF: 1" \
  -d '{"token": "...", "keyAuthorization": "..."}'

# The relay responds at /.well-known/acme-challenge/:token
```

### Option 2: Reverse Proxy (Recommended)

For most deployments, place a reverse proxy in front of the relay for TLS termination.

#### Nginx

```nginx
server {
    listen 443 ssl;
    server_name relay.example.com;

    ssl_certificate /etc/letsencrypt/live/relay.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/relay.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:4444;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

WebSocket upgrade headers (`Upgrade` and `Connection`) are required for the `/ws/relay` endpoint.

#### Caddy

Caddy handles TLS automatically:

```
relay.example.com {
    reverse_proxy localhost:4444
}
```

Caddy automatically provisions a Let's Encrypt certificate and handles WebSocket proxying without additional configuration.

## Federation Setup

Relays can discover and communicate with each other in a mesh topology.

### Multi-Relay Mesh

```bash
# Relay A
prism-relay start --mode p2p --port 4444 \
  --public-url https://relay-a.example.com

# Relay B (bootstraps from Relay A)
prism-relay start --mode p2p --port 4444 \
  --public-url https://relay-b.example.com \
  --bootstrap-peer did:key:zRelayA@https://relay-a.example.com
```

On startup, each relay announces itself to its bootstrap peers. Peers exchange announcements via `POST /api/federation/announce`. Envelopes addressed to DIDs on other relays are forwarded automatically via `POST /api/federation/forward`.

### Federation in Server Mode

Federation is opt-in for server mode. Enable it in the config:

```json
{
  "mode": "server",
  "federation": {
    "enabled": true,
    "publicUrl": "https://relay.example.com",
    "bootstrapPeers": [
      { "relayDid": "did:key:z6Mk...", "url": "https://peer.example.com" }
    ]
  }
}
```

## Monitoring

### Health Endpoint

`GET /api/health` returns relay health data, suitable for load balancer checks and Docker health checks:

```json
{
  "ok": true,
  "uptime": 86400,
  "memoryMB": 47,
  "connections": 12,
  "mode": "server"
}
```

### Status Endpoint

`GET /api/status` returns detailed relay state:

```json
{
  "running": true,
  "did": "did:key:z6Mk...",
  "modules": ["blind-mailbox", "relay-router", "..."],
  "peers": 3,
  "config": { "..." }
}
```

### Checking Health from CLI

```bash
prism-relay status --port 4444
```

### Logs

- **Server mode**: structured JSON logs to stdout (pipe to your log aggregator)
- **Dev mode**: human-readable text logs

Tail logs from a running relay:

```bash
prism-relay logs --level error --follow
```

### Backup

```bash
# Export full relay state
prism-relay backup --output relay-backup-$(date +%Y%m%d).json

# Restore from backup
prism-relay restore --input relay-backup-20260407.json
```

State is also auto-saved every 5 seconds to `{dataDir}/relay-state.json`, and saved immediately on graceful shutdown.

## Security Hardening Checklist

1. **Use server mode** -- CSRF is enabled, no CORS wildcard
2. **Set hashcash-bits >= 16** for production spam protection
3. **Place behind a reverse proxy** with TLS (nginx or Caddy)
4. **Monitor `/api/health`** for uptime and connection count
5. **Regular backups** via CLI (`prism-relay backup`) or state file copy
6. **Review federation peers** periodically (`prism-relay peers list`)
7. **Ban malicious peers** via `prism-relay peers ban <did>`
8. **Back up `identity.json`** and keep it secure (this is the relay's private key)
9. **Use `did:web` for production** identity (verifiable domain binding)
10. **Rate limiting is built-in** (token bucket per IP), but consider additional proxy-level limits for DDoS protection

## Backup & Recovery

### Manual Backup

```bash
# Export state
prism-relay backup --output relay-backup-$(date +%Y%m%d).json

# Restore state
prism-relay restore --input relay-backup-20260407.json
```

### Automatic State Persistence

State is auto-saved every 5 seconds to `{dataDir}/relay-state.json`. This includes portals, webhooks, templates, certificates, peer trust data, revoked tokens, and collection CRDT snapshots.

### Identity Backup

The relay identity (`~/.prism/relay/identity.json`) contains the Ed25519 private key. Back this up separately and keep it secure. If lost, the relay will generate a new identity and all peers will see it as a different relay.

## Scaling

- **Single relay** handles thousands of concurrent WebSocket connections
- **Horizontal scaling** via federation -- deploy a mesh of relays that forward envelopes between each other
- **Collection data is in-memory** -- plan memory based on active collection count and average collection size
- **Separate workloads** -- consider dedicated relays for public portals vs. private sync vs. federation hubs
- **Stateless HTTP** -- the relay can sit behind a load balancer for HTTP API requests, but WebSocket connections are sticky to a single instance

## Deployment Files Reference

| File | Purpose |
|------|---------|
| `Dockerfile` | Multi-stage Docker build (build + slim production) |
| `.dockerignore` | Excludes unnecessary files from Docker context |
| `docker-compose.yml` | Single-relay deployment with health checks and volumes |
| `docker-compose.federation.yml` | Two-relay federated mesh deployment |
| `.env.example` | Environment variable template with all supported vars |
| `docs/deployment.md` | This guide |
| `docs/development.md` | Architecture, modules, testing, contributing |
| `e2e/deployment.spec.ts` | Deployment test suite (Dockerfile, config, modes, CORS, CSRF, backup) |
