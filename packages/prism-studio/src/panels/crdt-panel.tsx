/**
 * CRDT Inspector panel — shows live state of the CollectionStore
 * and allows browsing objects/edges.
 */

import { useSyncExternalStore, useState, useRef } from "react";
import { useKernel } from "../kernel/index.js";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
export function CrdtPanel() {
  const kernel = useKernel();
  const [tab, setTab] = useState<"objects" | "edges" | "json">("objects");

  // Cache the snapshot to avoid infinite loops with useSyncExternalStore
  const cacheRef = useRef<{ data: ReturnType<typeof kernel.store.toJSON>; version: number }>({
    data: kernel.store.toJSON(),
    version: 0,
  });
  const counterRef = useRef(0);
  const version = useSyncExternalStore(
    (cb) => kernel.store.onChange(() => { counterRef.current++; cb(); }),
    () => counterRef.current,
  );
  if (cacheRef.current.version !== version) {
    cacheRef.current = { data: kernel.store.toJSON(), version };
  }
  const data = cacheRef.current.data;

  const objects = Object.values(data.objects);
  const edges = Object.values(data.edges);

  return (
    <div
      style={{
        height: "100%",
        display: "flex",
        flexDirection: "column",
      }}
    >
      <div
        style={{
          padding: "8px 12px",
          fontSize: 12,
          color: "#999",
          borderBottom: "1px solid #333",
          background: "#252526",
          display: "flex",
          gap: 8,
        }}
      >
        <span style={{ color: "#ccc" }}>Collection Inspector</span>
        <span style={{ color: "#555" }}>|</span>
        <span style={{ color: "#888" }}>
          {objects.length} objects, {edges.length} edges
        </span>
      </div>

      <div
        style={{
          display: "flex",
          gap: 0,
          borderBottom: "1px solid #333",
          background: "#2d2d2d",
        }}
      >
        {(["objects", "edges", "json"] as const).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            style={{
              padding: "6px 12px",
              fontSize: 11,
              background: tab === t ? "#1e1e1e" : "transparent",
              color: tab === t ? "#fff" : "#888",
              border: "none",
              borderBottom: tab === t ? "2px solid #007acc" : "2px solid transparent",
              cursor: "pointer",
              textTransform: "capitalize",
            }}
          >
            {t}
          </button>
        ))}
      </div>

      <div style={{ flex: 1, overflow: "auto", padding: 12 }}>
        {tab === "objects" && (
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {objects.map((obj) => (
              <div
                key={obj.id}
                onClick={() => kernel.select(obj.id)}
                style={{
                  padding: "8px 10px",
                  background: "#252526",
                  borderRadius: 4,
                  border: "1px solid #333",
                  cursor: "pointer",
                  fontSize: 12,
                }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 4 }}>
                  <span style={{ color: "#ccc", fontWeight: 500 }}>{obj.name}</span>
                  <span style={{
                    color: "#888",
                    fontSize: 10,
                    background: "#333",
                    padding: "1px 6px",
                    borderRadius: 3,
                  }}>{obj.type}</span>
                </div>
                <div style={{ color: "#666", fontSize: 10 }}>
                  {obj.id} {obj.parentId ? `\u2192 ${obj.parentId}` : "(root)"}
                </div>
              </div>
            ))}
            {objects.length === 0 && (
              <div style={{ color: "#555", fontSize: 12 }}>No objects in collection.</div>
            )}
          </div>
        )}

        {tab === "edges" && (
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {edges.map((edge) => (
              <div
                key={edge.id}
                style={{
                  padding: "8px 10px",
                  background: "#252526",
                  borderRadius: 4,
                  border: "1px solid #333",
                  fontSize: 12,
                }}
              >
                <span style={{ color: "#ccc" }}>{edge.sourceId}</span>
                <span style={{ color: "#007acc", margin: "0 6px" }}>\u2014{edge.relation}\u2192</span>
                <span style={{ color: "#ccc" }}>{edge.targetId}</span>
              </div>
            ))}
            {edges.length === 0 && (
              <div style={{ color: "#555", fontSize: 12 }}>No edges in collection.</div>
            )}
          </div>
        )}

        {tab === "json" && (
          <pre
            style={{
              background: "#1e1e1e",
              padding: 12,
              borderRadius: 4,
              fontSize: 11,
              overflow: "auto",
              whiteSpace: "pre-wrap",
              wordBreak: "break-all",
              color: "#d4d4d4",
              border: "1px solid #333",
            }}
          >
            {JSON.stringify(data, null, 2)}
          </pre>
        )}
      </div>
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const CRDT_LENS_ID = lensId("crdt");

export const crdtLensManifest: LensManifest = {

  id: CRDT_LENS_ID,
  name: "CRDT",
  icon: "\u29C9",
  category: "debug",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-crdt", name: "Switch to CRDT Inspector", shortcut: ["c"], section: "Navigation" }],
  },
};

export const crdtLensBundle: LensBundle = defineLensBundle(
  crdtLensManifest,
  CrdtPanel,
);
