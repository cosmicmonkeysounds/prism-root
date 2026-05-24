# Loom

A local-first IDE with a visual canvas, running entirely in the browser.

## Intent
- **Edit real files on disk** via the File System Access API — no server, no upload.
- **See structure visually** alongside code: every open file is also a node on a React Flow canvas, and edges represent relationships between them.
- **Stay minimal**: a thin shell over CodeMirror + xyflow, with a small Zustand store as the single source of truth.

## Stack
- React 19 + TypeScript + Vite
- Tailwind CSS v4 (`@tailwindcss/vite`)
- `@uiw/react-codemirror` (one-dark theme, per-language extensions)
- `@xyflow/react` for the canvas
- `zustand` for state
- `pnpm` for package management

## Layout
```
src/
  lib/        # fs (File System Access), language (CM extensions)
  store/      # zustand workspace store
  components/ # Sidebar, FileTree, Tabs, Editor, Canvas, SplitPane, StatusBar
  App.tsx     # three-pane shell
```

## Conventions
- Path alias `@/*` → `src/*`.
- Keep components small; put logic in `lib/` or `store/`.
- No backend. Anything persistent goes through the FS handles the user grants.
- Browser support: Chromium-based (File System Access API).

## Loom integration

`.loom` files are first-class. Highlighting + diagnostics flow through
`packages/loom/wasm` (built with `pnpm wasm:build`, output to
`src/loom-wasm/`):

- `src/lib/loom-language.ts` — CodeMirror `StreamLanguage` mirroring
  the Rust `loom_parser::lexer` line classifier. Token shapes match
  the TextMate grammar shipped to Zed/VSCode by `loom_syntax::emit_tmgrammar`.
- `src/lib/loom-lint.ts` — `linter()` extension that calls the
  wasm-compiled real parser (`diagnose(source)`) and maps byte spans
  to CodeMirror diagnostics. Loaded lazily on first `.loom` open.

To regenerate the wasm bundle after editing `packages/loom/parser` or
`packages/loom/wasm`, run `pnpm wasm:build` (release) or
`pnpm wasm:build:dev` (faster compile, larger output).
