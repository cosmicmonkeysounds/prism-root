/**
 * Loro CRDT bridge — wraps LoroDoc operations for the Prism object graph.
 * This is the single source of truth. All editors project Loro state.
 */

import { LoroDoc } from "loro-crdt";
import type { VersionVector } from "loro-crdt";
import type { CrdtSnapshot, CrdtUpdate } from "@prism/shared/types";

export type LoroChangeHandler = (key: string, value: unknown) => void;

/**
 * Creates and manages a LoroDoc instance for a single document.
 * Provides typed read/write operations and change subscriptions.
 */
export function createLoroBridge(peerId?: bigint) {
  const doc = new LoroDoc();
  if (peerId !== undefined) {
    doc.setPeerId(peerId);
  }
  const root = doc.getMap("root");

  const listeners = new Set<LoroChangeHandler>();

  /** Subscribe to the doc for change events */
  doc.subscribe((event) => {
    // Notify all listeners of changes
    for (const e of event.events) {
      if (e.diff.type === "map") {
        for (const [key, val] of Object.entries(e.diff.updated)) {
          if (val !== undefined) {
            for (const handler of listeners) {
              handler(key, val);
            }
          }
        }
      }
    }
  });

  return {
    /** The underlying LoroDoc instance. */
    doc,

    /** The root LoroMap. */
    root,

    /** Write a string value to the root map. */
    set(key: string, value: string): void {
      root.set(key, value);
      doc.commit();
    },

    /** Read a value from the root map. */
    get(key: string): unknown {
      return root.get(key);
    },

    /** Delete a key from the root map. */
    delete(key: string): void {
      root.delete(key);
      doc.commit();
    },

    /** Export the full document state as a snapshot. */
    exportSnapshot(): CrdtSnapshot {
      return doc.export({ mode: "snapshot" });
    },

    /** Export only updates since the given version. */
    exportUpdate(since?: VersionVector): CrdtUpdate {
      if (since !== undefined) {
        return doc.export({ mode: "update", from: since });
      }
      return doc.export({ mode: "update" });
    },

    /** Import a snapshot or update from another peer. */
    import(data: CrdtSnapshot | CrdtUpdate): void {
      doc.import(data);
    },

    /** Subscribe to changes on the root map. */
    onChange(handler: LoroChangeHandler): () => void {
      listeners.add(handler);
      return () => {
        listeners.delete(handler);
      };
    },

    /** Get all entries in the root map as a plain object. */
    toJSON(): Record<string, unknown> {
      return root.toJSON() as Record<string, unknown>;
    },
  };
}

export type LoroBridge = ReturnType<typeof createLoroBridge>;
