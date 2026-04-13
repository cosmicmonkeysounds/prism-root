# codemirror/

CodeMirror 6 bindings — Prism's sole editor (no Monaco). The marquee extension is `loroSync`, which makes CM6 a projection over a Loro `LoroText` so every keystroke mutates the CRDT first. Also includes editor setups, YAML/Loom languages, markdown live-preview, a spell-check extension, and inline-token extensions.

```ts
import { loroSync, useCodemirror } from "@prism/core/codemirror";
```

## Key exports

- `loroSync({ doc, text })` — CM6 `Extension` that bidirectionally syncs an `EditorView` with a `LoroText` node.
- `createLoroTextDoc()` — helper that returns a fresh `{ doc, text }` pair.
- `useCodemirror(options)` — React hook mounting a CM6 editor synced to a `LoroText`. Options: `{ doc, text, extensions?, readOnly? }`.
- `prismEditorSetup()` / `prismJSLang` / `prismJSONLang` — default editor setup and canned language bundles.
- `yamlLanguageSupport`, `yamlHighlightStyle`, `yamlStreamLanguage` — YAML language support.
- `loomLezerLanguage`, `loomLezerHighlightStyle`, `loomLanguageSupport` — Loom (Prism visual script) Lezer grammar.
- `markdownLivePreview` — markdown live-preview extension.
- `spellCheckExtension`, `SpellCheckExtensionBuilder`, `spellCheckExtensionBuilder` — pluggable spell-check.
- `createTokenMarkExtension`, `createTokenPreviewExtension`, `inlineTokenTheme` — inline token decorations (wiki-links, mentions, tags).
- `findLuauBlocks`, `processLuauBlocks`, `formatBlockResult` — Luau code-block runner for markdown.
- Types: `LoroSyncConfig`, `UseCodemirrorOptions`, `SpellCheckExtensionConfig`, `LuauRunner`, `LuauRunnerResult`, `LuauMarkdownBlock`.

## Usage

```tsx
import { LoroDoc } from "loro-crdt";
import { useCodemirror } from "@prism/core/codemirror";

function NoteEditor({ doc }: { doc: LoroDoc }) {
  const text = doc.getText("content");
  const { containerRef } = useCodemirror({ doc, text });
  return <div ref={containerRef} />;
}
```
