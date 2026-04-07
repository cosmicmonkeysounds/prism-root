# Prism Relay

The Prism Relay replaces the traditional "server" in fullstack apps. It's a zero-knowledge encrypted router — it sees addressing metadata (`from`, `to`) but never decrypts your CRDT data. Anyone can run a Relay, and Relays can federate with each other regardless of who hosts them.

## Quick Start

```bash
# Development (hot reload, debug logging, CORS=*)
pnpm dev

# Or with explicit mode
npx tsx src/cli.ts --mode dev
```

The relay generates a persistent Ed25519 identity on first run, saved to `~/.prism/relay/identity.json`. Subsequent starts reuse the same DID.

## Documentation

- **[Deployment Guide](docs/deployment.md)** -- Production setup, Docker, TLS, federation, monitoring
- **[Development Guide](docs/development.md)** -- Architecture, adding modules, testing, contributing

## Deployment Modes

### Server Mode — Production

Always-on relay with all 15 modules, JSON logging, hashcash=16 spam protection.

```bash
# Build for production
pnpm build

# Run
node dist/cli.js --mode server --port 4444 --host 0.0.0.0
```

### P2P Mode — Federated Peer

Minimal module set, federation enabled by default. Run on a home server, VPS, or any machine connected to the internet.

```bash
node dist/cli.js --mode p2p \
  --public-url https://my-relay.example.com \
  --bootstrap-peer did:key:zPeer1@https://peer1.example.com
```

### Dev Mode — Local Testing

All modules, hashcash=4 (fast), CORS=*, debug logging.

```bash
pnpm dev
# or
npx tsx src/cli.ts --mode dev --port 3000
```

## Docker

```bash
# Build image (from monorepo root)
docker build -f packages/prism-relay/Dockerfile -t prism-relay .

# Run
docker run -p 4444:4444 \
  -v prism-relay-data:/root/.prism/relay \
  -e PRISM_RELAY_MODE=server \
  prism-relay
```

The identity file is stored in the volume, so the relay keeps its DID across container restarts.

## Configuration

Config is resolved with this priority: **CLI flags > environment variables > config file > mode defaults**.

### Config File

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
    "capability-tokens",
    "collection-host",
    "hashcash",
    "peer-trust",
    "escrow",
    "federation"
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

### Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `PRISM_RELAY_MODE` | Deployment mode | `server`, `p2p`, `dev` |
| `PRISM_RELAY_HOST` | Bind address | `0.0.0.0` |
| `PRISM_RELAY_PORT` | Listen port | `4444` |
| `PRISM_RELAY_DATA_DIR` | Data directory | `/var/lib/prism-relay` |
| `PRISM_RELAY_PUBLIC_URL` | Public URL for federation | `https://relay.example.com` |
| `PRISM_RELAY_LOG_LEVEL` | Log level | `debug`, `info`, `warn`, `error` |

### CLI Flags

```
prism-relay [OPTIONS]

  -c, --config <path>        Config file path
  --mode <mode>              server | p2p | dev
  --port <number>            Listen port (default: 4444)
  --host <address>           Bind address
  --data-dir <path>          Data directory (default: ~/.prism/relay)
  --identity <path>          Identity key file
  --modules <list>           Comma-separated module names
  --cors <origins>           Comma-separated CORS origins
  --hashcash-bits <number>   Proof-of-work difficulty
  --did-method <key|web>     DID method for identity
  --did-web-domain <domain>  Domain for did:web
  --public-url <url>         Public URL for federation
  --bootstrap-peer <list>    did@url pairs (comma-separated)
  --log-level <level>        debug | info | warn | error
  --log-format <format>      text | json
  -h, --help                 Show help
  -v, --version              Show version
```

### Management Commands

Connect to a running relay and manage it remotely:

```bash
prism-relay peers list                    # List federation peers
prism-relay collections list              # List hosted collections
prism-relay portals list                  # List published portals
prism-relay webhooks list                 # List registered webhooks
prism-relay tokens list                   # List active tokens
prism-relay certs list                    # List ACME certificates
prism-relay backup --output state.json    # Export relay state
prism-relay restore --input state.json    # Import relay state
prism-relay logs --level error --follow   # Tail logs
```

