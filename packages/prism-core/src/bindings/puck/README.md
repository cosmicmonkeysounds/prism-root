# puck/

Puck (`@measured/puck`) layout bridge backed by Loro. Puck never saves on its own — it is strictly a visual manipulator. The Loro CRDT is the source of truth; Puck's `data`/`onChange` is wired to a Loro map entry.

Puck is Prism's page-builder surface. Any "view" (kanban, chart, map, report, timeline, etc.) is implemented as a Puck widget — it is NOT a new `ViewMode` entry on the lens system.

```ts
import { createPuckLoroBridge, usePuckLoro } from "@prism/core/puck";
```

## Key exports

- `createPuckLoroBridge(doc?, stateKey?)` — construct a bridge that round-trips Puck `Data` through a Loro `root` map entry. Returns `{ doc, getData, setData, subscribe }`.
- `usePuckLoro({ bridge })` — React hook returning `{ data, onChange }` to feed Puck; re-renders when Loro changes arrive from other peers/tabs.
- Types: `PuckLoroBridge`, `UsePuckLoroOptions`.

## Usage

```tsx
import { Puck } from "@measured/puck";
import { LoroDoc } from "loro-crdt";
import { createPuckLoroBridge, usePuckLoro } from "@prism/core/puck";

const bridge = createPuckLoroBridge(new LoroDoc());

function PageBuilder({ config }: { config: Config }) {
  const { data, onChange } = usePuckLoro({ bridge });
  return <Puck config={config} data={data} onPublish={onChange} />;
}
```

## Note

Because the bridge stores the whole Puck `Data` as a single JSON string in a Loro map entry, peer merges are last-write-wins at the document level. For per-block CRDT merge, project into kernel objects directly (see Studio's `layout-panel`) and reserve this bridge for standalone / test contexts.
