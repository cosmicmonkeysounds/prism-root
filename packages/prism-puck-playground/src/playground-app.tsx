import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  useSyncExternalStore,
} from "react";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import {
  createStudioKernel,
  KernelProvider,
  useKernel,
  useSelection,
} from "@prism/studio/kernel/index.js";
import type { StudioKernel } from "@prism/studio/kernel/index.js";
import { NotificationToast } from "@prism/studio/components/notification-toast.js";
import {
  LayoutPanel,
  layoutLensBundle,
} from "@prism/studio/panels/layout-panel.js";
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

  const handleReset = useCallback(() => {
    kernel.dispose();
    setKernel(createKernel());
    setResetKey((k) => k + 1);
  }, [kernel]);

  return (
    <KernelProvider kernel={kernel} key={resetKey}>
      <Chrome onReset={handleReset} />
      <NotificationToast />
    </KernelProvider>
  );
}

function Chrome({ onReset }: { onReset: () => void }) {
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "260px 1fr",
        height: "100vh",
        width: "100%",
        color: "#e2e8f0",
        background: "#0b1020",
        fontFamily:
          "system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif",
      }}
    >
      <Sidebar />
      <main style={{ position: "relative", overflow: "hidden" }}>
        <LayoutPanel />
      </main>
      <ResetButton onClick={onReset} />
    </div>
  );
}

// ── Sidebar: app switcher + per-app route list ────────────────────────────

function useStoreVersion(kernel: StudioKernel): number {
  return useSyncExternalStore(
    (cb) => kernel.store.onChange(() => cb()),
    () => kernel.store.allObjects().length,
  );
}

function Sidebar() {
  const kernel = useKernel();
  const storeVersion = useStoreVersion(kernel);
  const { selectedId } = useSelection();

  const apps = useMemo<GraphObject[]>(() => {
    void storeVersion;
    return kernel.store
      .allObjects()
      .filter((o) => o.type === "app" && !o.deletedAt)
      .sort((a, b) => a.position - b.position);
  }, [kernel, storeVersion]);

  const activeAppId = useMemo<string | null>(() => {
    const fromSelection = selectedId
      ? findEnclosingApp(kernel, selectedId as ObjectId)
      : null;
    if (fromSelection) return fromSelection;
    return (apps[0]?.id as unknown as string) ?? null;
  }, [apps, kernel, selectedId]);

  const routes = useMemo<RouteRow[]>(() => {
    void storeVersion;
    if (!activeAppId) return [];
    return collectRoutes(kernel, activeAppId);
  }, [kernel, activeAppId, storeVersion]);

  const selectApp = useCallback(
    (appId: string) => {
      const homePageId = resolveHomePageId(kernel, appId);
      if (homePageId) kernel.select(homePageId as ObjectId);
    },
    [kernel],
  );

  const selectRoute = useCallback(
    (row: RouteRow) => {
      if (row.pageId) kernel.select(row.pageId as ObjectId);
      else kernel.select(row.id as ObjectId);
    },
    [kernel],
  );

  const activePageId = useMemo<string | null>(() => {
    if (!selectedId) return null;
    const page = walkToPage(kernel, selectedId as ObjectId);
    return page ? (page.id as unknown as string) : null;
  }, [kernel, selectedId]);

  // If the active app has no page selected yet (e.g. seed selected the home
  // route, not its page), auto-promote the selection to the home page so the
  // Layout panel has something to render.
  useEffect(() => {
    if (!activeAppId || activePageId) return;
    const homePageId = resolveHomePageId(kernel, activeAppId);
    if (homePageId) kernel.select(homePageId as ObjectId);
  }, [kernel, activeAppId, activePageId]);

  return (
    <aside
      data-testid="playground-sidebar"
      style={{
        display: "flex",
        flexDirection: "column",
        borderRight: "1px solid #1e293b",
        background: "#0f172a",
        minWidth: 0,
      }}
    >
      <div
        style={{
          padding: "14px 14px 10px",
          borderBottom: "1px solid #1e293b",
        }}
      >
        <div style={sectionLabelStyle}>Apps</div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
          {apps.map((app) => {
            const id = app.id as unknown as string;
            const active = id === activeAppId;
            return (
              <button
                key={id}
                data-testid={`playground-app-${id}`}
                onClick={() => selectApp(id)}
                style={chipStyle(active)}
              >
                {app.name}
              </button>
            );
          })}
          {apps.length === 0 && (
            <span style={{ color: "#64748b", fontSize: 11 }}>
              No apps in workspace.
            </span>
          )}
        </div>
      </div>

      <div
        style={{
          padding: "12px 10px 6px",
          color: "#64748b",
        }}
      >
        <div style={sectionLabelStyle}>Pages</div>
      </div>
      <nav
        data-testid="playground-page-list"
        style={{ flex: 1, overflow: "auto", padding: "0 8px 14px" }}
      >
        {routes.length === 0 && (
          <div
            style={{
              color: "#64748b",
              fontSize: 11,
              padding: "6px 8px",
              fontStyle: "italic",
            }}
          >
            No pages in this app.
          </div>
        )}
        {routes.map((row) => {
          const active = row.pageId === activePageId;
          return (
            <button
              key={row.id}
              data-testid={`playground-page-${row.id}`}
              onClick={() => selectRoute(row)}
              style={pageRowStyle(active)}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {row.label}
                </span>
                {row.isHome && <span style={homeBadge}>HOME</span>}
              </div>
              {row.path && (
                <div
                  style={{
                    fontSize: 10,
                    color: active ? "#bfdbfe" : "#64748b",
                    fontFamily: "ui-monospace, Menlo, monospace",
                    marginTop: 2,
                  }}
                >
                  {row.path}
                </div>
              )}
            </button>
          );
        })}
      </nav>
    </aside>
  );
}

