/**
 * React hook for connecting Puck to Loro CRDT state.
 * Provides the data prop and onChange handler for the Puck editor.
 */

import { useEffect, useState, useCallback, useRef } from "react";
import type { Data } from "@measured/puck";
import type { PuckLoroBridge } from "./loro-puck-bridge.js";

export type UsePuckLoroOptions = {
  bridge: PuckLoroBridge;
};

/**
 * Hook that provides reactive Puck data backed by Loro CRDT.
 *
 * ```tsx
 * const bridge = createPuckLoroBridge(doc);
 * function MyBuilder() {
 *   const { data, onChange } = usePuckLoro({ bridge });
 *   return <Puck data={data} onPublish={onChange} />;
 * }
 * ```
 */
export function usePuckLoro(options: UsePuckLoroOptions) {
  const { bridge } = options;
  const [data, setData] = useState<Data>(() => bridge.getData());
  const bridgeRef = useRef(bridge);
  bridgeRef.current = bridge;

  useEffect(() => {
    // Hydrate on connect
    setData(bridge.getData());

    // Subscribe to Loro changes (e.g., from other peers/tabs)
    const unsub = bridge.subscribe((newData) => {
      setData(newData);
    });

    return unsub;
  }, [bridge]);

  /** Handler for Puck's onChange — pushes to Loro. */
  const onChange = useCallback((newData: Data) => {
    bridgeRef.current.setData(newData);
    setData(newData);
  }, []);

  return { data, onChange };
}
