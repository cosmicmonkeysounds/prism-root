# layout

Page and selection state for multi-pane, multi-tab shells. `SelectionModel` is a primary+multi selection set. `PageModel` is one navigable surface (target + view mode + active tab + its own selection). `PageRegistry` maps `target.kind` → default view/tab. `LensSlot` is a back/forward-navigable slot that caches `PageModel`s per target; `LensManager` owns many slots and tracks which is focused.

## Import

```ts
import {
  SelectionModel,
  PageModel,
  PageRegistry,
  LensSlot,
  LensManager,
} from "@prism/core/layout";
```

## Key exports

- `SelectionModel` — `select`/`toggle`/`selectRange`/`selectAll`/`clear`/`isSelected`; exposes `selectedIds`/`primary`/`size`/`hasMultiple`; `on(listener)` for change events.
- `PageModel<TTarget>` — holds `id`, `target`, `objectId`, `viewMode`, `activeTab`, and its own `selection`; `setViewMode`/`setTab`/`persist`/`dispose`/`on`; `PageModel.fromSerialized(...)` rehydrates.
- `PageRegistry<TTarget>` — `register(kind, { defaultViewMode, defaultTab, getObjectId? })`, `createPage(target)`.
- `LensSlot<TTarget>` — `go`/`back`/`forward`/`activePage`/`canGoBack`/`canGoForward`/`persistPages`, with LRU `PageModel` cache.
- `LensManager<TTarget>` — `open`/`close`/`focus`, tracks `activeSlot`/`activePage`, emits `slot-opened`/`slot-closed`/`slot-focused`.
- Types: `SerializedPage`, `PageTypeDef`, `LensSlotOptions`, plus event/listener aliases.

## Usage

```ts
import { PageRegistry, LensManager } from "@prism/core/layout";

type Target = { kind: "object"; id: string };

const pages = new PageRegistry<Target>();
pages.register("object", { defaultViewMode: "list", defaultTab: "overview" });

const lenses = new LensManager<Target>();
const slot = lenses.open("main", pages, { kind: "object", id: "obj-1" });
slot.go({ kind: "object", id: "obj-2" });
slot.back();
```
