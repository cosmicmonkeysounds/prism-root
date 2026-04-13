# batch

Atomic multi-operation transactions for TreeModel and EdgeModel. Collects a queue of typed mutation descriptors, validates them up front, executes them as a unit, rolls back on failure, and pushes a single combined entry to undo.

```ts
import { createBatchTransaction } from '@prism/core/batch';
```

## Key exports

- `createBatchTransaction(options)` — build a transaction bound to a `TreeModel` (and optional `EdgeModel` + `UndoRedoManager`).
- `BatchTransaction` — interface with `add`, `addAll`, `validate`, `execute`, `clear`, plus `size` and `ops` accessors.
- `BatchOp` — discriminated union over 7 op kinds: `create-object`, `update-object`, `delete-object`, `move-object`, `create-edge`, `update-edge`, `delete-edge`.
- `BatchResult` — `{ executed, created, createdEdges }` returned from `execute`.
- `BatchValidationResult` / `BatchValidationError` — returned from `validate()` for pre-flight checks.
- `BatchProgress` / `BatchProgressCallback` — per-op progress callback passed via `BatchExecuteOptions.onProgress`.

## Usage

```ts
import { createBatchTransaction } from '@prism/core/batch';

const tx = createBatchTransaction({ tree, edges, undo });
tx.add({ kind: 'create-object', draft: { type: 'task', name: 'A' } });
tx.add({ kind: 'update-object', id: 'task-1', changes: { status: 'done' } });
tx.add({ kind: 'delete-object', id: 'task-2' });

const check = tx.validate();
if (!check.valid) throw new Error(check.errors[0]?.reason);

const result = tx.execute({ description: 'Bulk update tasks' });
console.log(result.executed, result.created.length);
```
