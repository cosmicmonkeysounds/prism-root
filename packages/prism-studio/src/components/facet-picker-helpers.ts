/**
 * Pure helpers for the facet picker field — split out so vitest can
 * import them in the node env without evaluating Puck/React modules.
 */

import type { FacetDefinition } from "@prism/core/facet";

/** Turn a human-friendly name into a kernel-safe facet id. */
export function facetIdFromName(name: string, objectType: string): string {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  const base = slug || "facet";
  return `${objectType}-${base}`;
}

/** Pick an id that doesn't collide with an existing facet definition. */
export function uniqueFacetId(
  base: string,
  existing: ReadonlyArray<FacetDefinition>,
): string {
  const taken = new Set(existing.map((d) => d.id));
  if (!taken.has(base)) return base;
  let n = 2;
  while (taken.has(`${base}-${n}`)) n++;
  return `${base}-${n}`;
}
