# vfs

Virtual File System — content-addressed blob storage for binary assets (images, audio, video, PDFs). Blobs are keyed by SHA-256 hash so identical content is automatically deduplicated. `GraphObject.data` references blobs via `BinaryRef` rather than embedding bytes in the CRDT document. Non-mergeable files use the Binary Forking Protocol: acquire an exclusive lock, edit, then `replaceLockedFile` to fork a new hash.

```ts
import { createVfsManager } from '@prism/core/vfs';
```

## Key exports

- `createVfsManager(options?)` — build a `VfsManager` over a pluggable `VfsAdapter` (defaults to the in-memory adapter).
- `createMemoryVfsAdapter()` — in-memory `VfsAdapter` implementation for tests and ephemeral vaults.
- `computeBinaryHash(data)` — SHA-256 hex hash of a `Uint8Array`.
- `VfsManager` — `importFile`, `exportFile`, `removeFile`, `acquireLock`, `releaseLock`, `getLock`, `isLocked`, `listLocks`, `replaceLockedFile`, `stat`, `dispose`.
- `VfsAdapter` — storage I/O interface: `read`, `write`, `stat`, `list`, `delete`, `has`, `count`, `totalSize` (all async).
- `BinaryRef` — `{ hash, filename, mimeType, size, importedAt }` stored in GraphObject.data.
- `FileStat` — blob metadata returned by `stat`.
- `BinaryLock` — `{ hash, lockedBy, lockedAt, reason? }` for the Binary Forking Protocol.
- `VfsManagerOptions` — `{ adapter? }`.

## Usage

```ts
import { createVfsManager } from '@prism/core/vfs';

const vfs = createVfsManager();

const ref = await vfs.importFile(pngBytes, 'photo.png', 'image/png');
// store `ref` on a GraphObject.data field

const bytes = await vfs.exportFile(ref);

vfs.acquireLock(ref.hash, 'did:key:z6Mk...', 'editing in Photopea');
const forked = await vfs.replaceLockedFile(
  ref.hash, editedBytes, 'photo.png', 'image/png', 'did:key:z6Mk...',
);
```
