# markdown/

Markdown `LanguageContribution` for the unified `LanguageRegistry`. Reuses the single `parseMarkdown` tokenizer from `@prism/core/forms` so there is exactly one markdown parser in the codebase — block tokens are projected into the generic `RootNode` shape used by every other language in the registry.

```ts
import { createMarkdownContribution } from '@prism/core/markdown';
```

## Key exports

- `createMarkdownContribution()` — builds a `LanguageContribution` with id `prism:markdown`, extensions `.md` / `.mdx` / `.markdown`, a `parse(source) => RootNode` implementation, and a surface that offers both `code` and `preview` modes plus the built-in `WIKILINK_TOKEN`.

## Usage

```ts
import { LanguageRegistry } from '@prism/core/language-registry';
import { createMarkdownContribution } from '@prism/core/markdown';

const registry = new LanguageRegistry();
registry.register(createMarkdownContribution());

const md = registry.resolve({ filename: 'notes.md' });
const ast = md?.parse?.('# Hello\n\nparagraph');
// ast.children = [{ type: 'h1', value: 'Hello' }, { type: 'p', value: 'paragraph' }]
```

The React shell supplies the actual preview renderer — this module stays framework-free.
