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

| Editor | Today | Tomorrow |
|---|---|---|
| **VSCode** | TextMate highlights (`vscode-loom/`) | LSP semantic tokens (refines this) |
| **Sublime Text 3+** | TextMate highlights (drop `loom.tmLanguage.json` in) | LSP if it grows a Loom client |
| **IntelliJ / JetBrains** | TextMate highlights (via the TextMate bundle plugin) | LSP via official extension |
| **GitHub** | TextMate highlights (pending Linguist PR) | — |
| **Zed** | File association + brackets only (`zed-loom/`) | LSP semantic tokens (renders highlights) |
| **Neovim / Helix** | LSP from day one (when it ships) | — |

## Why TextMate now and LSP next

TextMate grammars are regex-based and approximate. They do well at
**lexical** classification (this run of characters is a keyword;
this span is a string) and badly at **structural** classification
(is this `node` identifier the declaration or a reference?). They're
also the *de facto* lingua franca — VSCode, Sublime, IntelliJ,
GitHub Linguist, and many others all read the same JSON, so one
file covers a huge surface area.

What they can't do is reflect the validator's view of the source.
[`loom-grammar.md` §21](../../docs/dev/loom-grammar.md#21-diagnostics-lint-catalog)
lists ~60 diagnostic IDs the parser will emit — unknown casts,
unbalanced range openers, dedent jumps, cycles in the disposition
mirror graph, stance default conflicts, etc. None of those are
expressible in a TextMate grammar. They need a real parser.

The plan is a small Prism LSP server (`prism loom lsp`) backed by
the canonical Loom parser inside
[`prism-core::language::loom`](../../packages/prism-core/src/language/loom/).
It will emit:

- **Semantic tokens** — refining the TextMate guess with the
  validator's verdict (e.g. an unknown `@cue` is highlighted as an
  error, not just a static-ref).
- **Diagnostics** — every rule in `loom-grammar.md` §21, surfaced
  inline.
- **Hover** — keyword help, sigil reminders, ledger predicate
  signatures.
- **Completion** — registry-aware: only casts you've declared,
  only cues that exist, only stance levels in the registry.

The parser itself is the next deliverable — `prism-core/CLAUDE.md`
tracks `language::loom` as the new contribution slot.

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
