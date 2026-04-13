# relay/

Composable Prism Relay runtime. A Relay is a modular zero-knowledge bridge —
not a monolithic server. Users mix-and-match Web 1/2/3 features by chaining
modules onto a builder; capabilities are shared via a `RelayContext` and
dependencies are validated at `.build()` time.

```ts
import { createRelayBuilder } from "@prism/core/relay";
```

## Key exports

### Builder & core

- `createRelayBuilder(options)` — builder with `.use(module)`, `.configure()`,
  and `.build()` returning a `RelayInstance` (`start`/`stop`/`getCapability`).
- `RelayModule` / `RelayContext` / `RelayConfig` / `RelayInstance` /
  `RelayBuilder` / `RelayBuilderOptions` — types for writing custom modules.
- `RELAY_CAPABILITIES` — well-known capability name registry (mailbox,
  router, timestamper, pinger, tokens, webhooks, portals, collections,
  hashcash, trust, escrow, federation, acme, templates, signaling, ...).
- `createRelayClient(options)` — client SDK that speaks the Relay wire
  protocol over WebSocket. Emits `connected`/`disconnected`/`error`/
  `state-change` events.
- `createMemoryPingTransport()` — in-memory `PingTransport` for tests.

### Built-in modules

Phase 1 (core wire protocol):

- `blindMailboxModule()` — E2EE store-and-forward queue for offline peers.
- `relayRouterModule()` — zero-knowledge envelope router (depends on mailbox).
- `relayTimestampModule(identity)` — signed cryptographic timestamp receipts.
- `blindPingModule()` — push notification fan-out.
- `capabilityTokenModule(identity)` — scoped, signed access tokens.
- `webhookModule(httpClient?)` — outgoing HTTP on CRDT / envelope events.
- `sovereignPortalModule()` — portal levels 1–4 (static to full app).
- `webrtcSignalingModule()` — P2P / SFU connection negotiation.

Phase 2 (federation & hosting):

- `collectionHostModule()` — host CRDT collections on the relay.
- `vaultHostModule()` — host and serve whole vaults.
- `hashcashModule(options?)` — SHA-256 proof-of-work spam gate.
- `peerTrustModule()` — trust / distrust / ban graph for federated peers.
- `escrowModule()` — deposit / claim / evict escrow lifecycle.
- `federationModule()` — forward envelopes to peer relays.
- `passwordAuthModule(...)` — password-based auth manager.
- `acmeCertificateModule()` — ACME / Let's Encrypt SSL cert management.
- `portalTemplateModule()` — portal template registry.

### Portal renderer

- `extractPortalSnapshot`, `renderPortalHtml`, `escapeHtml` — pure helpers
  that turn a `PortalSnapshot` into dependency-free HTML.

## Usage

```ts
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  relayTimestampModule,
  webhookModule,
  sovereignPortalModule,
} from "@prism/core/relay";

const relay = createRelayBuilder({ relayDid: identity.did })
  .use(blindMailboxModule())
  .use(relayRouterModule())
  .use(relayTimestampModule(identity))
  .use(webhookModule())
  .use(sovereignPortalModule())
  .build();

await relay.start();
// relay.getCapability(RELAY_CAPABILITIES.MAILBOX) etc.
```

Dependencies are declared per-module and checked at `.build()` time — e.g.
`relayRouterModule` requires `blindMailboxModule` to have been installed
first. Order-of-use is preserved for both install and `start`, and modules
are stopped in reverse order.
