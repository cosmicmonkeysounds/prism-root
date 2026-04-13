# template

Catalog of reusable `ObjectTemplate` blueprints. A template captures a tree of `TemplateNode`s plus `TemplateEdge`s and declared `TemplateVariable`s; instantiating it materialises live objects into a `TreeModel` with `{{variable}}` interpolation applied to string fields. Supports round-trip: create a template from an existing subtree, then instantiate it elsewhere.

```ts
import { createTemplateRegistry } from '@prism/core/template';
```

## Key exports

- `createTemplateRegistry(options)` — build a registry bound to a `TreeModel` (and optional `EdgeModel` + `UndoRedoManager`).
- `TemplateRegistry` — interface with `register`, `unregister`, `get`, `has`, `list(filter?)`, `instantiate`, `createFromObject`, and `size`.
- `ObjectTemplate` — blueprint with `id`, `name`, optional `description`/`category`, `variables`, `nodes`, `edges`.
- `TemplateNode`, `TemplateEdge`, `TemplateVariable` — the pieces that make up a blueprint.
- `TemplateFilter` — filter passed to `list` (by category, root type, or search string).
- `InstantiateOptions`, `InstantiateResult` — options (`variables`, `parentId`, `position`) and result (`rootId`, `createdObjects`, `createdEdges`).

## Usage

```ts
import { createTemplateRegistry } from '@prism/core/template';

const registry = createTemplateRegistry({ tree, edges, undo });

registry.register({
  id: 'sprint',
  name: 'Sprint skeleton',
  variables: [{ name: 'name', label: 'Sprint name' }],
  nodes: [{ localId: 'root', type: 'project', data: { name: '{{name}}' } }],
  edges: [],
});

const result = registry.instantiate('sprint', {
  variables: { name: 'Sprint 23' },
  parentId: 'workspace-1',
});
```
