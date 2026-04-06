# @prism/relay

Runtime server for Prism Relay — wraps Layer 1 relay primitives in HTTP + WebSocket.

## Build & Test
- `pnpm dev` — start with tsx watch (dev mode)
- `pnpm typecheck`
- `pnpm test:e2e` — Playwright E2E tests (32 tests, no browser needed)

## CLI
Three deployment modes:
- `--mode server` — always-on relay (all modules, hashcash=16, JSON logging, no CORS)
- `--mode p2p` — federated peer (minimal modules, federation enabled, hashcash=12)
- `--mode dev` — local development (all modules, hashcash=4, CORS=*, debug logging)

Config priority: CLI flags > env vars > config file > mode defaults.
Identity persists to `~/.prism/relay/identity.json` (auto-created on first run).
See `prism-relay --help` for full options.

## Architecture
- **Hono** HTTP framework with `@hono/node-ws` for WebSocket upgrade
- **Config system**: `relay.config.json` or CLI flags, env var overrides
- **Identity persistence**: Ed25519 JWK export/import via `@prism/core/identity`
- **ConnectionRegistry** tracks WS connections + collection subscriptions for broadcast
- **CORS middleware** configurable per deployment mode
- **Structured logger** (text or JSON format, configurable level)

## Exports
- `@prism/relay/server` — Hono app factory (`createRelayServer`)
- `@prism/relay/protocol` — WebSocket wire protocol types + serialization
- `@prism/relay/config` — Config resolution, arg parsing, logger
- `@prism/relay/cli` — CLI entry point

## Modules (14 total)
blind-mailbox, relay-router, relay-timestamp, blind-pings, capability-tokens, webhooks, sovereign-portals, collection-host, hashcash, peer-trust, escrow, federation, acme-certificates, portal-templates

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
- `GET /portals/:id` — renders portal as HTML
- `GET /portals/:id/snapshot.json` — raw JSON snapshot for API consumers
- `POST /portals/:id/submit` — Level 3+ form submission (creates object with ephemeral DID)

### Portal Levels
- **Level 1**: Static read-only HTML snapshot
- **Level 2**: Live incremental DOM patching via WebSocket (fetches snapshot.json + patches #portal-content)
- **Level 3**: Interactive forms with ephemeral DID auth, capability token verification for non-public portals
- **Level 4**: Full client-side hydration with `window.__PRISM_PORTAL__` API (subscribe/notify, bidirectional CRDT sync, sendUpdate/submitObject)

## ACME / SSL
- `GET /.well-known/acme-challenge/:token` — ACME HTTP-01 challenge response
- `POST /api/acme/challenges` — register challenge
- `DELETE /api/acme/challenges/:token` — remove challenge
- `GET/POST/DELETE /api/acme/certificates` — certificate lifecycle

## Portal Templates
- `GET/POST /api/templates` — list/create templates
- `GET/DELETE /api/templates/:id` — get/remove template

## HTTP API
- `GET /api/status` — relay state
- `GET /api/modules` — installed modules
- Webhooks: `GET/POST /api/webhooks`, `DELETE /api/webhooks/:id`
- Portals: `GET/POST /api/portals`, `GET/DELETE /api/portals/:id`
- Tokens: `POST /api/tokens/{issue,verify,revoke}`
- Collections: `GET/POST /api/collections`, `GET /:id/snapshot`, `POST /:id/import`
- Hashcash: `POST /api/hashcash/{challenge,verify}`
- Trust: `GET /api/trust`, `GET /:did`, `POST /:did/{ban,unban}`
- Escrow: `POST /api/escrow/{deposit,claim}`, `GET /:depositorId`
- Federation: `POST /api/federation/announce`, `GET /peers`, `POST /forward`
- ACME: `POST /api/acme/challenges`, `GET/POST/DELETE /api/acme/certificates`
- Templates: `GET/POST /api/templates`, `GET/DELETE /api/templates/:id`
