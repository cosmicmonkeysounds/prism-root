import { useCallback, useEffect, useMemo, useState } from "react";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import {
  createStudioKernel,
  KernelProvider,
  useKernel,
  useObjects,
  useSelection,
} from "@prism/studio/kernel/index.js";
import type { StudioKernel } from "@prism/studio/kernel/index.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { LayoutPanel, layoutLensBundle } from "@prism/studio/panels/layout-panel.js";
import {
  SitemapPanel,
  sitemapLensBundle,
} from "@prism/studio/panels/sitemap-panel.js";
import {
  BehaviorPanel,
  behaviorLensBundle,
} from "@prism/studio/panels/behavior-panel.js";
import { playgroundSeedInitializer } from "./playground-seed.js";

type ModeTab = "app-shell" | "sitemap" | "page" | "behaviors";

const TABS: ReadonlyArray<{ id: ModeTab; label: string }> = [
  { id: "app-shell", label: "App Shell" },
  { id: "sitemap", label: "Sitemap" },
  { id: "page", label: "Page" },
  { id: "behaviors", label: "Behaviors" },
];

function createKernel(): StudioKernel {
  return createStudioKernel({
    lensBundles: [layoutLensBundle, sitemapLensBundle, behaviorLensBundle],
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
  const [mode, setMode] = useState<ModeTab>("page");
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
      <div style={{ minHeight: 0, minWidth: 0 }}>
        <PanelGroup direction="horizontal" autoSaveId="playground-shell-split">
          <Panel defaultSize={20} minSize={12} maxSize={40}>
            <AppTreeSidebar mode={mode} />
          </Panel>
          <PanelResizeHandle
            style={{ width: 4, background: "#1e293b", cursor: "col-resize" }}
          />
          <Panel minSize={30}>
            <div
              style={{
                display: "grid",
                gridTemplateRows: "36px 1fr",
                height: "100%",
                minHeight: 0,
                minWidth: 0,
              }}
            >
              <ModeTabs mode={mode} onChange={setMode} />
              <div style={{ minHeight: 0, overflow: "hidden" }}>
                <ModePanel mode={mode} />
              </div>
            </div>
          </Panel>
        </PanelGroup>
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
          app builder harness
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

function ModeTabs({
  mode,
  onChange,
}: {
  mode: ModeTab;
  onChange: (m: ModeTab) => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "stretch",
        borderBottom: "1px solid #1e293b",
        background: "#0f172a",
      }}
    >
      {TABS.map((t) => {
        const active = t.id === mode;
        return (
          <button
            key={t.id}
            onClick={() => onChange(t.id)}
            data-testid={`playground-mode-${t.id}`}
            style={{
              background: active ? "#1e293b" : "transparent",
              color: active ? "#f1f5f9" : "#94a3b8",
              border: "none",
              borderBottom: active
                ? "2px solid #a855f7"
                : "2px solid transparent",
              padding: "0 16px",
              fontSize: 12,
              letterSpacing: 0.3,
              cursor: "pointer",
            }}
          >
            {t.label}
          </button>
        );
      })}
    </div>
  );
}

function ModePanel({ mode }: { mode: ModeTab }) {
  const kernel = useKernel();
  const allObjects = useObjects();
  const { selectedId } = useSelection();

  // Resolve the enclosing app for whatever is currently selected.
  const appId = useMemo(() => resolveAppId(allObjects, selectedId), [
    allObjects,
    selectedId,
  ]);

  // For the App Shell tab, force-select the app's app-shell child so
  // LayoutPanel renders it instead of whatever was previously active.
  useEffect(() => {
    if (mode !== "app-shell" || !appId) return;
    const shell = allObjects.find(
      (o) => o.type === "app-shell" && o.parentId === appId,
    );
    if (shell && shell.id !== selectedId) {
      kernel.select(shell.id);
    }
  }, [mode, appId, allObjects, selectedId, kernel]);

  if (mode === "sitemap") return <SitemapPanel />;
  if (mode === "behaviors") return <BehaviorPanel />;
  return <LayoutPanel />;
}

function resolveAppId(
  objects: readonly GraphObject[],
  selectedId: ObjectId | null,
): ObjectId | null {
  if (!selectedId) return null;
  const byId = new Map<ObjectId, GraphObject>();
  for (const o of objects) byId.set(o.id, o);
  let cursor: GraphObject | undefined = byId.get(selectedId);
  while (cursor) {
    if (cursor.type === "app") return cursor.id;
    if (!cursor.parentId) return null;
    cursor = byId.get(cursor.parentId);
  }
  return null;
}

function AppTreeSidebar({ mode: _mode }: { mode: ModeTab }) {
  const kernel = useKernel();
  const allObjects = useObjects();
  const { selectedId } = useSelection();

  const apps = useMemo(
    () =>
      allObjects
        .filter((o) => o.type === "app")
        .sort((a, b) => (a.position ?? 0) - (b.position ?? 0)),
    [allObjects],
  );

  const childrenByParent = useMemo(() => {
    const map = new Map<string, GraphObject[]>();
    for (const o of allObjects) {
      if (!o.parentId) continue;
      const key = o.parentId as unknown as string;
      const arr = map.get(key) ?? [];
      arr.push(o);
      map.set(key, arr);
    }
    for (const arr of map.values()) {
      arr.sort((a, b) => (a.position ?? 0) - (b.position ?? 0));
    }
    return map;
  }, [allObjects]);

  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const toggle = useCallback(
    (id: string) =>
      setExpanded((prev) => ({ ...prev, [id]: !(prev[id] ?? true) })),
    [],
  );

  const activeAppId = useMemo(
    () => resolveAppId(allObjects, selectedId),
    [allObjects, selectedId],
  );

  const select = (id: ObjectId) => kernel.select(id);

  return (
    <aside
      style={{
        height: "100%",
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
        Starter apps
      </div>
      {apps.length === 0 && (
        <div
          style={{
            padding: "8px 16px",
            fontSize: 12,
            color: "#475569",
            fontStyle: "italic",
          }}
        >
          No apps seeded.
        </div>
      )}
      {apps.map((app) => {
        const appKey = app.id as unknown as string;
        const isOpen = expanded[appKey] ?? true;
        const children = childrenByParent.get(appKey) ?? [];
        const routes = children.filter((c) => c.type === "route");
        const pages = children.filter((c) => c.type === "page");
        const shell = children.find((c) => c.type === "app-shell");
        const isActive = activeAppId === app.id;
        return (
          <div key={appKey} style={{ marginBottom: 6 }}>
            <button
              onClick={() => {
                toggle(appKey);
                select(app.id);
              }}
              data-testid={`app-row-${appKey}`}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
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
                fontWeight: 600,
                cursor: "pointer",
              }}
            >
              <span style={{ width: 10, color: "#64748b" }}>
                {isOpen ? "▾" : "▸"}
              </span>
              {app.name}
            </button>
            {isOpen && (
              <div style={{ paddingLeft: 22 }}>
                {shell && (
                  <TreeRow
                    label={shell.name}
                    hint="app-shell"
                    active={selectedId === shell.id}
                    onClick={() => select(shell.id)}
                  />
                )}
                {routes.length > 0 && (
                  <TreeHeading label="Routes" />
                )}
                {routes.map((route) => {
                  const pageIdRaw = (route.data as Record<string, unknown>)[
                    "pageId"
                  ];
                  const pageId =
                    typeof pageIdRaw === "string" ? pageIdRaw : null;
                  const page = pageId
                    ? allObjects.find((o) => (o.id as unknown as string) === pageId)
                    : undefined;
                  const routeLabel =
                    ((route.data as Record<string, unknown>)["label"] as string) ??
                    route.name;
                  const path =
                    ((route.data as Record<string, unknown>)["path"] as string) ??
                    "/";
                  return (
                    <div key={route.id as unknown as string}>
                      <TreeRow
                        label={`${routeLabel}`}
                        hint={path}
                        active={selectedId === route.id}
                        onClick={() => select(route.id)}
                      />
                      {page && (
                        <div style={{ paddingLeft: 14 }}>
                          <TreeRow
                            label={page.name}
                            hint="page"
                            active={selectedId === page.id}
                            onClick={() => select(page.id)}
                          />
                        </div>
                      )}
                    </div>
                  );
                })}
                {pages.filter(
                  (p) =>
                    !routes.some(
                      (r) =>
                        ((r.data as Record<string, unknown>)["pageId"] as string) ===
                        (p.id as unknown as string),
                    ),
                ).length > 0 && <TreeHeading label="Orphan pages" />}
                {pages
                  .filter(
                    (p) =>
                      !routes.some(
                        (r) =>
                          ((r.data as Record<string, unknown>)[
                            "pageId"
                          ] as string) === (p.id as unknown as string),
                      ),
                  )
                  .map((page) => (
                    <TreeRow
                      key={page.id as unknown as string}
                      label={page.name}
                      hint="page"
                      active={selectedId === page.id}
                      onClick={() => select(page.id)}
                    />
                  ))}
              </div>
            )}
          </div>
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
        Click an app to expand its shell + routes, then use the mode tabs to
        edit App Shell chrome, navigate the Sitemap, design a Page, or attach
        Behaviors to the selected block.
      </div>
    </aside>
  );
}

function TreeHeading({ label }: { label: string }) {
  return (
    <div
      style={{
        padding: "8px 16px 2px",
        fontSize: 10,
        textTransform: "uppercase",
        letterSpacing: 0.6,
        color: "#475569",
      }}
    >
      {label}
    </div>
  );
}

function TreeRow({
  label,
  hint,
  active,
  onClick,
}: {
  label: string;
  hint?: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        width: "100%",
        textAlign: "left",
        padding: "5px 16px",
        background: active ? "#1e293b" : "transparent",
        color: active ? "#f1f5f9" : "#94a3b8",
        border: "none",
        borderLeft: active ? "2px solid #a855f7" : "2px solid transparent",
        fontSize: 12,
        cursor: "pointer",
      }}
    >
      <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>
        {label}
      </span>
      {hint && (
        <span style={{ color: "#475569", fontSize: 10, marginLeft: 8 }}>
          {hint}
        </span>
      )}
    </button>
  );
}
