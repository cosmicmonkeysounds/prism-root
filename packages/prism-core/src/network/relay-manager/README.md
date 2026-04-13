# relay-manager/

Client-side connection manager for Prism Relays. Prism apps (Studio, Flux,
Lattice, Musica) are client-only SPAs — they never run server code. The
RelayManager is how a client tracks configured relay endpoints, opens
WebSocket sessions via the `@prism/core/relay` `RelayClient` SDK, and drives
portal / collection / federation / webhook / cert management over the
relay's HTTP API. HTTP and WebSocket clients are injectable for testing.

```ts
import { createRelayManager } from "@prism/core/relay-manager";
```

Note: `@prism/core/relay-manager` is the client-facing counterpart to the
`@prism/core/relay` builder runtime. It is not listed in
`packages/prism-core/CLAUDE.md`, but the subpath is exported from
`packages/prism-core/package.json`.

## Key exports

- `createRelayManager(options?)` — factory returning a `RelayManager`.
- `RelayManager` — full client API: `addRelay`/`removeRelay`/`listRelays`/
  `getRelay`, `connect`/`disconnect`, `publishPortal`/`unpublishPortal`/
  `listPortals`, `listCollections`/`inspectCollection`/`deleteCollection`/
  `syncCollection`, `listWebhooks`/`deleteWebhook`, `listPeers`/`banPeer`/
  `unbanPeer`/`getTrustGraph`, `listCertificates`, `backupRelay`/
  `restoreRelay`, `fetchStatus`/`fetchHealth`, `discoverRelay`, `subscribe`,
  `dispose`.
- `RelayEntry` — configured endpoint (`id`, `name`, `url`, `status`,
  `error`, `modules`, `relayDid`).
- `RelayConnectionStatus` — `"disconnected" | "connecting" | "connected" | "error"`.
- `RelayStatus` — response shape from a relay's `/status` HTTP endpoint.
- `PublishPortalOptions` / `DeployedPortal` — portal publish payload and
  result (includes `manifest`, `relayId`, `viewUrl`).
- `RelayHttpClient` — injectable `fetch` abstraction.
- `RelayManagerOptions` — `{ httpClient?, createWsClient? }`.

## Usage

```ts
import { createRelayManager } from "@prism/core/relay-manager";

const manager = createRelayManager();
const entry = manager.addRelay("Production", "https://relay.example.com");

await manager.connect(entry.id, identity);

const portal = await manager.publishPortal({
  relayId: entry.id,
  collectionId: "notes",
  name: "My Notes",
  level: 2,
});

const unsubscribe = manager.subscribe(() => {
  console.log(manager.listRelays());
});
```

Connecting emits `connected` / `disconnected` / `error` / `state-change`
events internally and mirrors them into `RelayEntry.status`, `modules`, and
`relayDid`. Auto-reconnect is enabled by default (3 s delay, 5 attempts).
Call `dispose()` on teardown to close all WebSockets and clear listeners.