Use `--port` and `--host` to target a specific relay (default: localhost:4444).

## Modules

The relay is modular — pick the modules you need via config or `--modules` flag.

| Module | Description | Default (server) | Default (p2p) |
|--------|-------------|:-:|:-:|
| `blind-mailbox` | E2EE store-and-forward for offline peers | Y | Y |
| `relay-router` | Zero-knowledge envelope routing | Y | Y |
| `relay-timestamp` | Cryptographic timestamps on envelopes | Y | Y |
| `blind-ping` | Push notification triggers (APNs/FCM) | Y | - |
| `capability-tokens` | Scoped access tokens (Ed25519 signed) | Y | Y |
| `webhooks` | Outgoing HTTP on CRDT changes | Y | - |
| `sovereign-portals` | SSR'd public web portals from CRDT data | Y | - |
| `collection-host` | Host CRDT collections for remote sync | Y | - |
| `hashcash` | SHA-256 proof-of-work spam protection | Y | Y |
| `peer-trust` | Peer reputation (trust/distrust/ban) | Y | Y |
| `escrow` | Blind key recovery deposits | Y | - |
| `federation` | Relay-to-relay mesh networking | Y | Y |
| `acme-certificates` | ACME HTTP-01 challenge + certificate lifecycle | Y | - |
| `portal-templates` | Reusable portal layout templates | Y | - |
| `webrtc-signaling` | P2P/SFU connection negotiation (rooms, SDP relay) | Y | - |

## Studio Integration

Prism Studio (the universal host) connects to Relays as a client for:
- Publishing Sovereign Portals (Levels 1-4)
- Syncing CRDT collections
- Managing federation peers, webhooks, certificates
- Monitoring relay health and backing up state

The Studio Relay Panel provides a full management UI. See the [Studio CLAUDE.md](../prism-studio/CLAUDE.md) for details.

## Connecting from an App

Any Prism app can connect to a deployed relay using the client SDK:

```typescript
import { createIdentity } from "@prism/core/identity";
import { createRelayClient } from "@prism/core/relay";

const identity = await createIdentity({ method: "key" });

const client = createRelayClient({
  url: "wss://relay.example.com/ws/relay",
  identity,
  autoReconnect: true,
});

await client.connect();
console.log(`Connected to relay: ${client.relayDid}`);
console.log(`Modules: ${client.modules.join(", ")}`);
```

### Sending Encrypted Envelopes

```typescript
// Encrypt your data first (relay never sees plaintext)
const ciphertext = await encrypt(myData, recipientPublicKey);

const result = await client.send({
  to: recipientDid,
  ciphertext,
  ttlMs: 7 * 24 * 60 * 60 * 1000, // 7 days
});

console.log(result.status); // "delivered" or "queued"
```

### Receiving Envelopes

```typescript
client.on("envelope", (env) => {
  const plaintext = await decrypt(env.ciphertext, myPrivateKey);
  // env.from, env.to, env.submittedAt also available
});
```

### Syncing CRDT Collections

```typescript
// Request a snapshot (also subscribes to live updates)
const snapshot = await client.syncRequest("my-collection-id");
myLocalStore.import(snapshot);

// Push local changes
const update = myLocalStore.exportSnapshot();
client.syncUpdate("my-collection-id", update);

// Receive updates from other peers
client.on("sync-update", ({ collectionId, update }) => {
  myLocalStore.import(update);
});
```

### State & Events

```typescript
client.state;  // "disconnected" | "connecting" | "authenticating" | "connected" | "reconnecting"

client.on("connected", ({ relayDid, modules }) => { ... });
client.on("disconnected", ({ reason }) => { ... });
client.on("error", ({ message }) => { ... });
client.on("state-change", ({ from, to }) => { ... });

client.close();  // Graceful disconnect
```

## HTTP API

All routes are under `/api`. The relay also serves WebSocket at `/ws/relay`.

### Status
- `GET /api/status` — `{ running, did, modules, peers, config }`
- `GET /api/modules` — `[{ name, description }]`

