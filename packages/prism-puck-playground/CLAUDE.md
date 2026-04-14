# @prism/puck-playground

Standalone harness for iterating on the Puck visual builder outside the full
Studio shell. Vite SPA → single-file `dist/index.html`.

## Build
- `pnpm --filter @prism/puck-playground dev` — Vite dev server on :4179
- `pnpm --filter @prism/puck-playground build` — single-file `dist/index.html`
- `pnpm --filter @prism/puck-playground typecheck`

No test suite — widget behaviour is covered by `*.test.ts` files in
`@prism/studio` next to each renderer.

## Architecture

- **Vite SPA**, dark themed, mounts a single React root in `index.html`.
- **`vite-plugin-singlefile`** inlines all JS + CSS + assets into one HTML.
  `cssCodeSplit: false`, `assetsInlineLimit: 100_000_000`.
- **`vite-plugin-wasm` + `vite-plugin-top-level-await`** are mandatory because
  the kernel pulls in `loro-crdt`.
- **No tauri / capacitor / daemon dependencies.** The playground deliberately
  avoids the universal-host plumbing — it consumes Studio source directly via
  Vite alias.

## Source-level reuse of `@prism/studio`

`vite.config.ts` builds two alias families:

1. `buildCoreAliases()` re-uses prism-core's `package.json` `exports` map (one
   alias per subpath, sorted longest-first). Same trick the studio uses,
   copied here so the playground stands alone.
2. Regex alias `@prism/studio/*` → `../prism-studio/src/$1` exposes the
   studio's source tree directly. Mirrored in `tsconfig.json` `paths`.
3. `elkjs` is forced to `elkjs/lib/elk.bundled.js` (no web-worker dep), same
   as studio.

This means **the playground always sees latest studio source — no build
artifact, no `exports` field, no publish step**. The trade-off: every studio
TS error blocks the playground typecheck.

## Files

- `src/main.tsx` — React root mount. Imports `leaflet/dist/leaflet.css` (the
  studio does this in its own `main.tsx`; the playground re-imports because
  the map widget renderer deliberately does *not* import the CSS so it stays
  vitest-safe).
- `src/playground-app.tsx` — kernel construction + `<StudioShell>` mount. No
  hand-rolled chrome: the playground mounts the same `StudioShell` Studio
  uses, with `createBuiltinLensBundles()` (full Studio panel set) and
  `createBuiltinShellWidgetBundles()` so the default `DEFAULT_STUDIO_SHELL_TREE`
  resolves every widget it references (`ActivityBar`, `TabBar`,
  `ObjectExplorer`, `ComponentPalette`, `InspectorPanel`, `PresenceIndicator`,
  `UndoStatusBar`). Opens the Layout tab **synchronously inside
  `createKernel()`** — not in a post-mount `useEffect` — so the first render
  never flashes the `LensOutlet` "No tab open" empty state. Uses the
  lifecycle pattern `useEffect(() => () => kernel.dispose(), [kernel])` and
  a `key` swap on `<KernelProvider>` to re-mount the provider tree on reset.
  The only playground-specific chrome is a fixed-position `ResetButton`
  overlay — everything else (tab switching, tree navigation, inspector,
  undo/redo, presence) comes from Studio's shell widgets.
- `src/playground-seed.ts` — `playgroundSeedInitializer: StudioInitializer`.
  Seeds 5 demo collections (15 tasks, 8 contacts, 16 sales, 7 places, 8
  events) and 7 demo pages, then calls `kernel.undo.clear()` and selects the
  Welcome page. Guard `kernel.store.objectCount() > 0` makes it a no-op on
  re-runs (so reset only refills via the kernel-recreate path, not via the
  initializer running twice).
- `src/vite-env.d.ts` — `*.png`/`*.css` ambient declarations.
- `index.html` — minimal shell, dark `#0b1020` background.
- `vite.config.ts` — alias setup + `viteSingleFile()` plugin.
- `tsconfig.json` — extends root `tsconfig.base.json`, adds `paths` mirroring
  the vite aliases.

## Lens setup

The playground installs `createBuiltinLensBundles()` — the full Studio
panel set — so the ActivityBar/TabBar populate with every authoring lens
and users can mock up full apps / pages exactly like Studio (layout,
canvas, editor, graph, admin, app-builder, etc.). The Vite alias
`@prism/studio/*` → `../prism-studio/src/$1` means adding a new panel in
Studio automatically flows through to the playground on next rebuild —
no re-export needed. The shell-mode system (`use` / `build` / `admin`) is
still wired in: the `ShellModeMenu` in the top bar (or `Cmd+Shift+E`)
cycles modes, and the ActivityBar/TabBar filter themselves down
accordingly. Default boot mode is `admin` / `dev`, same as Studio.

## Reset behaviour

The header **Reset workspace** button calls `kernel.dispose()`, replaces the
state with a freshly-constructed kernel, and increments a key on
`KernelProvider` so the entire React subtree unmounts and remounts. This is
the cleanest way to guarantee the seed initializer runs again — re-running
`playgroundSeedInitializer.install()` against an existing kernel is a no-op
because of the `objectCount() > 0` guard.

## When to add to the playground

- ✅ A new widget renderer that's hard to test in isolation.
- ✅ A reproduction of a layout-panel bug a user reported.
- ✅ A demo page that exercises a new widget category end-to-end.
- ❌ Anything that needs Tauri IPC, the daemon, or a real relay connection —
  use the full studio.
- ❌ Unit tests for widget logic — those go next to the widget renderer in
  `@prism/studio`.
