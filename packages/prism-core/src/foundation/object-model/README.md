# object-model

The universal graph primitives. Every node in Prism's unified graph is a `GraphObject`: a shell of structural fields plus an opaque `data` payload interpreted by whichever `EntityDef` is registered for its type. This folder also owns the in-memory tree/edge models, containment registry, weak-ref engine, address scheme (NSID / PrismAddress), and query helpers.

```ts
import { TreeModel, EdgeModel, ObjectRegistry } from '@prism/core/object-model';
```

## Key exports

- `GraphObject`, `ObjectEdge`, `ResolvedEdge`, `ObjectId`, `EdgeId` — branded-string IDs and core data shapes. `objectId()` / `edgeId()` cast at trust boundaries.
- `EntityDef`, `EntityFieldDef`, `EntityFieldType`, `EdgeTypeDef`, `CategoryRule`, `TabDefinition`, `ApiOperation` — schema types registered into `ObjectRegistry`.
- `ObjectRegistry` — authoritative registry of entity/edge defs, containment rules, slot contributions, and tree-node lookups.
- `TreeModel` + `TreeModelError` — in-memory tree of objects with atomic `add`/`remove`/`move`/`reorder`/`duplicate`/`update`, typed events, and `beforeX`/`afterX` lifecycle hooks.
- `EdgeModel` — typed graph edges with lifecycle hooks parallel to TreeModel.
- `WeakRefEngine` — pluggable extraction of weak references between objects.
- `ContextEngine` — computes context-menu actions, autocomplete suggestions, and edge/child options from the registry.
- `NSIDRegistry`, `NSID`, `PrismAddress`, `nsid`, `parseNSID`, `isValidNSID`, `prismAddress`, `parsePrismAddress`, `isValidPrismAddress` — namespaced addressing.
- `ObjectQuery`, `matchesQuery`, `sortObjects`, `queryToParams`, `paramsToQuery` — filter/sort helpers.
- `pascal`, `camel`, `singular` — string utilities for identifier casing.

## Usage

```ts
import { TreeModel, ObjectRegistry } from '@prism/core/object-model';

const registry = new ObjectRegistry();
registry.registerEntity({ type: 'task', category: 'productivity', fields: [] });

const tree = new TreeModel({ registry });
const root = tree.add({ type: 'project', name: 'Prism' });
const task = tree.add(
  { type: 'task', name: 'Write README' },
  { parentId: root.id },
);

tree.update(task.id, { status: 'done' });
const children = tree.getChildren(root.id);
```