### Envelope Routing (via WebSocket)
Envelopes are routed over WebSocket, not HTTP. See the Protocol section below.

### Collections
- `POST /api/collections` — `{ id }` → create hosted collection
- `GET /api/collections` — list collection IDs
- `GET /api/collections/:id/snapshot` — export CRDT snapshot (base64)
- `POST /api/collections/:id/import` — import CRDT data

### Capability Tokens
- `POST /api/tokens/issue` — `{ subject, permissions, scope }` → signed token
- `POST /api/tokens/verify` — verify a token
- `POST /api/tokens/revoke` — revoke by tokenId

### Webhooks
- `POST /api/webhooks` — `{ url, events, active }` → register
- `GET /api/webhooks` — list all
- `DELETE /api/webhooks/:id` — remove

### Portals (API)
- `POST /api/portals` — `{ name, level, collectionId, basePath, isPublic }` → register
- `GET /api/portals` — list all
- `GET /api/portals/:id` — get one
- `DELETE /api/portals/:id` — remove

### Portal Rendering (HTML)
- `GET /portals` — list all public portals as an HTML index page
- `GET /portals/:id` — render a portal as a full HTML page (SSR via Hono JSX)
- `GET /portals/:id/snapshot.json` — portal data as JSON snapshot

Level 1 portals render as static HTML snapshots. Level 2+ portals include a WebSocket client script that subscribes to collection updates and reloads on changes.

### Hashcash (Spam Protection)
- `POST /api/hashcash/challenge` — `{ resource }` → `{ resource, bits, salt, issuedAt }`
- `POST /api/hashcash/verify` — `{ challenge, counter, hash }` → `{ valid }`

### Trust
- `GET /api/trust` — list all peers
- `GET /api/trust/:did` — get peer info
- `POST /api/trust/:did/ban` — `{ reason }` → ban peer
- `POST /api/trust/:did/unban` — unban peer

### Escrow (Key Recovery)
- `POST /api/escrow/deposit` — `{ depositorId, encryptedPayload }` → deposit
- `POST /api/escrow/claim` — `{ depositId }` → claim (one-time)
- `GET /api/escrow/:depositorId` — list deposits

### Federation
- `POST /api/federation/announce` — `{ relayDid, url }` → register peer
- `GET /api/federation/peers` — list known relays
- `POST /api/federation/forward` — forward envelope to another relay

## WebSocket Protocol

Connect to `ws[s]://host:port/ws/relay`. All messages are JSON.

### Client → Relay

```jsonc
// Authenticate (required first message)
{ "type": "auth", "did": "did:key:z6Mk..." }

// Send encrypted envelope
{ "type": "envelope", "envelope": { "id", "from", "to", "ciphertext", "submittedAt", "ttlMs" } }

// Request collection snapshot (subscribes to updates)
{ "type": "sync-request", "collectionId": "..." }

// Push CRDT update to collection
{ "type": "sync-update", "collectionId": "...", "update": "<base64>" }

// Submit hashcash proof
{ "type": "hashcash-proof", "proof": { "challenge": {...}, "counter": N, "hash": "..." } }

// Heartbeat
{ "type": "ping" }
```

### Relay → Client

```jsonc
// Auth success
{ "type": "auth-ok", "relayDid": "did:key:...", "modules": ["..."] }

// Inbound envelope (from another peer)
{ "type": "envelope", "envelope": { ... } }

// Route result
{ "type": "route-result", "result": { "status": "delivered" | "queued" } }

// Collection snapshot
{ "type": "sync-snapshot", "collectionId": "...", "snapshot": "<base64>" }

// Collection update broadcast
{ "type": "sync-update", "collectionId": "...", "update": "<base64>" }

// Hashcash challenge
{ "type": "hashcash-challenge", "challenge": { "resource", "bits", "salt", "issuedAt" } }

// Hashcash verified
{ "type": "hashcash-ok" }

// Error
{ "type": "error", "message": "..." }

// Heartbeat response
{ "type": "pong" }
```

## Federation

Relays can discover and communicate with each other. When federation is enabled:

