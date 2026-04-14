/**
 * Pure helpers that back the lens auto-aggregator in `./index.tsx`.
 *
 * Split into their own module so tests can import them without pulling
 * in the `import.meta.glob` call that lives in `./index.tsx` (which
 * eagerly loads every panel, including ones that touch browser-only
 * globals like `window` at import time).
 */

import type { LensBundle } from "./bundle.js";

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
 * Build the canonical bundle list from a pre-loaded module map. Pure so
 * tests can exercise ordering / filtering without `import.meta.glob`.
 *
 * - Sorts by file path for deterministic ordering (KBar listing, tab
 *   iteration order).
 * - Deduplicates bundles that share an id across modules, keeping the
 *   first occurrence.
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
      bundles.push(bundle);
    }
  }
  return bundles;
}
