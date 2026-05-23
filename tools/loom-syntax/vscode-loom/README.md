# Loom (Prism) — VSCode extension

Syntax highlighting for `.loom` files. Part of the
[Prism Framework](https://github.com/anthropics/prism-root).

## What it covers

- All §3.1 reserved keywords from `docs/dev/loom-grammar.md`, grouped
  by category so themes can colour control flow differently from
  story keywords differently from stats keywords.
- The three sigils — `$` (resolve refs), `@` (static refs), `[[ … ]]`
  (backlinks) — each with its own scope.
- Line classes: headers (`#`), sluglines (`##`), sections (`--`),
  speakers (ALL\_CAPS at line start), flavor (`>`), choices (`*` / `+`),
  diverts (`->`), returns (`<-`), actions (`~`), properties (`.name`).
- Inline atoms: triggers (`<sfx:bell>`), chain triggers
  (`<sfx:a+camera:b>`), conditional triggers (`<?expr><trigger>`),
  range closers (`</>` / `</%name>`), inline assigns (`<$x := y>`),
  inline eval (`${expr}` / `$(expr ...)`), text variations
  (`[a / b].mode`).
- Numbers with the duration suffixes from §3 (`60s`, `250ms`, `5m`,
  `1h`).
- Strings (`"…"`), docstrings (`'''…'''`), comments (`// …` and
  `/* … */`).

## What it doesn't cover (yet)

The grammar is regex-based, so it can't see structure. It will
**not** flag:

- Unknown casts, cues, locations, cohorts, factions (the validator's
  job — these are build-time errors per `loom-grammar.md` §21).
- Unbalanced brackets, unmatched range openers, dedent jumps.
- Anything that requires the registry (unknown action keywords,
  unknown trigger types, unknown stance levels).

Those land when the Prism LSP server for Loom ships. The LSP will be
backed by the canonical Loom parser inside `prism-core` and emit
[LSP semantic tokens](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_semanticTokens)
on top of this grammar for accurate highlights, plus diagnostics for
the rules above.

## Building / publishing

```bash
# From the workspace root — regenerate the bundled grammar:
prism codegen loom-tmgrammar

# Package the extension (requires `vsce` on PATH):
cd tools/loom-syntax/vscode-loom
vsce package
```

The grammar file at `syntaxes/loom.tmLanguage.json` is **generated**.
Edit `packages/prism-core/src/language/loom/keywords.rs` instead —
adding a keyword there automatically extends the highlights on the
next codegen run.

## Local development

1. Open this directory in VSCode.
2. Press `F5` to launch an Extension Development Host.
3. Open any `.loom` file in the new window — highlights should kick
   in immediately.
