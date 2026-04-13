# clipboard

Cut / copy / paste for GraphObject subtrees. Objects are deep-cloned on copy, IDs are remapped on paste to avoid collisions, and edges whose endpoints both live inside the copied subtree are preserved. Cut behaves as copy + delete sources on paste.

```ts
import { createTreeClipboard } from '@prism/core/clipboard';
```

## Key exports

- `createTreeClipboard(options)` — build a clipboard bound to a `TreeModel` (and optional `EdgeModel` + `UndoRedoManager`).
- `TreeClipboard` — interface with `copy(ids)`, `cut(ids)`, `paste(options?)`, `clear()`, plus `hasContent` and `entry` accessors.
- `ClipboardEntry` / `ClipboardMode` — the stored payload (`'copy' | 'cut'` plus serialized subtrees).
- `SerializedSubtree` — deep-cloned `{ root, descendants, internalEdges }` frozen at copy time.
- `PasteOptions` / `PasteResult` — target parent / position and the newly-created objects and edges returned from `paste`.

## Usage

```ts
import { createTreeClipboard } from '@prism/core/clipboard';

const clipboard = createTreeClipboard({ tree, edges, undo });

clipboard.copy(['section-1', 'section-2']);

const result = clipboard.paste({ parentId: 'page-2' });
if (result) {
  console.log('pasted', result.newObjects.length, 'objects');
}
```
