# persistence

Durable storage for GraphObjects and ObjectEdges. `createCollectionStore` wraps a `LoroDoc` holding two top-level maps (`objects` and `edges`) and exposes CRUD, filtered queries, snapshot import/export, and change subscriptions. `createVaultManager` orchestrates a `PrismManifest`'s collections against a `PersistenceAdapter`, lazy-loading stores on first access and tracking dirty state.

```ts
import {
  createCollectionStore,
  createVaultManager,
  createMemoryAdapter,
} from '@prism/core/persistence';
```

## Key exports

- `createCollectionStore(options?)` — build a Loro-backed `CollectionStore` with `putObject`/`getObject`/`listObjects(filter?)`, `putEdge`/`listEdges(filter?)`, `exportSnapshot`/`import`, and `onChange(handler)`.
- `CollectionStore`, `CollectionStoreOptions`, `ObjectFilter` — `ObjectFilter` supports `types`, `tags`, `statuses`, `parentId`, `excludeDeleted`.
- `CollectionChange`, `CollectionChangeType`, `CollectionChangeHandler` — change subscription types (`object-put` / `object-remove` / `edge-put` / `edge-remove`).
- `createVaultManager(manifest, adapter, options?)` — manifest-driven lifecycle: `openCollection(id)`, `saveCollection(id)`, `saveAll()`, `closeCollection(id)`, `isDirty(id)`, `openCollections()`.
- `VaultManager`, `VaultManagerOptions`.
- `PersistenceAdapter` — pluggable I/O (`load`/`save`/`delete`/`exists`/`list`).
- `createMemoryAdapter()` — in-memory adapter for testing and ephemeral vaults.

## Usage

```ts
import {
  createVaultManager,
  createMemoryAdapter,
} from '@prism/core/persistence';

const adapter = createMemoryAdapter();
const vault = createVaultManager(manifest, adapter);

const notes = vault.openCollection('notes');
notes.putObject({ id: 'n1', type: 'note', name: 'Hello', /* ... */ } as any);

vault.saveAll();
```
