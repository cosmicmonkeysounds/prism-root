# network/

Everything that crosses the wire. The `network/` category contains Prism's
transport-facing subsystems: composable relay runtimes, ephemeral presence,
real-time communication sessions, vault discovery, and framework-agnostic
server route generation. All modules respect the Prism invariants — Loro CRDT
is the source of truth, ephemeral state (cursors, presence) is RAM-only, and
any relay server is zero-knowledge.

## Subsystems

- [`relay/`](./relay/README.md) — `@prism/core/relay`. Composable relay
  runtime built with `createRelayBuilder().use(...)` chaining. Mix-and-match
  Web 1/2/3 modules (blind mailbox, router, timestamps, pings, capability
  tokens, webhooks, sovereign portals, WebRTC signaling, and more).
- [`relay-manager/`](./relay-manager/README.md) — Client-side connection
  manager. Studio is client-only; `createRelayManager` handles relay CRUD,
  WebSocket connect/disconnect via the RelayClient SDK, and HTTP portal /
  collection / federation / webhook management against deployed relays.
- [`presence/`](./presence/README.md) — `@prism/core/presence`. RAM-only peer
  awareness (cursors, selections, active view) with TTL-based sweep eviction.
- [`session/`](./session/README.md) — `@prism/core/session`. Communication
  Fabric: session lifecycle, participants, transcript timeline,
  hypermedia-synced playback, pluggable transports and transcription
  providers.
- [`discovery/`](./discovery/README.md) — `@prism/core/discovery`. Vault
  Discovery: persistent roster of known vaults plus filesystem scanning for
  `.prism.json` manifests.
- [`server/`](./server/README.md) — `@prism/core/server`. Framework-agnostic
  Server Factory: generate REST route specs and OpenAPI 3.1 documents from an
  `ObjectRegistry`.

## Import rules

- Inside prism-core, cross-category imports go through `@prism/core/<subsystem>`
  path aliases. Only same-category siblings may use relative paths.
- `network/` may import from `foundation/`, `language/`, and `identity/`.
- `network/` and `kernel/` may reference each other's types (e.g. `actor`
  consumes relay capability types) but must not reach down into
  `interaction/` or `bindings/`.
