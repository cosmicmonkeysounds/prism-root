/**
 * Lens Registry — runtime registry of lens manifests.
 *
 * Follows the same pattern as createActionRegistry():
 * factory function, Map-based storage, Set<listener> for subscriptions.
 */

import type { LensId, LensCategory, LensManifest } from "./lens-types.js";

export type LensRegistryEvent =
  | { type: "registered"; manifest: LensManifest }
  | { type: "unregistered"; id: LensId }
  | { type: "change" };

export type LensRegistryEventListener = (event: LensRegistryEvent) => void;

export interface LensRegistry {
  register(manifest: LensManifest): () => void;
  unregister(id: LensId): void;
  get(id: LensId): LensManifest | undefined;
  has(id: LensId): boolean;
  allLenses(): LensManifest[];
  getByCategory(category: LensCategory): LensManifest[];
  subscribe(listener: LensRegistryEventListener): () => void;
}

export function createLensRegistry(): LensRegistry {
  const lenses = new Map<LensId, LensManifest>();
  const listeners = new Set<LensRegistryEventListener>();

  function notify(event: LensRegistryEvent): void {
    for (const listener of listeners) {
      listener(event);
    }
    if (event.type !== "change") {
      for (const listener of listeners) {
        listener({ type: "change" });
      }
    }
  }

  return {
    register(manifest: LensManifest): () => void {
      lenses.set(manifest.id, manifest);
      notify({ type: "registered", manifest });
      return () => {
        if (lenses.has(manifest.id)) {
          lenses.delete(manifest.id);
          notify({ type: "unregistered", id: manifest.id });
        }
      };
    },

    unregister(id: LensId): void {
      if (lenses.has(id)) {
        lenses.delete(id);
        notify({ type: "unregistered", id });
      }
    },

    get(id: LensId): LensManifest | undefined {
      return lenses.get(id);
    },

    has(id: LensId): boolean {
      return lenses.has(id);
    },

    allLenses(): LensManifest[] {
      return [...lenses.values()];
    },

    getByCategory(category: LensCategory): LensManifest[] {
      return [...lenses.values()].filter((m) => m.category === category);
    },

    subscribe(listener: LensRegistryEventListener): () => void {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
