# design-tokens

Framework-agnostic CSS variable registry for theming. Tokens live in three buckets (`colors`, `spacing`, `fonts`) and are materialized as CSS custom properties on `:root`. No React, no DOM — just plain data, a small subscribable registry, and helpers that emit `<style>`-ready strings. The same registry feeds both live Studio theming and the page-builder HTML export.

## Import

```ts
import {
  DEFAULT_TOKENS,
  createDesignTokenRegistry,
  tokensToCss,
  lookupToken,
  mergeTokens,
} from "@prism/core/design-tokens";
```

## Key exports

- `DEFAULT_TOKENS` — baseline `DesignTokenBundle` (10 colors, 6 spacing steps, 3 font stacks).
- `DesignTokenRegistry` / `createDesignTokenRegistry(initial?)` — mutable store with `get`/`set`/`patch`/`subscribe`.
- `tokensToCss(bundle)` — renders a bundle to a `:root { --color-*: ...; --space-*: ...px; --font-*: ...; }` string.
- `lookupToken(bundle, "colors.primary")` — dotted-path reader, returns `string | number | undefined`.
- `mergeTokens(base, patch)` — shallow per-bucket merge.
- Types: `DesignTokenBundle`.

## Usage

```ts
import {
  createDesignTokenRegistry,
  tokensToCss,
  DEFAULT_TOKENS,
} from "@prism/core/design-tokens";

const tokens = createDesignTokenRegistry(DEFAULT_TOKENS);
tokens.patch({ colors: { primary: "#6366f1" } });

const css = tokensToCss(tokens.get());
// ":root { --color-primary: #6366f1; ... }"
```
