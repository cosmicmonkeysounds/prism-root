/**
 * Collection Host module — hosts CRDT collections on the Relay for remote sync.
 *
 * Wraps createCollectionStore from the persistence layer, allowing clients
 * to create, retrieve, snapshot, and import collections via the relay.
 */

import type { RelayModule, RelayContext, CollectionHost } from "./relay-types.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import { createCollectionStore } from "../persistence/collection-store.js";
import type { CollectionStore } from "../persistence/collection-store.js";

export function collectionHostModule(): RelayModule {
  return {
    name: "collection-host",
    description: "Hosts CRDT collections for remote sync",
    dependencies: [],

    install(ctx: RelayContext): void {
      const collections = new Map<string, CollectionStore>();

      const host: CollectionHost = {
        create(id: string): CollectionStore {
          const existing = collections.get(id);
          if (existing) return existing;
          const store = createCollectionStore();
          collections.set(id, store);
          return store;
        },

        get(id: string): CollectionStore | undefined {
          return collections.get(id);
        },

        list(): string[] {
          return [...collections.keys()];
        },

        remove(id: string): boolean {
          return collections.delete(id);
        },
      };

      ctx.setCapability(RELAY_CAPABILITIES.COLLECTIONS, host);
    },
  };
}
