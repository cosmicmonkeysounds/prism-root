# `tools/loom-syntax`

Editor integration for the [Loom storytelling language](../../docs/dev/loom-design.md).

Loom is Prism's authored format for branching dialogue, immersive
theatre, character + faction simulation, and reactive procedural
content. This directory holds **everything an outside editor needs**
to recognise `.loom` files — the canonical TextMate grammar plus
extension wrappers for VSCode and Zed.

```
tools/loom-syntax/
├── loom.tmLanguage.json     ← canonical TextMate grammar (generated)
├── vscode-loom/             ← VSCode extension (TextMate-based highlights)
└── zed-loom/                ← Zed extension (LSP-based, file association only today)
```

## Where the grammar comes from

The TextMate grammar is **derived** from
[`packages/prism-core/src/language/loom/`](../../packages/prism-core/src/language/loom/),
which holds the canonical keyword / sigil / operator registry for
Loom. Adding a keyword to `keywords.rs` updates the editor highlights
on the next codegen run:

```bash
prism codegen loom-tmgrammar
```

This writes two files — the canonical
`tools/loom-syntax/loom.tmLanguage.json` and the bundled copy at
`tools/loom-syntax/vscode-loom/syntaxes/loom.tmLanguage.json`. Both
are git-tracked so the extension is loadable without re-running
codegen.

**Never edit the `.json` files by hand** — they're regenerated from
Rust on every codegen run and your changes will be overwritten.

## Editor coverage

| Editor | Today | Notes |
|---|---|---|
| **VSCode** | TextMate highlights (`vscode-loom/`) | LSP refines via `prism loom lsp`; configure via `vscode-langservers-extracted`. |
| **Sublime Text 3+** | TextMate highlights (drop `loom.tmLanguage.json` in) | LSP via the `LSP` package + custom client config. |
| **IntelliJ / JetBrains** | TextMate highlights (via the TextMate bundle plugin) | LSP via the JetBrains LSP API. |
| **GitHub** | TextMate highlights (pending Linguist PR) | No LSP path on github.com. |
| **Zed** | Full LSP-driven highlights via `zed-loom/` | Diagnostics + hover + completion + semantic tokens, all from the canonical parser. |
| **Neovim / Helix** | LSP via the standalone `prism-loom-lsp` binary | Add a server config pointing at `target/debug/prism-loom-lsp` (or `prism loom lsp`). |

## Two layers: TextMate + LSP

TextMate grammars are regex-based and approximate. They do well at
**lexical** classification (this run of characters is a keyword;
this span is a string) and badly at **structural** classification
(is this `node` identifier the declaration or a reference?).
They're the *de facto* lingua franca — VSCode, Sublime, IntelliJ,
GitHub Linguist all consume the same JSON.

The **LSP server** ([`packages/prism-loom-lsp`](../../packages/prism-loom-lsp/))
is the structural layer. It's backed by the canonical Loom parser
inside
[`prism-core::language::loom`](../../packages/prism-core/src/language/loom/)
and emits:

- **Semantic tokens** — refines TextMate guesses with the parser's
  verdict (an unknown `@cue` shows as an error, not just a
  static-ref).
- **Diagnostics** — the parser surfaces ~20 of the §21 ids today
  (lex errors, unbalanced brackets, indent jumps, unknown action
  keywords, …); the validator pass that adds registry-driven ones
  (`unknown-cast`, `divert-target-unknown`, `stance-cycle`, …) is
  the next deliverable.
- **Hover** — keyword help + category labels from the
  `keywords.rs` registry.
- **Completion** — reserved words filtered by prefix.

Launch it via `prism loom lsp` (uses the unified `prism` binary) or
the standalone `prism-loom-lsp` binary if you'd rather not install
the whole CLI.

## Installing the extensions

### VSCode

```bash
cd tools/loom-syntax/vscode-loom
vsce package          # produces loom-0.1.0.vsix
code --install-extension loom-0.1.0.vsix
```

Or open `tools/loom-syntax/vscode-loom/` in VSCode and press **F5**
to launch an Extension Development Host.

### Zed

From the command palette, run **`zed: install dev extension`** and
point it at `tools/loom-syntax/zed-loom/`. Highlights are LSP-driven
(coming soon); brackets and comments work today.

### Other editors

Drop `tools/loom-syntax/loom.tmLanguage.json` into your editor's
TextMate bundle directory. Most editors with TextMate support
auto-discover it once it's there.
