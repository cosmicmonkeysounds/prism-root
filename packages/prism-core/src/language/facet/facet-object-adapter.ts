/**
 * FacetDefinition ↔ GraphObject adapter.
 *
 * Facets are stored as first-class GraphObjects of type "facet-def" in
 * the same CollectionStore that backs every other entity. The full
 * FacetDefinition body lives under `GraphObject.data.definition`; the
 * shell keeps `name` and `id` in sync for the object explorer and tree
 * views. This is the bridge that unifies the Facet registry with the
 * ObjectRegistry — no parallel Map, no parallel persistence path.
 */

import type { FacetDefinition, FacetLayout } from "./facet-schema.js";
import { createFacetDefinition } from "./facet-schema.js";

/** Entity type string used for facet-def GraphObjects. */
export const FACET_DEF_TYPE = "facet-def" as const;

/**
 * Minimal shape required to feed `facetDefFromObject`. Matches the
 * `GraphObject` interface from `@prism/core/object-model` but is typed
 * locally so this file stays in `language/` and doesn't import from
 * `foundation/` (the dependency DAG flows the other way).
 */
export interface FacetObjectLike {
  id: string;
  name: string;
  data: Record<string, unknown>;
}

/**
 * Extract a FacetDefinition from a GraphObject shell + data payload.
 * Missing fields fall back to createFacetDefinition defaults. If the
 * object doesn't look like a facet-def at all (e.g. `data.definition`
 * is absent), a minimal empty facet is returned so callers don't crash.
 */
export function facetDefFromObject(obj: FacetObjectLike): FacetDefinition {
  const raw = (obj.data["definition"] ?? {}) as Partial<FacetDefinition>;
  const id = typeof raw.id === "string" && raw.id.length > 0 ? raw.id : obj.id;
  const objectType =
    typeof raw.objectType === "string" ? raw.objectType : "";
  const layout: FacetLayout =
    (raw.layout as FacetLayout) ?? "form";
  const base = createFacetDefinition(id, objectType, layout);
  return {
    ...base,
    ...raw,
    id,
    name: raw.name ?? obj.name ?? id,
    objectType,
    layout,
    parts: Array.isArray(raw.parts) ? raw.parts : [],
    slots: Array.isArray(raw.slots) ? raw.slots : [],
  };
}

/**
 * Produce the patch you'd hand to `kernel.createObject` or
 * `kernel.updateObject` to persist a FacetDefinition. The `id` of the
 * underlying GraphObject is *not* set here — callers either generate an
 * ObjectId from the facet id (studio-kernel does this via `objectId()`)
 * or update an existing object in place.
 */
export function objectPatchFromFacetDef(
  def: FacetDefinition,
): { type: string; name: string; data: Record<string, unknown> } {
  return {
    type: FACET_DEF_TYPE,
    name: def.name || def.id,
    data: { definition: def },
  };
}

/**
 * True when a GraphObject (shell) represents a facet definition.
 * Cheap runtime check to filter a collection store listing.
 */
export function isFacetDefObject(obj: { type: string }): boolean {
  return obj.type === FACET_DEF_TYPE;
}
