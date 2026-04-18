# prism-relay

Rust-native relay server — **Sovereign Portal** host + full 18-module
relay protocol. Built on `axum` + `tower` + `tokio`, it serves ~80 HTTP
endpoints, a WebSocket relay protocol, and SSR portals through the same
`prism-builder` component registry the Studio uses.

> **Status:** Full 18-module feature surface ported from the legacy Hono
> JSX relay (2026-04-18). The composable module system lives in
> `prism-core::network::relay`; the HTTP/WS surface lives here.

## Build & Test
- `cargo build -p prism-relay` — lib + `prism-relayd` bin.
- `cargo run -p prism-relay --bin prism-relayd -- --bind 127.0.0.1:1420 --mode dev`
  — start the server with all 18 modules.
- `cargo test -p prism-relay` — 21 unit tests + 8 integration tests.
- `cargo clippy -p prism-relay -- -D warnings` — zero warnings.
- Also reachable via: `cargo run -p prism-cli -- dev relay` and
  `cargo run -p prism-cli -- build --target relay`.

## Architecture

```
┌──────────────────────────────────────────────────────┐
│  FullRelayState                                      │
│  ├── RelayInstance (18 modules via RelayBuilder)      │
│  ├── RelayConfig (mode/env/CLI)                      │
│  ├── RequestMetrics (Prometheus)                     │
│  └── RateLimiter (token-bucket per IP)               │
└──────────────────────────────────────────────────────┘
                    │
                    ▼
        build_full_router(Arc<FullRelayState>)
                    │
         ┌──────────┼──────────┐
         ▼          ▼          ▼
    /api/*      /ws        SSR pages
   (~80 routes)  (relay     (portals,
                 protocol)   sitemap,
                             robots)
```

Two routers coexist:
- `build_full_router` — the complete relay (API + WS + admin + metrics + ACME).
  Used by `prism-relayd` in production.
- `build_router` — the original SSR-only router (portals + SEO).
  Used by integration tests and any consumer that only needs L1 portals.

### Modules (server-side)

| Module | Description |
|---|---|
| `config` | `RelayConfig` with `RelayMode` (Server/P2p/Dev), env var overrides, mode-specific defaults |
| `middleware` | CSRF (`X-Prism-CSRF: 1`), token-bucket rate limiting, body size limit, request metrics |
| `persistence` | `RelayState` + `FileStore` — JSON file-based state persistence |
| `relay_state` | `FullRelayState` — wires all 17 `prism-core` relay modules via `RelayBuilder`, typed capability accessors |
| `router` | `build_full_router` — wires all API routes + WS + middleware into a single `axum::Router` |
| `routes/` | 25 route modules covering the full API surface |
| `ws` | WebSocket relay protocol (auth, envelope, collect, ping, sync, hashcash, presence) |
| `ssr_routes` | Original SSR portal router (`build_router`) |

## API Routes

### Status & Admin
| Method | Path | Handler |
|---|---|---|
| GET | `/api/status` | Relay status + DID + module list |
| GET | `/api/modules` | Module enumeration |
| GET | `/api/health` | Health check with uptime + peer count |
| GET | `/admin` | Admin dashboard (HTML) |
| GET | `/admin/api/snapshot` | Admin metrics snapshot |
| GET | `/metrics` | Prometheus text format |

### Portals & Collections
| Method | Path | Handler |
|---|---|---|
| GET/POST | `/api/portals` | List / create portals |
| GET/DELETE | `/api/portals/{id}` | Get / delete portal |
| GET | `/api/portals/{id}/export` | Export portal + snapshot |
| GET/POST | `/api/collections` | List / create collections |
| GET/POST | `/api/collections/{id}/snapshot` | Export / import CRDT snapshot |
| DELETE | `/api/collections/{id}` | Delete collection |
| GET/POST/PUT/DELETE | `/api/rest/{collection_id}[/{object_id}]` | AutoREST gateway |

### Auth
| Method | Path | Handler |
|---|---|---|
| POST | `/api/auth/password/register` | PBKDF2-SHA256 registration |
| POST | `/api/auth/password/login` | Password login |
| POST | `/api/auth/password/change` | Change password |
| GET/DELETE | `/api/auth/password/user/{username}` | Get / delete user |
| GET | `/api/auth/providers` | OAuth provider list (stub) |

### Webhooks & Tokens
| Method | Path | Handler |
|---|---|---|
| GET/POST | `/api/webhooks` | List / create webhooks |
| DELETE | `/api/webhooks/{id}` | Delete webhook |
| GET | `/api/webhooks/{id}/deliveries` | Delivery history |
| POST | `/api/webhooks/{id}/test` | Test fire |
| GET | `/api/tokens` | List capability tokens |
| POST | `/api/tokens/issue` | Issue token |
| POST | `/api/tokens/verify` | Verify token |
| POST | `/api/tokens/revoke` | Revoke token |

