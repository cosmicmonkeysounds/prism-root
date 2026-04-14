/**
 * Pure helpers that back the lens auto-aggregator in `./index.tsx`.
 *
 * Split into their own module so tests can import them without pulling
 * in the `import.meta.glob` call that lives in `./index.tsx` (which
 * eagerly loads every panel, including ones that touch browser-only
 * globals like `window` at import time).
 *
 * ## Shell-mode constraint attachment
 *
 * Panel files stay terse — they only declare `defineLensBundle(manifest,
 * Component)` and let the auto-aggregator hang shell-mode visibility
 * constraints off each bundle at load time. The policy lives centrally
 * in `./panel-modes.ts` (see that file's header for the rationale).
 *
 * `buildLensBundleList` walks the looked-up row for each discovered
 * bundle and runs it through `withShellModes` from `@prism/core/lens`.
 * Bundles whose id isn't in the table pass through unchanged and pick
 * up the library defaults (`availableInModes: ["build", "admin"]`,
 * `minPermission: "user"`).
 */

import { withShellModes } from "@prism/core/lens";
import type { LensBundle } from "./bundle.js";
import { lookupPanelMode } from "./panel-modes.js";

function isLensBundle(value: unknown): value is LensBundle {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as { id?: unknown; install?: unknown };
  return (
    typeof candidate.id === "string" &&
    typeof candidate.install === "function"
  );
}

/**
 * Scan one loaded panel module and return every `*LensBundle` export
 * that passes the shape check.
 */
export function collectLensBundlesFromModule(
  mod: Record<string, unknown>,
): LensBundle[] {
  const out: LensBundle[] = [];
  for (const [key, value] of Object.entries(mod)) {
    if (!key.endsWith("LensBundle")) continue;
    if (isLensBundle(value)) out.push(value);
  }
  return out;
}

/**
 * Apply the centralized `PANEL_MODES` row (if any) to a single bundle.
 * Bundles without an entry pass through unchanged.
 */
function applyPanelMode(bundle: LensBundle): LensBundle {
  const constraints = lookupPanelMode(bundle.id);
  if (
    constraints.availableInModes === undefined &&
    constraints.minPermission === undefined
  ) {
    return bundle;
  }
  return withShellModes(bundle, constraints);
}

/**
 * Build the canonical bundle list from a pre-loaded module map. Pure so
 * tests can exercise ordering / filtering without `import.meta.glob`.
 *
 * - Sorts by file path for deterministic ordering (KBar listing, tab
 *   iteration order).
 * - Deduplicates bundles that share an id across modules, keeping the
 *   first occurrence.
 * - Attaches shell-mode constraints from `./panel-modes.ts` so the
 *   kernel can filter bundles per shell mode + permission at query
 *   time.
 */
export function buildLensBundleList(
  modules: Record<string, Record<string, unknown> | undefined>,
): LensBundle[] {
  const bundles: LensBundle[] = [];
  const seen = new Set<string>();
  for (const path of Object.keys(modules).sort()) {
    const mod = modules[path];
    if (!mod) continue;
    for (const bundle of collectLensBundlesFromModule(mod)) {
      if (seen.has(bundle.id)) continue;
      seen.add(bundle.id);
      bundles.push(applyPanelMode(bundle));
    }
  }
  return bundles;
}
