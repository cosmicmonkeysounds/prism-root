# Prism Relay Development Guide

Developer guide for contributing to `@prism/relay`.

## Architecture Overview

- **Hono** HTTP framework (3 KB core) with `@hono/node-ws` for WebSocket upgrade
- **15 composable modules** via builder pattern -- each module is a self-contained capability
- **Config system**: file > env > CLI with 3 mode presets (server, p2p, dev)
- **File persistence** for module state (JSON, auto-saved every 5s)
- **ConnectionRegistry** for WebSocket connection tracking and broadcast
- **Security middleware**: CSRF, body size limits, banned peer rejection, rate limiting

## Local Development Setup

```bash
cd packages/prism-relay
pnpm dev  # tsx watch, dev mode
```

Dev mode defaults:
- All 15 modules enabled
- Hashcash bits = 4 (fast proof-of-work)
- CORS = `*` (open)
- CSRF disabled
- Log level = debug, format = text

The relay starts at `http://localhost:4444` and generates an identity at `~/.prism/relay/identity.json` on first run.

## Project Structure

```
src/
  cli.ts                    CLI entry point, all subcommands
  index.ts                  Package exports
  config/
    relay-config.ts         Config types + resolution (4-layer merge)
    parse-args.ts           CLI argument parser
    logger.ts               Structured text/JSON logger
  server/
    relay-server.ts         Hono app factory, middleware stack, WebSocket
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
  middleware/
    csrf.ts                 CSRF protection (X-Prism-CSRF header)
    body-size.ts            Content-Length enforcement
    banned-peer.ts          X-Prism-DID peer rejection
  protocol/
    relay-protocol.ts       Wire format types + serialization
  persistence/
    file-store.ts           JSON file store for relay state
e2e/
  relay.spec.ts             Core E2E tests (87 tests)
  production-readiness.spec.ts  Security + resilience tests (48 tests)
docs/
  deployment.md             Production deployment guide
  development.md            This file
```

## Adding a New Module

1. **Define the module** in `@prism/core/relay` (`relay-types.ts`, `relay.ts`) -- add the capability type and module interface
2. **Create a route file** in `src/routes/` following the route handler pattern below
3. **Export** from `src/routes/index.ts`
4. **Wire** into `src/server/relay-server.ts` -- mount the sub-app on a path
5. **Add to module factories** in `src/cli.ts` `createModules()` function
6. **Add to `ALL_MODULES`** in `config/relay-config.ts` and update mode defaults
7. **Add persistence** in `src/persistence/file-store.ts` if the module has state
8. **Write unit tests** (`src/routes/your-module.test.ts`)

## Route Handler Pattern

Each module's HTTP surface is a standalone Hono sub-app that receives the relay instance:

```typescript
import { Hono } from "hono";
import type { RelayInstance } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { XyzModule } from "@prism/core/relay";

export function createXyzRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function getModule(): XyzModule | undefined {
    return relay.getCapability<XyzModule>(RELAY_CAPABILITIES.XYZ);
  }

  // Guard: return 404 if the module isn't installed
  app.use("/*", async (c, next) => {
    if (!getModule()) return c.json({ error: "xyz module not installed" }, 404);
    await next();
  });

  app.get("/", (c) => c.json(getModule()!.list()));

  app.post("/", async (c) => {
    const body = await c.req.json();
    const result = getModule()!.create(body);
    return c.json(result, 201);
  });

  return app;
}
```

Key conventions:
- The `getModule()` helper fetches the capability from the relay instance
- A wildcard middleware guards all routes -- if the module isn't installed, every endpoint returns 404
- Route files are pure functions (no global state)
- Return appropriate HTTP status codes (201 for creation, 404 for not found, etc.)

## Testing

### Unit Tests (Vitest)

```bash
# Run all unit tests
npx vitest run

# Watch mode
npx vitest

# Run a specific test file
npx vitest run src/routes/portal-routes.test.ts
```

Unit test pattern: create a relay with modules via the builder, then use Hono's `app.request()` for HTTP testing without starting a real server:

```typescript
import { describe, it, expect } from "vitest";
import { createRelayServer } from "../server/relay-server.js";

describe("xyz routes", () => {
  it("should list items", async () => {
    const { app } = await createRelayServer({
      mode: "dev",
      modules: ["xyz"],
      // ... minimal config
    });

    const res = await app.request("/api/xyz");
    expect(res.status).toBe(200);

    const body = await res.json();
    expect(body).toEqual([]);
  });
});
```

### E2E Tests (Playwright)

```bash
# Run E2E tests (no browser needed -- these test HTTP/WS)
pnpm --filter @prism/relay test:e2e
```

E2E tests start a real relay server and exercise the full stack including WebSocket connections, federation, and persistence.

### Type Checking

```bash
pnpm --filter @prism/relay typecheck
```

## WebSocket Protocol

Connect to `ws[s]://host:port/ws/relay`. All messages are JSON.

### Client to Relay

