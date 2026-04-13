import { useMemo, useState, useEffect } from "react";
import {
  createStudioKernel,
  KernelProvider,
  useKernel,
  useObjects,
  useSelection,
} from "@prism/studio/kernel/index.js";
import type { StudioKernel } from "@prism/studio/kernel/index.js";
import { LayoutPanel, layoutLensBundle } from "@prism/studio/panels/layout-panel.js";
import { playgroundSeedInitializer } from "./playground-seed.js";

function createKernel(): StudioKernel {
  return createStudioKernel({
    lensBundles: [layoutLensBundle],
    initializers: [playgroundSeedInitializer],
  });
}

export function PlaygroundApp() {
  const [kernel, setKernel] = useState<StudioKernel>(() => createKernel());
  const [resetKey, setResetKey] = useState(0);

  useEffect(() => () => kernel.dispose(), [kernel]);

  const handleReset = () => {
    kernel.dispose();
    setKernel(createKernel());
    setResetKey((k) => k + 1);
  };

  return (
    <KernelProvider kernel={kernel} key={resetKey}>
      <Shell onReset={handleReset} />
    </KernelProvider>
  );
}

function Shell({ onReset }: { onReset: () => void }) {
  return (
    <div
      style={{
        display: "grid",
        gridTemplateRows: "48px 1fr",
        height: "100vh",
        width: "100vw",
        background: "#0b1020",
        color: "#e2e8f0",
      }}
    >
      <Header onReset={onReset} />
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "240px 1fr",
          minHeight: 0,
        }}
      >
        <PageSidebar />
        <div style={{ minHeight: 0, minWidth: 0, overflow: "hidden" }}>
          <LayoutPanel />
        </div>
      </div>
    </div>
  );
}

function Header({ onReset }: { onReset: () => void }) {
  return (
    <header
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "0 16px",
        borderBottom: "1px solid #1e293b",
        background: "#0f172a",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span style={{ fontSize: 18 }}>▦</span>
        <strong style={{ fontSize: 14, letterSpacing: 0.3 }}>
          Prism Puck Playground
        </strong>
        <span style={{ fontSize: 12, color: "#64748b" }}>
          standalone builder harness
        </span>
      </div>
      <button
        onClick={onReset}
        style={{
          background: "#1e293b",
          color: "#e2e8f0",
          border: "1px solid #334155",
          borderRadius: 6,
          padding: "6px 12px",
          fontSize: 12,
          cursor: "pointer",
        }}
      >
        Reset workspace
      </button>
    </header>
  );
}

function PageSidebar() {
  const kernel = useKernel();
  const allObjects = useObjects();
  const { selectedId } = useSelection();

  const pages = useMemo(
    () =>
      allObjects
        .filter((o) => o.type === "page")
        .sort((a, b) => (a.position ?? 0) - (b.position ?? 0)),
    [allObjects],
  );

  const activePageId = useMemo(() => {
    if (!selectedId) return null;
    let cur = allObjects.find((o) => o.id === selectedId) ?? null;
    while (cur && cur.type !== "page") {
      const parentId = cur.parentId;
      cur = parentId ? allObjects.find((o) => o.id === parentId) ?? null : null;
    }
    return cur?.id ?? null;
  }, [allObjects, selectedId]);

  return (
    <aside
      style={{
        borderRight: "1px solid #1e293b",
        background: "#0f172a",
        padding: "12px 0",
        overflowY: "auto",
      }}
    >
      <div
        style={{
          padding: "4px 16px 10px",
          fontSize: 11,
          textTransform: "uppercase",
          letterSpacing: 0.8,
          color: "#64748b",
        }}
      >
        Demo pages
      </div>
      {pages.map((page) => {
        const isActive = page.id === activePageId;
        return (
          <button
            key={page.id}
            onClick={() => kernel.select(page.id)}
            style={{
              display: "block",
              width: "100%",
              textAlign: "left",
              padding: "8px 16px",
              background: isActive ? "#1e293b" : "transparent",
              color: isActive ? "#f1f5f9" : "#cbd5e1",
              border: "none",
              borderLeft: isActive
                ? "2px solid #a855f7"
                : "2px solid transparent",
              fontSize: 13,
              cursor: "pointer",
            }}
          >
            {page.name}
          </button>
        );
      })}
      <div
        style={{
          marginTop: 16,
          padding: "8px 16px",
          fontSize: 11,
          color: "#475569",
          lineHeight: 1.5,
        }}
      >
        Click a page, then drag Puck widgets from the left Puck panel onto the
        canvas. All changes sync back to the Prism kernel.
      </div>
    </aside>
  );
}
