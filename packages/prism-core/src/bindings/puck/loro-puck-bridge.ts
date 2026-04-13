/**
 * Puck ↔ Loro CRDT bridge.
 *
 * Puck never saves — it is strictly a visual manipulator.
 * The Loro CRDT is the source of truth for layout state.
 *
 * Flow:
 *   Puck onChange → extract diff → push to Loro root map
 *   Loro state → feed back into Puck `data` prop
 */

import { LoroDoc } from "loro-crdt";
import type { Data } from "@measured/puck";

const PUCK_STATE_KEY = "puck_layout";

export type PuckLoroBridge = {
  /** The underlying LoroDoc. */
  doc: LoroDoc;
  /** Get current Puck data from Loro state. */
  getData: () => Data;
  /** Push Puck data changes to Loro. Called from Puck's onChange. */
  setData: (data: Data) => void;
  /** Subscribe to Loro changes and get updated Puck data. */
  subscribe: (callback: (data: Data) => void) => () => void;
};

/** Default empty Puck data structure. */
const EMPTY_PUCK_DATA: Data = {
  content: [],
  root: { props: {} },
};

/**
 * Create a bridge between Puck and a Loro document.
 *
 * Stores the Puck layout data as a JSON string in the Loro root map.
 * This handles Puck 0.20 slot-shaped data transparently because slot
 * content is just nested `ComponentData[]` arrays inside the JSON tree —
 * the bridge doesn't inspect the shape, it only round-trips it.
 *
 * Trade-off: at the Loro layer the whole document is a single string entry,
 * so merges between peers are last-write-wins for the document as a whole
 * rather than per-component. Studio's `layout-panel` avoids that limit by
 * projecting into kernel objects directly (every block is its own Loro
 * map), and reserves this bridge for standalone / test contexts.
 */
export function createPuckLoroBridge(
  doc?: LoroDoc,
  stateKey = PUCK_STATE_KEY,
): PuckLoroBridge {
  const loroDoc = doc ?? new LoroDoc();
  const root = loroDoc.getMap("root");

  function getData(): Data {
    const raw = root.get(stateKey);
    if (typeof raw === "string") {
      try {
        return JSON.parse(raw) as Data;
      } catch {
        return EMPTY_PUCK_DATA;
      }
    }
    return EMPTY_PUCK_DATA;
  }

  function setData(data: Data): void {
    root.set(stateKey, JSON.stringify(data));
    loroDoc.commit();
  }

  function subscribe(callback: (data: Data) => void): () => void {
    const sub = loroDoc.subscribe((event) => {
      for (const e of event.events) {
        if (e.diff.type === "map" && stateKey in e.diff.updated) {
          callback(getData());
        }
      }
    });
    return () => sub();
  }

  return { doc: loroDoc, getData, setData, subscribe };
}
