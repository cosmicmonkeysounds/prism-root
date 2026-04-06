/**
 * CRDT Inspector panel — shows live state of the Loro document
 * and allows manual key-value writes.
 */

import { useState, useEffect, useCallback } from "react";
import type { CrdtStore } from "@prism/core/layer1/stores/use-crdt-store";

type StoreWithSubscribe = {
  getState: () => CrdtStore;
  subscribe: (listener: (state: CrdtStore) => void) => () => void;
};

export type CrdtPanelProps = {
  store: StoreWithSubscribe;
  fullWidth?: boolean;
};

export function CrdtPanel({ store, fullWidth }: CrdtPanelProps) {
  const [storeState, setStoreState] = useState(store.getState().data);
  const [input, setInput] = useState("");
  const [key, setKey] = useState("greeting");

  useEffect(() => {
    const unsub = store.subscribe((state) => {
      setStoreState({ ...state.data });
    });
    return unsub;
  }, [store]);

  const handleWrite = useCallback(() => {
    if (key && input) {
      store.getState().set(key, input);
      setInput("");
    }
  }, [key, input, store]);

  return (
    <div
      style={{
        height: "100%",
        display: "flex",
        flexDirection: "column",
        padding: fullWidth ? "1rem" : 0,
      }}
    >
      <div
        style={{
          padding: "8px 12px",
          fontSize: 12,
          color: "#666",
          borderBottom: "1px solid #eee",
          background: "#fafafa",
        }}
      >
        CRDT State Inspector
      </div>

      <div style={{ padding: "12px", flex: 1, overflow: "auto" }}>
        <div
          style={{
            marginBottom: "12px",
            display: "flex",
            gap: 4,
            flexWrap: "wrap",
          }}
        >
          <input
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder="key"
            style={{ padding: "4px 8px", fontSize: 13, width: 80 }}
          />
          <input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleWrite()}
            placeholder="value"
            style={{ padding: "4px 8px", fontSize: 13, flex: 1, minWidth: 80 }}
          />
          <button
            onClick={handleWrite}
            style={{ padding: "4px 8px", fontSize: 13 }}
          >
            Set
          </button>
        </div>

        <pre
          style={{
            background: "#f5f5f5",
            padding: "12px",
            borderRadius: 4,
            fontSize: 12,
            overflow: "auto",
            whiteSpace: "pre-wrap",
            wordBreak: "break-all",
          }}
        >
          {JSON.stringify(storeState, null, 2)}
        </pre>
      </div>
    </div>
  );
}