| Type | Description | Payload |
|------|-------------|---------|
| `auth` | Authenticate (required first message) | `{ did }` |
| `envelope` | Send encrypted envelope | `{ envelope: { id, from, to, ciphertext, submittedAt, ttlMs } }` |
| `sync-request` | Request collection snapshot (subscribes to updates) | `{ collectionId }` |
| `sync-update` | Push CRDT update to collection | `{ collectionId, update }` |
| `hashcash-proof` | Submit proof-of-work solution | `{ proof: { challenge, counter, hash } }` |
| `ping` | Heartbeat | (empty) |
| `presence-update` | Update presence state | `{ status, metadata }` |

### Relay to Client

| Type | Description | Payload |
|------|-------------|---------|
| `auth-ok` | Authentication success | `{ relayDid, modules }` |
| `envelope` | Inbound envelope from another peer | `{ envelope: { ... } }` |
| `route-result` | Envelope delivery status | `{ result: { status: "delivered" \| "queued" } }` |
| `sync-snapshot` | Collection snapshot | `{ collectionId, snapshot }` |
| `sync-update` | Collection update broadcast | `{ collectionId, update }` |
| `hashcash-challenge` | Proof-of-work challenge | `{ challenge: { resource, bits, salt, issuedAt } }` |
| `hashcash-ok` | Proof-of-work verified | (empty) |
| `error` | Error message | `{ message }` |
| `pong` | Heartbeat response | (empty) |
| `presence-broadcast` | Presence update from another peer | `{ did, status, metadata }` |

### Connection Lifecycle

1. Client opens WebSocket to `/ws/relay`
2. Client sends `{ type: "auth", did: "did:key:z6Mk..." }`
3. If hashcash is enabled, relay sends `{ type: "hashcash-challenge" }` -- client must solve and respond with `{ type: "hashcash-proof" }`
4. Relay responds with `{ type: "auth-ok", relayDid, modules }`
5. Client can now send envelopes, sync collections, update presence
6. Relay pushes inbound envelopes and sync updates
7. Periodic `ping`/`pong` for keepalive

## Configuration Internals

### 4-Layer Resolution

Config is merged in this order (later layers override earlier):

1. **Mode defaults** -- each mode (server, p2p, dev) has a preset configuration
2. **Config file** -- `relay.config.json` in the working directory or `--config` path
3. **Environment variables** -- `PRISM_RELAY_*` prefixed
4. **CLI flags** -- highest priority, always wins

### Mode Presets

Mode presets auto-configure:
- Which modules are enabled
- Hashcash difficulty (bits)
- CORS origins
- CSRF enforcement
- Log level and format
- Federation defaults

See `src/config/relay-config.ts` for the full preset definitions.

## CLI Commands Reference

### Server Lifecycle

```bash
prism-relay start                         # Start relay (default command)
prism-relay start --mode server           # Start in server mode
prism-relay start --mode p2p              # Start in P2P mode
prism-relay start --mode dev              # Start in dev mode
```

### Configuration

```bash
prism-relay init --mode server -o config.json   # Generate starter config
prism-relay config validate -c config.json      # Validate config file
prism-relay config show --mode server           # Show resolved config
```

### Identity

```bash
prism-relay identity show                 # Display DID and public key
prism-relay identity regenerate           # Generate new identity (backs up old)
```

### Monitoring

```bash
prism-relay status --port 4444            # Check health of running relay
prism-relay logs --level error --follow   # Tail relay logs
```

### Module Discovery

```bash
prism-relay modules list                  # List all 15 modules with descriptions
```

### Peer Management

```bash
prism-relay peers list                    # List federation peers
prism-relay peers ban <did>               # Ban a peer
prism-relay peers unban <did>             # Unban a peer
```

### Data Management

```bash
prism-relay collections list              # List hosted collections
prism-relay portals list                  # List published portals
prism-relay webhooks list                 # List registered webhooks
prism-relay tokens list                   # List active tokens
prism-relay certs list                    # List ACME certificates
```

### Backup & Restore

```bash
prism-relay backup --output state.json    # Export relay state
prism-relay restore --input state.json    # Import relay state
```

### Help

```bash
prism-relay --help                        # Show all options
prism-relay --version                     # Show version
```

All management commands accept `--port` and `--host` to target a specific relay (default: `localhost:4444`).

## Contributing

1. **Fork and create a feature branch** from `main`
2. **Write tests first** (TDD preferred) -- unit tests with Vitest, E2E with Playwright
3. **Follow existing patterns** -- route handler factory, module capability pattern, config resolution
4. **Run checks before submitting**:
   ```bash
   pnpm --filter @prism/relay typecheck
   npx vitest run
   pnpm --filter @prism/relay test:e2e
   ```
5. **Conventional commits**: `feat(relay): add xyz module`, `fix(relay): handle edge case`
6. **No `any` types** -- TypeScript strict mode is enforced
7. **Use `@prism/*` path aliases** for cross-package imports -- never relative paths across packages
8. **kebab-case** for file names
