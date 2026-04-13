# atom

Reactive UI state atoms. `PrismBus` is a lightweight typed event bus that bridges the push world (actions, mutations) into the pull world (Zustand atoms). `AtomStore` holds UI state (selection, active panel, search query); `ObjectAtomStore` is an in-memory mirror of GraphObjects + edges; `connectBusToAtoms`/`connectBusToObjectAtoms` wire them together.

## Import

```ts
import {
  createPrismBus,
  createAtomStore,
  createObjectAtomStore,
  connectBusToAtoms,
  connectBusToObjectAtoms,
  PrismEvents,
  selectChildren,
} from "@prism/core/atom";
```

## Key exports

- `createPrismBus()` — factory for `PrismBus` with `emit`/`on`/`once`/`off`/`listenerCount`.
- `PrismEvents` — well-known event-type constants (`ObjectCreated`, `SelectionChanged`, `NavigationNavigate`, ...).
- `createAtomStore()` — Zustand vanilla store with `selectedId`, `selectionIds`, `editingObjectId`, `activePanel`, `searchQuery`, `navigationTarget`.
- `createObjectAtomStore()` — in-memory `{ objects, edges }` cache with `setObject`/`setObjects`/`removeObject`/`moveObject`/`setEdge`/`removeEdge`.
- `connectBusToAtoms(bus, atomStore)` / `connectBusToObjectAtoms(bus, objectStore)` — one-shot bridges, return cleanup.
- Selectors: `selectObject`, `selectQuery`, `selectChildren`, `selectEdgesFrom`, `selectEdgesTo`, `selectAllObjects`, `selectAllEdges`.

## Usage

```ts
import {
  createPrismBus,
  createAtomStore,
  connectBusToAtoms,
  PrismEvents,
} from "@prism/core/atom";

const bus = createPrismBus();
const atoms = createAtomStore();
const disconnect = connectBusToAtoms(bus, atoms);

bus.emit(PrismEvents.SelectionChanged, { id: "obj-123" });
atoms.getState().selectedId; // "obj-123"
```
