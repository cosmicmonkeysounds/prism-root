# @prism/relay

Runtime server for Prism Relay — wraps Layer 1 relay primitives in HTTP + WebSocket.

## Build & Test
- `pnpm dev` — start with tsx watch (dev mode)
- `pnpm typecheck`
- `npx vitest run` — unit tests (35+ test files)
- `pnpm test:e2e` — Playwright E2E tests (4 spec files, no browser needed)
- `pnpm test:docker` — Docker E2E tests (builds image, runs containers, tests API/WS/federation)

## CLI
Installable as `prism-relay` via the `bin` field. 30+ subcommands:

### Server Lifecycle
- `prism-relay start` (default) — start the relay server
- `prism-relay init [--mode server|p2p|dev] [-o path]` — generate a starter config file
- `prism-relay status [--port N]` — check health of a running relay via `/api/health`
- `prism-relay identity show` — display relay DID and public key
- `prism-relay identity regenerate` — generate new identity (backs up old one)
- `prism-relay modules list` — list all 15 available relay modules with descriptions
- `prism-relay config validate [-c path]` — validate config without starting
- `prism-relay config show [--mode ...]` — show fully resolved config with defaults

### Remote Management (connect to running relay via HTTP)
- `prism-relay peers list|ban|unban` — federation peer management
- `prism-relay collections list|inspect|export|import|delete` — collection management
- `prism-relay portals list|inspect|delete` — portal management
- `prism-relay webhooks list|delete|test` — webhook management
- `prism-relay tokens list|revoke` — capability token management
- `prism-relay certs list|renew` — ACME certificate management
- `prism-relay backup [--output path]` — export relay state
- `prism-relay restore [--input path]` — import relay state
- `prism-relay logs [--level L] [--follow]` — view/tail relay logs

Three deployment modes:
- `--mode server` — always-on relay (all modules, hashcash=16, JSON logging, no CORS, CSRF enabled)
- `--mode p2p` — federated peer (minimal modules, federation enabled, hashcash=12)
- `--mode dev` — local development (all modules, hashcash=4, CORS=*, CSRF disabled, debug logging)

Config priority: CLI flags > env vars > config file > mode defaults.
Identity persists to `~/.prism/relay/identity.json` (auto-created on first run).
State persists to `{dataDir}/relay-state.json` (auto-save every 5s, save on shutdown).
Background jobs: mailbox eviction, ACME challenge eviction, signaling room cleanup.
See `prism-relay --help` for full options.

## Architecture
- **Hono** HTTP framework with `@hono/node-ws` for WebSocket upgrade
- **Config system**: `relay.config.json` or CLI flags, env var overrides
- **Identity persistence**: Ed25519 JWK export/import via `@prism/core/identity`
- **State persistence**: JSON file store for portals, webhooks, templates, certs, trust, collections
- **ConnectionRegistry** tracks WS connections + collection subscriptions for broadcast
- **Security middleware**: CSRF (X-Prism-CSRF header), body size limits, banned peer rejection
- **CORS middleware** configurable per deployment mode (includes X-Prism-CSRF, X-Prism-DID headers)
- **Structured logger** (text or JSON format, configurable level)

## Exports
- `@prism/relay/server` — Hono app factory (`createRelayServer`)
- `@prism/relay/protocol` — WebSocket wire protocol types + serialization
- `@prism/relay/config` — Config resolution, arg parsing, logger
- `@prism/relay/cli` — CLI entry point

## Modules (16 total)
blind-mailbox, relay-router, relay-timestamp, blind-ping, capability-tokens, webhooks, sovereign-portals, collection-host, hashcash, peer-trust, escrow, federation, acme-certificates, portal-templates, webrtc-signaling, vault-host

## Protocol
WebSocket at `/ws/relay`:
1. Client sends `{ type: "auth", did }` → Relay replies `{ type: "auth-ok" }`
2. Client sends `{ type: "envelope", envelope }` → Relay routes + replies `{ type: "route-result" }`
3. Relay pushes `{ type: "envelope" }` for inbound envelopes
4. `{ type: "sync-request", collectionId }` → `{ type: "sync-snapshot", collectionId, snapshot }`
5. `{ type: "sync-update", collectionId, update }` → broadcast to subscribers
6. `{ type: "hashcash-proof", proof }` → `{ type: "hashcash-ok" }` or error
7. `{ type: "ping" }` → `{ type: "pong" }`