1. On startup, the relay announces itself to bootstrap peers
2. Peers exchange announcements via `POST /api/federation/announce`
3. Envelopes addressed to DIDs on other relays are forwarded automatically
4. The forward transport uses HTTP POST to the peer's `/api/federation/forward`

```bash
# Run two federated relays
node dist/cli.js --mode p2p --port 4444 --public-url http://relay-a:4444
node dist/cli.js --mode p2p --port 5555 --public-url http://relay-b:5555 \
  --bootstrap-peer did:key:zRelayA@http://relay-a:4444
```

## Cloud Deployment Examples

### OVHCloud / Any VPS

```bash
# On the server
git clone <repo> && cd prism-root
pnpm install
pnpm --filter @prism/relay build

# Run with systemd, pm2, or screen
PRISM_RELAY_MODE=server \
PRISM_RELAY_PORT=4444 \
PRISM_RELAY_PUBLIC_URL=https://relay.yourdomain.com \
  node packages/prism-relay/dist/cli.js
```

Put a reverse proxy (nginx/caddy) in front for TLS:

```nginx
server {
    listen 443 ssl;
    server_name relay.yourdomain.com;

    location / {
        proxy_pass http://127.0.0.1:4444;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
    }
}
```

### Docker Compose

```yaml
version: "3.8"
services:
  relay:
    build:
      context: .
      dockerfile: packages/prism-relay/Dockerfile
    ports:
      - "4444:4444"
    volumes:
      - relay-data:/root/.prism/relay
    environment:
      PRISM_RELAY_MODE: server
      PRISM_RELAY_PUBLIC_URL: https://relay.yourdomain.com
    restart: unless-stopped

volumes:
  relay-data:
```

## Testing

```bash
# Unit tests (vitest)
pnpm test

# E2E tests (playwright — no browser needed)
pnpm test:e2e

# Type check
pnpm typecheck
```

## Architecture

```
src/
  cli.ts                    CLI entry point
  index.ts                  Package exports
  config/
    relay-config.ts         Config types + resolution
    parse-args.ts           CLI argument parser
    logger.ts               Structured logger
  server/
    relay-server.ts         Hono app factory + CORS + WS
  middleware/
    csrf.ts                 CSRF protection (X-Prism-CSRF header)
    body-size.ts            Content-Length enforcement
    banned-peer.ts          X-Prism-DID peer rejection
  routes/
    status-routes.ts        GET /api/status, /api/modules, /api/health
    webhook-routes.ts       Webhook CRUD
    portal-routes.ts        Portal manifest CRUD + HTML rendering (JSX SSR)
    token-routes.ts         Capability token issue/verify/revoke
    collection-routes.ts    Collection hosting + AutoREST gateway
    hashcash-routes.ts      Proof-of-work challenge/verify
    trust-routes.ts         Peer trust management
    escrow-routes.ts        Key recovery escrow
    federation-routes.ts    Relay-to-relay mesh
    ping-routes.ts          Blind push notification registration
    signaling-routes.ts     WebRTC signaling rooms
    auth-routes.ts          OAuth/OIDC (Google, GitHub)
    safety-routes.ts        Content flagging + toxic hash gossip
    acme-routes.ts          ACME HTTP-01 challenges + cert lifecycle
    seo-routes.ts           sitemap.xml, robots.txt
  transport/
    ws-transport.ts         WebSocket message handler
    connection-registry.ts  Connection tracking + broadcast
  protocol/
    relay-protocol.ts       Wire format types + serialization
  persistence/
    file-store.ts           JSON file store for relay state
  e2e/
    relay.spec.ts                 Core E2E (87 tests)
    production-readiness.spec.ts  Security + resilience (48 tests)
```

## Security

- **Zero-knowledge**: The relay routes encrypted envelopes. It never sees plaintext CRDT data.
- **Ed25519 identity**: All relay identities use Ed25519 keypairs with W3C DID format.
- **Capability tokens**: Scoped, signed tokens for access control.
- **Hashcash**: Configurable proof-of-work prevents spam and abuse.
- **Peer trust**: Ban/unban peers, track reputation.
- **No persistence of envelope content**: Envelopes are held in memory only, with TTL-based eviction.
