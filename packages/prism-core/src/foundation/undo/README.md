# undo

Framework-agnostic undo/redo. Mutations reduce to `ObjectSnapshot`s capturing `before`/`after` state; undo restores `before`, redo re-applies `after`. Entries can be coalesced via `merge` (for rapid edits) or pushed as single multi-snapshot batches (for compound operations). `createUndoBridge` wires `TreeModel` / `EdgeModel` lifecycle hooks so every mutation auto-records a snapshot without manual bookkeeping.

```ts
import { UndoRedoManager, createUndoBridge } from '@prism/core/undo';
```

## Key exports

- `UndoRedoManager` — class constructed with `(applier, { maxHistory? })`. Methods: `push(description, snapshots)`, `merge(snapshots)`, `undo()`, `redo()`, `clear()`. Accessors: `canUndo`, `canRedo`, `undoLabel`, `redoLabel`, `history`, `historySize`, `futureSize`. Listeners via `subscribe(listener)`.
- `ObjectSnapshot` — `{ kind: 'object' | 'edge', before, after }` discriminated record.
- `UndoEntry` — `{ description, snapshots, timestamp }` stored on the past/future stacks.
- `UndoApplier` — `(snapshots, direction: 'undo' | 'redo') => void` injected into `UndoRedoManager` to apply state back to models.
- `UndoListener` — subscription callback fired after every stack change.
- `createUndoBridge(manager)` — returns `{ treeHooks, edgeHooks }` that auto-push snapshots for every TreeModel/EdgeModel mutation.
- `UndoBridge` — shape returned by `createUndoBridge`.

## Usage

```ts
import { UndoRedoManager, createUndoBridge } from '@prism/core/undo';
import { TreeModel, EdgeModel } from '@prism/core/object-model';

const manager = new UndoRedoManager((snapshots, direction) => {
  // apply each snapshot back to your tree / edge model
});

const { treeHooks, edgeHooks } = createUndoBridge(manager);
const tree = new TreeModel({ hooks: treeHooks });
const edges = new EdgeModel({ hooks: edgeHooks });

tree.add({ type: 'task', name: 'Write docs' });
if (manager.canUndo) manager.undo();
```