## Sovereign Portals (HTML Rendering — Level 1-4)
- Uses Hono's built-in JSX (`jsxImportSource: "hono/jsx"` in tsconfig)
- `GET /portals` — lists public portals as HTML
- `GET /portals/:id` — renders portal as HTML with OpenGraph + Twitter Card + JSON-LD metadata
- `GET /portals/:id/snapshot.json` — raw JSON snapshot for API consumers
- `POST /portals/:id/submit` — Level 3+ form submission (creates object with ephemeral DID)

### Portal Levels
- **Level 1**: Static read-only HTML snapshot
- **Level 2**: Live incremental DOM patching via WebSocket (fetches snapshot.json + patches #portal-content)
- **Level 3**: Interactive forms with ephemeral DID auth, capability token verification for non-public portals
- **Level 4**: Full client-side hydration with `window.__PRISM_PORTAL__` API (subscribe/notify, bidirectional CRDT sync, sendUpdate/submitObject)

## SEO
- `GET /sitemap.xml` — auto-generated XML sitemap from public portals
- `GET /robots.txt` — crawler directives (allow /portals/, disallow /api/)
- Portal pages include `og:title`, `og:description`, `og:type`, `og:site_name`, Twitter Card, JSON-LD

## Security
- **CSRF**: `X-Prism-CSRF: 1` header required on all POST/PUT/DELETE to `/api/*` (disabled in dev mode)
- **Body size**: Content-Length checked against `config.relay.maxEnvelopeSizeBytes` (default 1MB)
- **Banned peers**: `X-Prism-DID` header checked against PeerTrustGraph
- **Webhook delivery**: real HTTP POST with 10s timeout in production (dry-run in tests)

## Auth (OAuth/OIDC)
- `GET /api/auth/providers` — list configured providers
- `GET /api/auth/google` → Google OIDC redirect
- `POST /api/auth/callback/google` — exchange code for session token
- `GET /api/auth/github` → GitHub OAuth redirect
- `POST /api/auth/callback/github` — exchange code for session token
- Session tokens are Prism capability tokens (configurable TTL, default 24h)

## Blind Escrow (Key Recovery)
- `POST /api/auth/escrow/derive` — PBKDF2 key derivation (password + OAuth salt) → store encrypted vault key
- `POST /api/auth/escrow/recover` — verify password + salt → return encrypted vault key

## AutoREST API Gateway
- `GET /api/rest/:collectionId` — list objects (supports `type`, `status`, `tag`, `limit`, `offset` query params)
- `GET /api/rest/:collectionId/:objectId` — get single object
- `POST /api/rest/:collectionId` — create object (fires `object.created` webhook)
- `PUT /api/rest/:collectionId/:objectId` — update object (fires `object.updated` webhook)
- `DELETE /api/rest/:collectionId/:objectId` — soft delete (fires `object.deleted` webhook)
- All endpoints support capability token auth via `Authorization: Bearer <base64-token>`

## Trust & Safety
- `POST /api/safety/report` — submit whistleblower packet (flags content hash)
- `GET /api/safety/hashes` — list all flagged toxic hashes
- `POST /api/safety/hashes` — import toxic hashes from federated peer
- `POST /api/safety/check` — batch verify content hashes against flagged DB
- `POST /api/safety/gossip` — push flagged hashes to all federation peers

## Blind Pings (Push Notifications)
- `POST /api/pings/register` — register device token (APNs or FCM)
- `DELETE /api/pings/register/:did` — unregister device tokens
- `GET /api/pings/devices` — list registered devices
- `POST /api/pings/send` — send blind ping to DID via BlindPinger module
- `POST /api/pings/wake` — send pings to all devices for a DID
- `createPushPingTransport(config)` — concrete APNs/FCM transport factory

## Vault Host (Persistent Vault Storage)
- **Module**: `vault-host` — opt-in via `.use(vaultHostModule())`
- Relays can act as persistent storage nodes for complete vaults (manifest + collection snapshots)
- Stores opaque binary blobs — encryption is client-side (relay is storage-agnostic)
- `POST /api/vaults` — publish vault (manifest + base64 collection snapshots)
- `GET /api/vaults` — list vaults (`?public=true`, `?search=term`)
- `GET /api/vaults/:id` — vault metadata + manifest
- `GET /api/vaults/:id/collections` — list collection IDs with sizes
- `GET /api/vaults/:id/collections/:cid` — single collection snapshot (base64)
- `GET /api/vaults/:id/download` — full vault (manifest + all snapshots)
- `PUT /api/vaults/:id/collections` — update snapshots (owner-authenticated via ownerDid)
- `DELETE /api/vaults/:id` — remove vault (owner-authenticated)

## Directory Feed (Relay Discovery)
- `GET /api/directory` — public JSON feed of relay profile + public portals + public vaults
- Cacheable (5 min TTL via Cache-Control header), no auth required
- Config: `directory.name`, `directory.description`, `directory.listed` (default: true)
- Returns: relay DID, modules, federation info, uptime, public portals list, public vaults list
- Designed for aggregation by Nexus or other directory crawlers

## Persistence
- **File store**: `{dataDir}/relay-state.json` saves all module state (including hosted vaults)
- **Auto-save**: configurable interval (default 5s), immediate save on shutdown
- **Restores on startup**: portals, webhooks, templates, certs, peers, flagged hashes, revoked tokens, collection CRDT snapshots, hosted vaults

## ACME / SSL
- `GET /.well-known/acme-challenge/:token` — ACME HTTP-01 challenge response
- `POST /api/acme/challenges` — register challenge
- `DELETE /api/acme/challenges/:token` — remove challenge
- `GET/POST/DELETE /api/acme/certificates` — certificate lifecycle

## Portal Templates
- `GET/POST /api/templates` — list/create templates
- `GET/DELETE /api/templates/:id` — get/remove template

## WebRTC Signaling (P2P/SFU Connection Negotiation)
- `GET /api/signaling/rooms` — list active signaling rooms
- `GET /api/signaling/rooms/:roomId/peers` — list peers in a room
- `POST /api/signaling/rooms/:roomId/join` — join a room (returns existing peers)
- `POST /api/signaling/rooms/:roomId/leave` — leave a room (notifies remaining peers)
- `POST /api/signaling/rooms/:roomId/signal` — relay SDP offer/answer/ICE candidate to target peer
- `POST /api/signaling/rooms/:roomId/poll` — poll buffered signals for a peer
- Signal types: `offer`, `answer`, `ice-candidate`, `leave`
- Relay is transport-agnostic: routes opaque payloads between peers

## HTTP API (Core)
- `GET /api/status` — relay state
- `GET /api/modules` — installed modules
- `GET /api/health` — health check (uptime, memory, peer count) for load balancers/Docker
- Webhooks: `GET/POST /api/webhooks`, `DELETE /api/webhooks/:id`, `GET /:id/deliveries`, `POST /:id/test`
- Portals: `GET/POST /api/portals`, `GET/DELETE /api/portals/:id`
- Tokens: `GET /api/tokens` (list), `POST /api/tokens/{issue,verify,revoke}`
- Collections: `GET/POST /api/collections`, `GET /:id/snapshot`, `POST /:id/import`, `DELETE /:id`
- Hashcash: `POST /api/hashcash/{challenge,verify}`
- Trust: `GET /api/trust`, `GET /:did`, `POST /:did/{ban,unban}`
- Escrow: `POST /api/escrow/{deposit,claim}`, `GET /:depositorId`
- Federation: `POST /api/federation/announce`, `GET /peers`, `POST /forward`
- Signaling: `GET /api/signaling/rooms`, `GET /rooms/:id/peers`, `POST /rooms/:id/{join,leave,signal,poll}`
- Backup: `GET/POST /api/backup` — export/import full relay state
- Logs: `GET /api/logs` (with `?level=` and `?limit=` filters), `DELETE /api/logs`

## Deployment Files
- `Dockerfile` — multi-stage Docker build (non-root user, HEALTHCHECK, VOLUME)
- `.dockerignore` — excludes node_modules, dist, tests, legacy packages
- `docker-compose.yml` — single-relay deployment
- `docker-compose.federation.yml` — two-relay federated mesh
- `docker-compose.test.yml` — test-specific (ephemeral ports, profiles: single/persist/dev/federation)
- `.env.example` — environment variable template

## Docs
- [Deployment Guide](docs/deployment.md) — Docker, TLS, federation, monitoring, security
- [Development Guide](docs/development.md) — Architecture, adding modules, testing, contributing
