/**
 * Built-in lens bundles for Prism Studio.
 *
 * Every panel file colocates a `xxxLensBundle: LensBundle` with its
 * component. This file is a pure auto-aggregator — it uses Vite's
 * `import.meta.glob` to sweep `../panels/*-panel.tsx` eagerly at bundle
 * time, picks every export whose name ends in `LensBundle`, and hands
 * the resulting list to the kernel via `createBuiltinLensBundles()`.
 *
 * The upshot: adding a new lens is a **one-file** edit — create
 * `panels/my-panel.tsx` exporting `myLensBundle` and the kernel picks
 * it up automatically the next time Vite rebuilds. No manual
 * re-export, no parallel list to keep in sync.
 *
 * Ordering is alphabetical by file path for determinism (KBar listing,
 * tab iteration order, etc.). The default tab id (`EDITOR_LENS_ID`) is
 * still explicit in `App.tsx`.
 *
 * The scanning helpers live in `./collect.ts` so tests can drive them
 * without pulling in the glob (which eagerly loads every panel and its
 * browser-only transitive deps).
 */

import type { LensBundle } from "./bundle.js";
import { buildLensBundleList } from "./collect.js";

// `import.meta.glob` is a Vite / Vitest compile-time primitive. Both the
// production build and the vitest test runner understand it, so callers
// never need a Node shim.
const panelModules = import.meta.glob<Record<string, unknown>>(
  "../panels/*-panel.tsx",
  { eager: true },
);

const BUILTIN_BUNDLES: LensBundle[] = buildLensBundleList(panelModules);

// Re-export the default-tab lens id for App-level bootstrap. All other
// lens ids live next to their panel component.
export { EDITOR_LENS_ID } from "../panels/editor-panel.js";

export type { LensBundle, LensInstallContext } from "./bundle.js";
export { defineLensBundle } from "./bundle.js";

/** The canonical list of built-in Studio lens bundles. */
export function createBuiltinLensBundles(): LensBundle[] {
  return [...BUILTIN_BUNDLES];
}
