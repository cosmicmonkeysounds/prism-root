# @prism/relay

Runtime server for Prism Relay — wraps Layer 1 relay primitives in HTTP + WebSocket.

## Build
- `pnpm dev` — start with tsx watch
- `pnpm typecheck`

## Architecture
- **Hono** HTTP framework with `@hono/node-ws` for WebSocket upgrade
- Imports all relay primitives from `@prism/core/relay`
- Identity from `@prism/core/identity`
- Zero persistence — ephemeral in-memory state (matching Layer 1 primitives)
- **ConnectionRegistry** tracks WS connections + collection subscriptions for broadcast

## Exports
- `@prism/relay/server` — Hono app factory (`createRelayServer`)
- `@prism/relay/protocol` — WebSocket wire protocol types + serialization
- `@prism/relay/cli` — CLI entry point

## Modules (12 total)
blind-mailbox, relay-router, relay-timestamp, blind-pings, capability-tokens, webhooks, sovereign-portals, collection-host, hashcash, peer-trust, escrow, federation

## Protocol
WebSocket at `/ws/relay`:
1. Client sends `{ type: "auth", did }` → Relay replies `{ type: "auth-ok" }`
2. Client sends `{ type: "envelope", envelope }` → Relay routes + replies `{ type: "route-result" }`
3. Relay pushes `{ type: "envelope" }` for inbound envelopes
4. `{ type: "sync-request", collectionId }` → `{ type: "sync-snapshot", collectionId, snapshot }`
5. `{ type: "sync-update", collectionId, update }` → broadcast to subscribers
6. `{ type: "hashcash-proof", proof }` → `{ type: "hashcash-ok" }` or error
7. `{ type: "ping" }` → `{ type: "pong" }`

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
