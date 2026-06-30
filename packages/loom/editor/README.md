# loom-app (the Loom editor)

The user-facing **web IDE** for authoring `.loom` projects — a local-first
editor with a visual canvas, a modal "Studio" shell (Writing / Editing /
Simulating / Performing / Production), and an in-browser play engine.

This is the **authoring** surface. It is distinct from the participant
app ([`loom-play`](../play)), which is what guests and performers use
during a live event.

> Architecture, the Studio shell topology, the Run/Debug surfaces, and the
> play model are documented in [`CLAUDE.md`](./CLAUDE.md). This README is
> the quick "what is it / how do I run it".

## Stack

React 19 · TypeScript · Vite · Tailwind v4 · CodeMirror
(`@uiw/react-codemirror`) · `@xyflow/react` canvas · `allotment` ·
`@dnd-kit` · `zustand`.

## Install

The loom JS packages share one install — run it once from the loom root:

```bash
cd packages/loom && pnpm install
```

(`packages/loom/pnpm-workspace.yaml` covers `core`, `play`, and `editor`.)

## Run

```bash
pnpm --filter loom-app dev        # Vite + HMR at http://localhost:5173
```

Play works out of the box with **no relay and no account** — open a local
`.loom` folder (File System Access API) or use the bundled tutorial, hit
`⌘3` (Simulating), and Start. For collaborative/cloud play, run the relay
alongside it; the one-command dev loop is `prism loom dev` (Vite :5173 +
`loom-relayd` :7878). See [`CLAUDE.md` → Hosting / Playing](./CLAUDE.md).

```bash
pnpm --filter loom-app build      # → dist/ (served by loom-relayd in prod)
```

## The engine under the editor

Today the editor's parse / lint / play surfaces run the Rust engine
compiled to wasm (`packages/loom/wasm`, rebuilt with `pnpm wasm:build`).
The native TypeScript port [`@loom/core`](../core) — a complete parser
plus a first-principles ecosystem runtime — is being built to replace that
wasm bundle (no WASM, no Prism). See the core README for status.