// ── Kernel queries (pure, no React) ───────────────────────────────────────

interface RouteRow {
  /** Route object id. */
  id: string;
  label: string;
  path: string;
  pageId: string | null;
  isHome: boolean;
}

function findEnclosingApp(kernel: StudioKernel, id: ObjectId): string | null {
  let cursor: GraphObject | undefined = kernel.store.getObject(id);
  const guard = new Set<string>();
  while (cursor) {
    const cid = cursor.id as unknown as string;
    if (guard.has(cid)) return null;
    guard.add(cid);
    if (cursor.type === "app") return cid;
    if (!cursor.parentId) return null;
    cursor = kernel.store.getObject(cursor.parentId);
  }
  return null;
}

function walkToPage(
  kernel: StudioKernel,
  id: ObjectId,
): GraphObject | undefined {
  let cursor: GraphObject | undefined = kernel.store.getObject(id);
  const guard = new Set<string>();
  while (cursor) {
    const cid = cursor.id as unknown as string;
    if (guard.has(cid)) return undefined;
    guard.add(cid);
    if (cursor.type === "page") return cursor;
    if (!cursor.parentId) return undefined;
    cursor = kernel.store.getObject(cursor.parentId);
  }
  return undefined;
}

function collectRoutes(kernel: StudioKernel, appId: string): RouteRow[] {
  const all = kernel.store.allObjects();
  const app = kernel.store.getObject(appId as ObjectId);
  const homeRouteId =
    app && typeof (app.data as Record<string, unknown>)["homeRouteId"] === "string"
      ? ((app.data as Record<string, unknown>)["homeRouteId"] as string)
      : null;

  const routes = all
    .filter(
      (o) =>
        o.type === "route" &&
        !o.deletedAt &&
        (o.parentId as unknown as string) === appId,
    )
    .sort((a, b) => a.position - b.position);

  if (routes.length > 0) {
    return routes.map((r) => {
      const data = r.data as Record<string, unknown>;
      const pageId =
        typeof data["pageId"] === "string" ? (data["pageId"] as string) : null;
      return {
        id: r.id as unknown as string,
        label: (data["label"] as string) || r.name,
        path: (data["path"] as string) || "",
        pageId,
        isHome: homeRouteId === (r.id as unknown as string),
      };
    });
  }

  // Fallback for app workspaces that only have pages (no routes yet).
  return all
    .filter(
      (o) =>
        o.type === "page" &&
        !o.deletedAt &&
        (o.parentId as unknown as string) === appId,
    )
    .sort((a, b) => a.position - b.position)
    .map((p) => {
      const data = p.data as Record<string, unknown>;
      return {
        id: p.id as unknown as string,
        label: (data["title"] as string) || p.name,
        path: (data["slug"] as string) || "",
        pageId: p.id as unknown as string,
        isHome: Boolean(data["isHome"]),
      };
    });
}

function resolveHomePageId(
  kernel: StudioKernel,
  appId: string,
): string | null {
  const app = kernel.store.getObject(appId as ObjectId);
  const homeRouteId =
    app && typeof (app.data as Record<string, unknown>)["homeRouteId"] === "string"
      ? ((app.data as Record<string, unknown>)["homeRouteId"] as string)
      : null;

  if (homeRouteId) {
    const route = kernel.store.getObject(homeRouteId as ObjectId);
    const rdata = route?.data as Record<string, unknown> | undefined;
    const pageId = rdata?.["pageId"];
    if (typeof pageId === "string") return pageId;
  }

  // Fallback: first route/page under the app.
  const routes = collectRoutes(kernel, appId);
  return routes[0]?.pageId ?? routes[0]?.id ?? null;
}

// ── Styles ─────────────────────────────────────────────────────────────────

const sectionLabelStyle: React.CSSProperties = {
  fontSize: 10,
  textTransform: "uppercase",
  letterSpacing: "0.08em",
  color: "#64748b",
  marginBottom: 8,
  fontWeight: 600,
};

function chipStyle(active: boolean): React.CSSProperties {
  return {
    padding: "4px 10px",
    fontSize: 12,
    fontWeight: 500,
    background: active ? "#2563eb" : "#1e293b",
    color: active ? "#f8fafc" : "#cbd5f5",
    border: `1px solid ${active ? "#3b82f6" : "#334155"}`,
    borderRadius: 999,
    cursor: "pointer",
  };
}

function pageRowStyle(active: boolean): React.CSSProperties {
  return {
    display: "block",
    width: "100%",
    textAlign: "left",
    padding: "8px 10px",
    marginBottom: 2,
    background: active ? "#1e3a8a" : "transparent",
    border: `1px solid ${active ? "#2563eb" : "transparent"}`,
    borderRadius: 6,
    color: active ? "#f8fafc" : "#cbd5f5",
    fontSize: 12,
    cursor: "pointer",
  };
}

const homeBadge: React.CSSProperties = {
  fontSize: 9,
  fontWeight: 700,
  color: "#4ade80",
  border: "1px solid #166534",
  borderRadius: 4,
  padding: "0 4px",
  letterSpacing: "0.05em",
};

// ── Reset button ───────────────────────────────────────────────────────────

function ResetButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      data-testid="playground-reset"
      style={{
        position: "fixed",
        bottom: 12,
        right: 12,
        background: "#1e293b",
        color: "#e2e8f0",
        border: "1px solid #334155",
        borderRadius: 6,
        padding: "6px 12px",
        fontSize: 12,
        cursor: "pointer",
        zIndex: 1000,
      }}
    >
      Reset workspace
    </button>
  );
}