### Trust & Safety
| Method | Path | Handler |
|---|---|---|
| GET | `/api/trust` | List peer reputations |
| GET | `/api/trust/{did}` | Get peer trust score |
| POST | `/api/trust/{did}/ban` | Ban peer |
| POST | `/api/trust/{did}/unban` | Unban peer |
| POST | `/api/safety/report` | Report content |
| GET | `/api/safety/hashes` | List flagged hashes |
| POST | `/api/safety/hashes/import` | Import flagged hashes |
| POST | `/api/safety/hashes/check` | Check hashes |
| POST | `/api/safety/hashes/gossip` | Gossip flagged hashes |

### Federation & Escrow
| Method | Path | Handler |
|---|---|---|
| POST | `/api/federation/announce` | Announce relay |
| GET | `/api/federation/peers` | List federation peers |
| POST | `/api/federation/forward` | Forward envelope |
| POST | `/api/federation/sync` | Receive sync |
| POST | `/api/escrow/deposit` | Create escrow deposit |
| POST | `/api/escrow/claim` | Claim deposit |
| GET | `/api/escrow/{depositor_id}` | List deposits |

### Signaling & Presence
| Method | Path | Handler |
|---|---|---|
| GET | `/api/signaling/rooms` | List WebRTC rooms |
| GET | `/api/signaling/rooms/{room_id}/peers` | Room peer list |
| POST | `/api/signaling/rooms/{room_id}/join` | Join room |
| POST | `/api/signaling/rooms/{room_id}/leave` | Leave room |
| POST | `/api/signaling/rooms/{room_id}/signal` | Relay signal |
| GET | `/api/presence` | Presence snapshot |

### Vaults & Templates
| Method | Path | Handler |
|---|---|---|
| GET/POST | `/api/vaults` | List / publish vaults |
| GET/DELETE | `/api/vaults/{id}` | Get / delete vault |
| GET/PUT | `/api/vaults/{id}/collections` | List / update vault collections |
| GET | `/api/vaults/{id}/collections/{coll_id}` | Get collection snapshot |
| GET | `/api/vaults/{id}/download` | Download full vault |
| GET/POST | `/api/templates` | List / create templates |
| GET/DELETE | `/api/templates/{id}` | Get / delete template |

### Infrastructure
| Method | Path | Handler |
|---|---|---|
| GET | `/.well-known/acme-challenge/{token}` | ACME challenge response |
| POST/DELETE | `/api/acme/challenges[/{token}]` | Manage ACME challenges |
| GET/POST | `/api/acme/certificates` | List / add certificates |
| GET | `/api/acme/certificates/{domain}` | Get certificate |
| POST | `/api/hashcash/challenge` | Create hashcash challenge |
| POST | `/api/hashcash/verify` | Verify proof |
| GET/POST | `/api/backup` | Export / import full state |
| GET/DELETE | `/api/logs` | Query / clear logs |
| GET | `/api/email/status` | Email transport status |
| POST | `/api/email/send` | Send email (stub) |
| GET | `/api/directory` | Relay directory feed |
| GET | `/api/pings/devices` | List push devices |
| POST | `/api/pings/register` | Register push device |
| POST | `/api/pings/send` | Send push ping |
| POST | `/api/pings/wake` | Wake device |

### WebSocket (`/ws`)
Wire protocol: `auth` → `auth-ok`, `envelope` → `route-result`,
`collect` → inbound envelopes, `ping` → `pong`, `sync-request` →
`sync-snapshot`, `sync-update`, `hashcash-proof` → `hashcash-ok`,
`presence-update`.

### SSR Pages (via `build_router`)
| Method | Path | Description |
|---|---|---|
| GET | `/healthz` | Liveness probe |
| GET | `/` or `/portals` | Portal index |
| GET | `/portals/{id}` | Render portal as HTML |
| GET | `/sitemap.xml` | XML sitemap |
| GET | `/robots.txt` | Robots directives |

## Middleware Stack
- **CSRF** — `X-Prism-CSRF: 1` required on POST/PUT/DELETE to `/api/*`
  (exempts ACME challenges, admin, metrics).
- **Body limit** — rejects `Content-Length` > 1MB.
- **Rate limiting** — token-bucket per IP (100 burst, 20/s refill, 10k entry LRU).
- **Metrics** — atomic request counter + per-route status histogram, Prometheus `/metrics`.

## Tests
- **Unit** — 21 tests: portal (4), components (7), state (2), SSR routes (6),
  relay_state (2: module construction + capability accessor coverage).
- **Integration** — 8 tests in `tests/routes.rs` via `tower::ServiceExt::oneshot`.
- All tests pass with zero clippy warnings under `-D warnings`.

## Migration notes
- The old folder contained ~30k lines of TypeScript. Every file was deleted
  as part of the Rust rewrite; `pnpm-workspace.yaml` no longer lists
  `packages/prism-relay`.
- Port 1420 / `prism-relayd` binary name carry over for downstream tooling.
- `prism test` is Rust-only (`cargo test --workspace`).
