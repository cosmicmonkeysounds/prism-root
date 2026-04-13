# discovery/

Vault Discovery. Two layers: a persistent roster of known vaults with CRUD,
sort, pin, search, and dedup; and a filesystem scanner that walks search
paths looking for `.prism.json` manifests and merges findings back into the
roster. Both layers are I/O-free at the core — a `RosterStore` handles
persistence and a `DiscoveryAdapter` abstracts filesystem access so
Tauri/Node/test environments can plug in.

```ts
import { createVaultRoster, createVaultDiscovery } from "@prism/core/discovery";
```

## Key exports

### Roster

- `createVaultRoster(store?)` — persistent registry of known vaults.
- `createMemoryRosterStore()` — in-memory `RosterStore` for tests.
- `VaultRoster` — `add`/`remove`/`get`/`getByPath`/`update`/`touch`/`pin`/
  `list`/`size`/`all`/`onChange`/`save`/`reload`.
- `RosterStore` — persistence interface (`load`/`save`).
- `RosterEntry` / `RosterListOptions` / `RosterSortField` / `RosterSortDir`
  — entry shape and list/query options.
- `RosterChange` / `RosterChangeType` / `RosterChangeHandler` — events.

### Discovery

- `createVaultDiscovery(adapter, roster?)` — scans paths for manifests,
  emits events, and optionally merges into a roster.
- `createMemoryDiscoveryAdapter()` — in-memory `DiscoveryAdapter` with
  `addDirectory`/`addFile` helpers for tests.
- `VaultDiscovery` — `scan(options)`, `scanning`, `lastScanAt`,
  `lastScanCount`, `onEvent`.
- `DiscoveryAdapter` / `MemoryDiscoveryAdapter` — filesystem abstraction.
- `DiscoveryScanOptions` — `{ searchPaths, maxDepth?, mergeToRoster? }`.
- `DiscoveredVault` / `DiscoveryEvent` / `DiscoveryEventType` /
  `DiscoveryEventHandler` — result and event types.

## Usage

```ts
import {
  createVaultRoster,
  createVaultDiscovery,
  createMemoryRosterStore,
  createMemoryDiscoveryAdapter,
} from "@prism/core/discovery";

const roster = createVaultRoster(createMemoryRosterStore());
const adapter = createMemoryDiscoveryAdapter();
const discovery = createVaultDiscovery(adapter, roster);

discovery.onEvent((event) => {
  if (event.type === "vault-found") console.log("found", event.vault.path);
});

const found = discovery.scan({
  searchPaths: ["/Users/alice/Vaults"],
  maxDepth: 2,
});
```

With `mergeToRoster` enabled (default), each discovered vault is added
to the roster automatically and persisted through the backing store.
