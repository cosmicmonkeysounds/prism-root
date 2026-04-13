# presence/

Ephemeral peer awareness for real-time collaboration. Presence state is
RAM-only — nothing is ever persisted to the Loro CRDT. The manager tracks
each peer's cursor position, selection ranges, active view, and arbitrary
metadata, emits `joined`/`updated`/`left` events, and evicts stale peers on
a TTL sweep. Timers are injectable so tests can run deterministically.

```ts
import { createPresenceManager } from "@prism/core/presence";
```

## Key exports

- `createPresenceManager(options)` — factory returning a `PresenceManager`.
- `PresenceManager` — interface: `local`, `get(peerId)`, `getPeers()`,
  `getAll()`, `setCursor`, `setSelections`, `setActiveView`, `setData`,
  `updateLocal`, `receiveRemote`, `removePeer`, `subscribe`, `sweep`,
  `dispose`.
- `PresenceManagerOptions` — `{ localIdentity, ttlMs?, sweepIntervalMs?, timers? }`.
- `PeerIdentity` — `{ peerId, displayName, color, avatarUrl? }`.
- `PresenceState` — `{ identity, cursor, selections, activeView, lastSeen, data }`.
- `CursorPosition` / `SelectionRange` — field-level cursor and selection shapes.
- `PresenceChange` / `PresenceChangeType` / `PresenceListener` — event types.
- `TimerProvider` — injectable `now` / `setInterval` / `clearInterval` for tests.

## Usage

```ts
import { createPresenceManager } from "@prism/core/presence";

const presence = createPresenceManager({
  localIdentity: {
    peerId: "did:key:alice",
    displayName: "Alice",
    color: "#f97316",
  },
});

presence.setCursor({ objectId: "note-42", field: "body", offset: 128 });

const off = presence.subscribe((change) => {
  if (change.type === "joined") console.log("peer joined", change.peerId);
});

// Feed remote awareness messages from your transport:
presence.receiveRemote(remoteState);
```

Defaults: `ttlMs` 30 000, `sweepIntervalMs` 5 000. Call `dispose()` to stop
the sweep timer and clear remote peers on teardown.
