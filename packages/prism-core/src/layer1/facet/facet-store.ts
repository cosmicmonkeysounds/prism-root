/**
 * FacetStore — persistent registry for FacetDefinitions.
 *
 * Stores FacetDefinitions with serialize/load for CRDT persistence.
 * Replaces the in-memory Map in studio-kernel with a subscribable store
 * that can round-trip through Loro.
 *
 * Also stores VisualScripts and ValueLists for full facet persistence.
 */

import type { FacetDefinition } from "./facet-schema.js";
import type { ValueList } from "./value-list.js";
import type { VisualScript } from "./script-steps.js";

// ── FacetStore ──────────────────────────────────────────────────────────────

export type FacetStoreListener = () => void;

export interface FacetStoreSnapshot {
  facets: FacetDefinition[];
  scripts: VisualScript[];
  valueLists: ValueList[];
}

export interface FacetStore {
  // ── Facet Definitions ───────────────────────────────────────────────────
  listFacets(): FacetDefinition[];
  getFacet(id: string): FacetDefinition | undefined;
  putFacet(definition: FacetDefinition): void;
  removeFacet(id: string): boolean;
  facetsForType(objectType: string): FacetDefinition[];

  // ── Visual Scripts ──────────────────────────────────────────────────────
  listScripts(): VisualScript[];
  getScript(id: string): VisualScript | undefined;
  putScript(script: VisualScript): void;
  removeScript(id: string): boolean;

  // ── Value Lists ─────────────────────────────────────────────────────────
  listValueLists(): ValueList[];
  getValueList(id: string): ValueList | undefined;
  putValueList(list: ValueList): void;
  removeValueList(id: string): boolean;

  // ── Persistence ─────────────────────────────────────────────────────────
  serialize(): FacetStoreSnapshot;
  load(snapshot: FacetStoreSnapshot): void;

  // ── Subscription ────────────────────────────────────────────────────────
  onChange(listener: FacetStoreListener): () => void;

  /** Total items across all registries. */
  readonly size: number;
}

export function createFacetStore(): FacetStore {
  const facets = new Map<string, FacetDefinition>();
  const scripts = new Map<string, VisualScript>();
  const valueLists = new Map<string, ValueList>();
  const listeners = new Set<FacetStoreListener>();

  function notify(): void {
    for (const fn of listeners) fn();
  }

  return {
    // ── Facets ──────────────────────────────────────────────────────────
    listFacets(): FacetDefinition[] {
      return [...facets.values()];
    },
    getFacet(id: string): FacetDefinition | undefined {
      return facets.get(id);
    },
    putFacet(definition: FacetDefinition): void {
      facets.set(definition.id, definition);
      notify();
    },
    removeFacet(id: string): boolean {
      const deleted = facets.delete(id);
      if (deleted) notify();
      return deleted;
    },
    facetsForType(objectType: string): FacetDefinition[] {
      return [...facets.values()].filter((f) => f.objectType === objectType);
    },

    // ── Scripts ─────────────────────────────────────────────────────────
    listScripts(): VisualScript[] {
      return [...scripts.values()];
    },
    getScript(id: string): VisualScript | undefined {
      return scripts.get(id);
    },
    putScript(script: VisualScript): void {
      scripts.set(script.id, script);
      notify();
    },
    removeScript(id: string): boolean {
      const deleted = scripts.delete(id);
      if (deleted) notify();
      return deleted;
    },

    // ── Value Lists ─────────────────────────────────────────────────────
    listValueLists(): ValueList[] {
      return [...valueLists.values()];
    },
    getValueList(id: string): ValueList | undefined {
      return valueLists.get(id);
    },
    putValueList(list: ValueList): void {
      valueLists.set(list.id, list);
      notify();
    },
    removeValueList(id: string): boolean {
      const deleted = valueLists.delete(id);
      if (deleted) notify();
      return deleted;
    },

    // ── Persistence ─────────────────────────────────────────────────────
    serialize(): FacetStoreSnapshot {
      return {
        facets: [...facets.values()],
        scripts: [...scripts.values()],
        valueLists: [...valueLists.values()],
      };
    },
    load(snapshot: FacetStoreSnapshot): void {
      facets.clear();
      scripts.clear();
      valueLists.clear();
      for (const f of snapshot.facets) facets.set(f.id, f);
      for (const s of snapshot.scripts) scripts.set(s.id, s);
      for (const v of snapshot.valueLists) valueLists.set(v.id, v);
      notify();
    },

    // ── Subscription ────────────────────────────────────────────────────
    onChange(listener: FacetStoreListener): () => void {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },

    get size(): number {
      return facets.size + scripts.size + valueLists.size;
    },
  };
}
