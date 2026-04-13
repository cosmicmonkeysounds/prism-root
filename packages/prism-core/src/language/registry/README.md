# registry/

The unified language + document registry from ADR-002 §A2. Replaces the legacy parallel `LanguageRegistry` / `DocumentSurfaceRegistry` pair with one record type (`LanguageContribution`) and one registry that resolves by id or filename. Generic over renderer/editor-extension types so the core stays React/CodeMirror-free.

```ts
import { LanguageRegistry, type LanguageContribution } from '@prism/core/language-registry';
```

## Key exports

- `LanguageRegistry<TRenderer, TEditorExtension>` — registry with `register`, `unregister`, `get`, `resolve({ id?, filename? })`, `resolveByPath`, `getByExtension`, `all()`.
- `LanguageContribution` — descriptor for a language: `id`, `extensions`, `displayName`, optional `parse`, `serialize`, `surface`, `codegen`, `syntaxProvider`.
- `LanguageSurface` — surface config: `defaultMode`, `availableModes`, `inlineTokens`, optional renderers.
- `LanguageCodegen` — optional codegen hooks attached to a contribution.
- `ResolveOptions` — `{ id?, filename? }` input to `resolve`.
- `SurfaceMode` — `"code" | "preview" | "form" | "canvas" | string`.
- `InlineTokenDef` / `InlineTokenBuilder` / `inlineToken` — inline-token descriptors for surfaces.
- `WIKILINK_TOKEN` — canonical `[[wiki-link]]` inline token shared by markdown and other contributions.

## Usage

```ts
import { LanguageRegistry } from '@prism/core/language-registry';
import { createMarkdownContribution } from '@prism/core/markdown';
import { createLuauContribution } from '@prism/core/luau';

const registry = new LanguageRegistry();
registry.register(createMarkdownContribution());
registry.register(createLuauContribution());

const hit = registry.resolve({ filename: 'notes/todo.md' });
// hit?.id === 'prism:markdown'
```
