# foundation

Pure data primitives with no external concerns. The lowest layer of `@prism/core` — no React, no DOM, no network, no dependencies on any other category. Everything above (`language`, `kernel`, `network`, `interaction`, `domain`, `bindings`) builds on these.

Loro CRDT is the hidden buffer beneath most of these modules; Zustand stores in `crdt-stores` project Loro state into React land.

## Subsystems

- [object-model/](./object-model/README.md) — `@prism/core/object-model` — GraphObject, ObjectRegistry, TreeModel, EdgeModel, WeakRefEngine, ContextEngine, NSID/PrismAddress, query helpers, string utilities.
- [persistence/](./persistence/README.md) — `@prism/core/persistence` — `createCollectionStore` (Loro-backed object/edge storage), `createVaultManager` (manifest-driven lifecycle), `PersistenceAdapter` + memory adapter.
- [vfs/](./vfs/README.md) — `@prism/core/vfs` — content-addressed SHA-256 blob store for binary assets, Binary Forking Protocol locks, `VfsAdapter` interface.
- [crdt-stores/](./crdt-stores/README.md) — `@prism/core/stores` — Zustand store factories (`createCrdtStore`, `createGraphStore`) that bridge Loro documents into reactive state.
- [batch/](./batch/README.md) — `@prism/core/batch` — `createBatchTransaction` for atomic multi-op execution with pre-flight validation, rollback, and a single undo entry.
- [clipboard/](./clipboard/README.md) — `@prism/core/clipboard` — `createTreeClipboard` for cut/copy/paste of GraphObject subtrees with deep clone, ID remapping, and internal edge preservation.
- [template/](./template/README.md) — `@prism/core/template` — `createTemplateRegistry` for `ObjectTemplate` blueprints with `{{variable}}` interpolation and round-trip `createFromObject`.
- [undo/](./undo/README.md) — `@prism/core/undo` — `UndoRedoManager` (snapshot-based undo/redo with merge/batch) and `createUndoBridge` for auto-recording TreeModel/EdgeModel mutations.

`loro-bridge.ts` is a shared helper used internally across this category and has no dedicated subpath export.
