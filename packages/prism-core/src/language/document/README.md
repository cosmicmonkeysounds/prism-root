# document/

`PrismFile` + `FileBody` — the single file/document abstraction from ADR-002 §A1. Wraps plain strings, `LoroText`, `GraphObject`, and `BinaryRef` in a discriminated union so surfaces, syntax, codegen, and persistence share one "what a file is" contract.

```ts
import { createTextFile, isTextBody, type PrismFile } from '@prism/core/document';
```

## Key exports

- `PrismFile` — unified record: `path`, `languageId?`, `surfaceId?`, `body`, optional `schema`/`metadata`.
- `FileBody` — discriminated union: `{ kind: 'text'; ref: LoroText | string }` | `{ kind: 'graph'; ref: GraphObject }` | `{ kind: 'binary'; ref: BinaryRef }`.
- `createTextFile` / `createGraphFile` / `createBinaryFile` — ergonomic constructors for each body kind.
- `isTextBody` / `isGraphBody` / `isBinaryBody` — type guards that narrow `FileBody` to one variant.

## Usage

```ts
import { createTextFile, isTextBody } from '@prism/core/document';

const file = createTextFile({
  path: 'notes/todo.md',
  text: '# Today\n- ship readmes',
  languageId: 'prism:markdown',
});

if (isTextBody(file.body)) {
  const source = typeof file.body.ref === 'string' ? file.body.ref : file.body.ref.toString();
  console.log(source);
}
```
