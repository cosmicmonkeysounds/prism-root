# @prism/puck-playground

Standalone harness for iterating on the **Puck visual builder** outside the
full Prism Studio shell. Boots a real `StudioKernel` with seeded demo data,
mounts the Layout lens, and bundles to a single self-contained `index.html`
you can open from the filesystem.

## Quick start

```bash
pnpm install                                  # from repo root
pnpm --filter @prism/puck-playground dev      # http://localhost:4179
pnpm --filter @prism/puck-playground build    # → dist/index.html (single file)
pnpm --filter @prism/puck-playground typecheck
```

Open `dist/index.html` directly in any browser — no server, no daemon, no
Tauri. The whole React + Puck + Loro stack is inlined via
[`vite-plugin-singlefile`](https://github.com/richardtallent/vite-plugin-singlefile).

## Why a separate package?

Studio brings a lot of weight: Tauri IPC, Capacitor shell config, the full
22-lens panel set, daemon assets, real persistence. When you're iterating on
**how a Puck widget renders**, all of that gets in the way:

- A single rebuild touches dozens of unrelated panels.
- A single test failure in an unrelated lens blocks the dev loop.
- You can't share a static repro with someone who doesn't have the monorepo.

The playground strips Studio down to the one lens that matters
(`LayoutPanel` from `@prism/studio/panels/layout-panel.tsx`) and seeds the
kernel with real data so every data-aware widget — list, table, card-grid,
kanban, report, chart, map, calendar — has something to render the moment
it lands on the canvas.

## What it ships

### Demo workspace (`src/playground-seed.ts`)

A custom `StudioInitializer` that seeds five sample collections and seven
demo pages on first boot:

| Collection     | Count | Used by                                |
| -------------- | ----- | -------------------------------------- |
| `demo-task`    | 15    | list / table / kanban / card-grid      |
| `demo-contact` | 8     | list / card-grid                       |
| `demo-sale`    | 16    | charts / report                        |
| `demo-place`   | 7     | map widget (real lat/lng)              |
| `demo-event`   | 8     | calendar widget (dates relative to today) |

| Page                   | Exercises                                                |
| ---------------------- | -------------------------------------------------------- |
| 1. Welcome             | hero (heading / text / button) + stat widgets           |
| 2. Data Widgets        | list, table, kanban, card-grid bound to demo collections |
| 3. Charts & Reports    | bar, line, pie, area charts + report widget              |
| 4. Map & Calendar      | map widget (Leaflet) + calendar widget                   |
| 5. Forms               | text, email, textarea, select, number, date, checkbox    |
| 6. Display & Content   | alert, badge, progress-bar, markdown, code-block         |
| 7. Layout Primitives   | columns, divider, spacer, tab-container                  |

The seed runs only when the store is empty, so the **Reset workspace**
button in the header (which disposes the kernel and creates a fresh one)
restores the full demo set.

### App shell (`src/playground-app.tsx`)

A 175-line React app:

- Creates a singleton `StudioKernel` via
  `createStudioKernel({ lensBundles: [layoutLensBundle], initializers: [playgroundSeedInitializer] })`.
- Wraps `LayoutPanel` in `KernelProvider`.
- Left sidebar lists every `page` object from the kernel and calls
  `kernel.select(pageId)` on click. Highlights the active page by walking
  the `parentId` chain from the current selection.
- Header has a **Reset workspace** button that disposes the kernel and
  re-mounts with a fresh one (key swap).

No router, no presence, no relay manager UI, no settings panel.

## How it pulls in `@prism/studio` source

`vite.config.ts` reuses the same `buildCoreAliases()` trick the studio uses
for `@prism/core/*`, plus a regex alias `@prism/studio/*` →
`../prism-studio/src/$1`. Both are mirrored in `tsconfig.json`'s `paths`.
This sidesteps the need for a `package.json` `exports` map on the studio
package and means **the playground always sees the latest source** — no
intermediate build step.

## Build output

`pnpm build` produces a single `dist/index.html` (~10 MB, ~3 MB gzipped)
that contains the full React + Puck + Loro + recharts + Leaflet runtime
plus the seeded demo workspace. `cssCodeSplit: false` and
`assetsInlineLimit: 100_000_000` are set so the file is truly self-contained.

## What this is **not**

- **Not** a test fixture — for real test coverage of widget logic, see the
  `*.test.ts` files alongside each widget renderer in `@prism/studio`.
- **Not** a published artifact — `private: true`. The single-file build is
  for sharing reproductions and quick browser testing only.
- **Not** a distribution channel for Puck widgets — widgets live in
  `@prism/studio/src/components/`. The playground is a consumer, not a host.
