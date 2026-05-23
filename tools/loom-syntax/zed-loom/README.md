# Loom (Prism) — Zed extension

Minimal Zed extension for `.loom` files. Part of the
[Prism Framework](https://github.com/anthropics/prism-root).

## What it covers today

- File association for `.loom`.
- Comment markers (`//`, `/* … */`).
- Auto-closing brackets for `{ }`, `[ ]`, `( )`, `[[ ]]`, `${ }`,
  `$( )`, and `" "`.
- Default indent (2 spaces, no hard tabs) — Loom is
  indentation-sensitive (`loom-grammar.md` §2.3).

## What it doesn't cover yet

**No syntax highlighting.** Zed renders syntax via tree-sitter
grammars, and the Prism Loom contribution doesn't ship one. The
`prism-framework/vscode-loom` extension covers VSCode / Sublime /
IntelliJ / GitHub Linguist via the canonical
`loom.tmLanguage.json` (also under `tools/loom-syntax/`); Zed
intentionally goes a different route.

The plan is to ship a Prism Loom **LSP server** (backed by the
canonical Loom parser in `prism-core::language::loom`) that emits
LSP semantic tokens. Zed picks those up natively and renders
highlights matching the validator's view of the source — accurate by
construction. The `[language_servers.loom-lsp]` block in
`extension.toml` reserves the name; the server binary is the next
deliverable.

## Local development

1. `zed: install dev extension` from the command palette.
2. Point it at this directory.
3. Open any `.loom` file — bracket pairing and comments should work
   immediately. Highlights will appear once the LSP server lands.
