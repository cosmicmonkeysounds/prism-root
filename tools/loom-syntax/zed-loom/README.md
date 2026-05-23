# Loom (Prism) — Zed extension

Zed extension for the Loom storytelling language. Part of the
[Prism Framework](https://github.com/anthropics/prism-root).

## What it does

- **File association** for `.loom`.
- **Comment markers** (`//`, `/* … */`).
- **Auto-closing brackets** for `{ }`, `[ ]`, `( )`, `[[ ]]`,
  `${ }`, `$( )`, and `" "`.
- **Default indent**: 2 spaces, no hard tabs — Loom is
  indentation-sensitive (`loom-grammar.md` §2.3).
- **LSP wiring**: spawns `prism loom lsp` and streams diagnostics,
  hover, completion, and semantic-token highlights from the
  canonical Loom parser in
  [`prism-core::language::loom`](../../../packages/prism-core/src/language/loom/).

## Setup

1. Install the unified `prism` binary on your PATH (see the
   workspace root for `./scripts/install-cli.sh`). The extension
   shells out to `prism loom lsp`.
2. From Zed's command palette: `zed: install dev extension`, point
   it at this directory.
3. Open a `.loom` file — diagnostics appear inline, hover reveals
   keyword help, completion suggests reserved words, and semantic
   tokens colour the source via the LSP stream.

## Override the LSP path

If you don't want to install the full `prism` binary, you can run
the standalone `prism-loom-lsp` instead. Add to your Zed settings:

```jsonc
"lsp": {
  "prism-loom": {
    "binary": {
      "path": "/absolute/path/to/target/debug/prism-loom-lsp",
      "arguments": []
    }
  }
}
```

## How it differs from the VSCode extension

The `vscode-loom` extension highlights via a regex TextMate grammar.
This Zed extension highlights via LSP semantic tokens, which see the
actual parse tree — so an unknown `@cue` shows as an error, not just
a static-ref. Both share the same diagnostic ids (§21 of the
grammar doc).
