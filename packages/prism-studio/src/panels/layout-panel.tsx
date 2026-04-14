/**
 * Layout Panel — Puck visual builder wired to the Studio kernel.
 *
 * Instead of maintaining its own isolated Loro doc, this panel:
 *   1. Generates the Puck Config dynamically from ObjectRegistry entity defs
 *   2. Projects kernel objects (children of the selected page) into Puck Data
 *   3. Diffs Puck onChange back into kernel CRUD operations
 *
 * The kernel CollectionStore (Loro CRDT) remains the source of truth.
 */

import {
  useState,
  useEffect,
  useCallback,
  useMemo,
  useRef,
  type ReactNode,
} from "react";
import { Puck, type Config, type Data, type ComponentConfig } from "@measured/puck";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { objectId } from "@prism/core/object-model";
import {
  getShellSlots,
  pascalToKebab,
  kernelToPuckData,
  buildPuckCategories,
  splitRootProps,
  PAGE_SLOTS,
} from "./layout-panel-data.js";

export {
  SHELL_SLOTS,
  PAGE_SLOTS,
  getShellSlots,
  isShellType,
  kernelToPuckData,
  splitRootProps,
  COMPONENT_CATEGORY_MAP,
  CATEGORY_TITLES,
  buildPuckCategories,
} from "./layout-panel-data.js";
import { useKernel, useSelection } from "../kernel/index.js";
import { buildPuckRootConfig } from "../kernel/entity-puck-config.js";
import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import {
  HelpProvider,
  HelpTooltip,
  HelpRegistry,
  DocSheet,
  DocSearch,
} from "@prism/core/help";
import "./puck-help-entries.js";
import { fetchHelpDoc } from "./help-docs/index.js";

// ── Styles ──────────────────────────────────────────────────────────────────

const emptyStyle = {
  height: "100%",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  background: "#1e1e1e",
  color: "#888",
  fontFamily: "system-ui, sans-serif",
} as const;

// ── Puck Data → Kernel diff ────────────────────────────────────────────────

type KernelSync = {
  store: {
    listObjects(opts: { parentId: ObjectId }): GraphObject[];
    allObjects(): GraphObject[];
    getObject(id: ObjectId): GraphObject | undefined;
  };
  createObject(
    obj: Omit<GraphObject, "id" | "createdAt" | "updatedAt">,
  ): GraphObject;
  updateObject(id: ObjectId, patch: Partial<GraphObject>): GraphObject | undefined;
  deleteObject(id: ObjectId): boolean;
};

/**
 * Diff Puck data against the current kernel state and apply CRUD ops.
 *
 * Walks Puck content recursively: slots become nested kernel children
 * tagged with `data.__slot`, and any existing kernel object under the page
 * that isn't referenced by the new tree is deleted. `newData.root.props`
 * is split into scalar page fields (written to `page.data`) and per-slot
 * child arrays (written as slotted page children) so the page entity can
 * own its sidebar/header/footer regions directly — no PageShell wrapper
 * required. This is the critical sync path — Puck edits → kernel mutations.
 */
function syncPuckToKernel(
  newData: Data,
  pageId: ObjectId,
  kernel: KernelSync,
) {
  const allObjs = kernel.store.allObjects().filter((o) => !o.deletedAt);
  const existingById = new Map(allObjs.map((o) => [o.id, o]));
  const seen = new Set<string>();

  // 1. Main content (non-slotted page children)
  syncContentArray(newData.content, pageId, null, existingById, seen, kernel);

  // 2. Root props → page.data (scalars) + page slot children (arrays)
  const rootProps = (newData.root?.props ?? {}) as Record<string, unknown>;
  const { pageData, slots } = splitRootProps(rootProps);
  const page = kernel.store.getObject(pageId);
  if (page) {
    const prevData = (page.data ?? {}) as Record<string, unknown>;
    const slot = prevData["__slot"];
    const mergedData: Record<string, unknown> = { ...pageData };
    if (typeof slot === "string") mergedData["__slot"] = slot;
    kernel.updateObject(pageId, { data: mergedData });
  }
  for (const slotName of PAGE_SLOTS) {
    const items = slots[slotName] ?? [];
    syncContentArray(items, pageId, slotName, existingById, seen, kernel);
  }

  // Delete any descendants of this page that the new tree didn't mention.
  for (const obj of collectDescendants(pageId, allObjs)) {
    if (!seen.has(obj.id)) kernel.deleteObject(objectId(obj.id));
  }
}

function syncContentArray(
  items: Data["content"],
  parentId: ObjectId,
  slotName: string | null,
  existingById: Map<string, GraphObject>,
  seen: Set<string>,
  kernel: KernelSync,
) {
  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    if (!item) continue;
    const rawProps = (item.props ?? {}) as Record<string, unknown>;
    const id = typeof rawProps["id"] === "string" ? (rawProps["id"] as string) : undefined;
    const kernelType = pascalToKebab(item.type);
    const slotNames = getShellSlots(kernelType);

    const dataProps: Record<string, unknown> = {};
    const slotData: Record<string, Data["content"]> = {};
    for (const [k, v] of Object.entries(rawProps)) {
      if (k === "id") continue;
      if (slotNames.includes(k)) {
        slotData[k] = Array.isArray(v) ? (v as Data["content"]) : [];
        continue;
      }
      dataProps[k] = v;
    }
    if (slotName) dataProps["__slot"] = slotName;

    let objId: ObjectId;
    const oid = id ? objectId(id) : null;
    if (oid && existingById.has(oid)) {
      kernel.updateObject(oid, {
        position: i,
        parentId,
        data: dataProps,
      });
      objId = oid;
    } else {
      const newObj = kernel.createObject({
        type: kernelType,
        name:
          (dataProps["text"] as string) ??
          (dataProps["title"] as string) ??
          (dataProps["label"] as string) ??
          `New ${kernelType}`,
        parentId,
        position: i,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: dataProps,
      });
      objId = newObj.id;
      existingById.set(objId, newObj);
    }
    seen.add(objId);

    for (const slot of slotNames) {
      const children = slotData[slot] ?? [];
      syncContentArray(children, objId, slot, existingById, seen, kernel);
    }
  }
}

function collectDescendants(
  rootId: ObjectId,
  allObjs: GraphObject[],
): GraphObject[] {
  const byParent = new Map<string, GraphObject[]>();
  for (const o of allObjs) {
    const arr = byParent.get(o.parentId ?? "") ?? [];
    arr.push(o);
    byParent.set(o.parentId ?? "", arr);
  }
  const out: GraphObject[] = [];
  const stack: string[] = [rootId];
  while (stack.length > 0) {
    const parent = stack.pop();
    if (parent === undefined) break;
    const kids = byParent.get(parent) ?? [];
    for (const k of kids) {
      out.push(k);
      stack.push(k.id);
    }
  }
  return out;
}

// ── Hook: resolve selected page ────────────────────────────────────────────

function useResolvePage(): GraphObject | undefined {
  const kernel = useKernel();
  const { selectedId } = useSelection();

  return useMemo(() => {
    if (!selectedId) return undefined;
    const obj = kernel.store.getObject(selectedId);
    if (!obj || obj.deletedAt) return undefined;
    if (obj.type === "page") return obj;

    // Walk up parentId chain
    let current = obj;
    while (current.parentId) {
      const parent = kernel.store.getObject(current.parentId);
      if (!parent) break;
      if (parent.type === "page") return parent;
      current = parent;
    }
    return undefined;
  }, [kernel, selectedId]);
}

// ── Hook: reactive page children ───────────────────────────────────────────

function usePageChildren(pageId: ObjectId | null): GraphObject[] {
  const kernel = useKernel();
  const [children, setChildren] = useState<GraphObject[]>([]);

  useEffect(() => {
    if (!pageId) {
      setChildren([]);
      return;
    }
    const refresh = () => {
      setChildren(
        kernel.store
          .listObjects({ parentId: pageId })
          .filter((o) => !o.deletedAt)
          .sort((a, b) => a.position - b.position),
      );
    };
    refresh();
    return kernel.store.onChange(() => refresh());
  }, [kernel, pageId]);

  return children;
}


export function LayoutPanel() {
  const kernel = useKernel();
  const page = useResolvePage();
  const pageChildren = usePageChildren(page?.id ?? null);

  // Debounce ref for live onChange sync
  const syncTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Help system surface (ADR-005): toolbar "?" button opens DocSearch; a
  // HelpEntry's "View full docs" button opens DocSheet over the panel.
  const [docState, setDocState] = useState<
    { path: string; anchor?: string } | null
  >(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const handleOpenDoc = useCallback((path: string, anchor?: string) => {
    setDocState(anchor !== undefined ? { path, anchor } : { path });
    setSearchOpen(false);
  }, []);
  const closeDoc = useCallback(() => setDocState(null), []);

  // Puck config: components come from `kernel.puckComponents` (registered
  // at kernel boot by `buildEntityPuckComponents`), categories are derived
  // from the component-category map, and the root config is built from the
  // `page` EntityDef. `selectedId` flows into the root render via a thunk
  // so bar-resize commits target the currently-open page reactively.
  const selectedPageIdRef = useRef<ObjectId | null>(null);
  selectedPageIdRef.current = page?.id ?? null;
  const puckConfig = useMemo<Config>(() => {
    const components: Record<string, ComponentConfig> = {};
    for (const name of kernel.puckComponents.directNames()) {
      const cfg = kernel.puckComponents.getDirect(name);
      if (cfg) components[name] = cfg;
    }
    const categories = buildPuckCategories(Object.keys(components));
    const root = buildPuckRootConfig(kernel, () => selectedPageIdRef.current);
    return {
      components,
      categories,
      root,
    } as unknown as Config;
  }, [kernel]);

  // Project kernel objects → Puck data
  const allObjects = useMemo(
    () => kernel.store.allObjects().filter((o) => !o.deletedAt),
    [kernel.store, pageChildren],
  );

  const puckData = useMemo<Data>(
    () =>
      page
        ? kernelToPuckData(page.id, allObjects)
        : { content: [], root: { props: {} } },
    [page, pageChildren, allObjects],
  );

  // Live onChange: debounced sync from Puck → kernel
  const handleChange = useCallback(
    (data: Data) => {
      if (!page) return;
      if (syncTimer.current) clearTimeout(syncTimer.current);
      syncTimer.current = setTimeout(() => {
        syncPuckToKernel(data, page.id, kernel);
      }, 300);
    },
    [page, kernel],
  );

  // Handle Puck publish → immediate sync + notification
  const handlePublish = useCallback(
    (data: Data) => {
      if (!page) return;
      if (syncTimer.current) clearTimeout(syncTimer.current);
      syncPuckToKernel(data, page.id, kernel);
      kernel.notifications.add({ title: "Layout saved", kind: "success" });
    },
    [page, kernel],
  );

  // Clean up debounce timer
  useEffect(() => {
    return () => {
      if (syncTimer.current) clearTimeout(syncTimer.current);
    };
  }, []);

  // Puck overrides: wrap component palette items with HelpTooltip so users
  // can hover a block in the sidebar and read its summary + "View full docs".
  // Puck's `componentItem` / `drawerItem` receive the PascalCase component
  // name; we convert it back to kebab-case to build the HelpEntry id
  // (`puck.components.<kebab-type>`).
  const puckOverrides = useMemo(() => {
    const wrap = ({
      children,
      name,
    }: {
      children: ReactNode;
      name: string;
    }) => {
      const kebab = pascalToKebab(name);
      const entry = HelpRegistry.get(`puck.components.${kebab}`);
      if (!entry) return <>{children}</>;
      return <HelpTooltip entry={entry}>{children}</HelpTooltip>;
    };
    return {
      componentItem: wrap,
      drawerItem: wrap,
    };
  }, []);

  if (!page) {
    return (
      <div style={emptyStyle} data-testid="layout-panel">
        <div style={{ textAlign: "center", maxWidth: 340 }}>
          <div style={{ fontSize: "2em", marginBottom: 8, opacity: 0.5 }}>
            {"\uD83D\uDD28"}
          </div>
          <div style={{ marginBottom: 6 }}>Select a page to edit its layout</div>
          <div style={{ fontSize: 12, opacity: 0.6 }}>
            The Layout Builder composes pages from registered components —
            drag from the sidebar, drop into sections, edit properties on
            the right. Create a page in the Object Explorer (left) to begin.
          </div>
        </div>
      </div>
    );
  }

  const pageData = page.data as Record<string, unknown> | undefined;
  const pageTitle =
    typeof pageData?.["title"] === "string" && pageData["title"] !== ""
      ? (pageData["title"] as string)
      : page.name;
  const published = pageData?.["published"] === true;
  const slug =
    typeof pageData?.["slug"] === "string" ? (pageData["slug"] as string) : "";

  return (
    <HelpProvider onOpenDoc={handleOpenDoc}>
      <div
        style={{ height: "100%", display: "flex", flexDirection: "column", position: "relative" }}
        data-testid="layout-panel"
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 12,
            padding: "8px 14px",
            borderBottom: "1px solid #1e293b",
            background: "#0f172a",
            color: "#e2e8f0",
            fontFamily: "system-ui, -apple-system, sans-serif",
            flex: "0 0 auto",
          }}
          data-testid="layout-panel-header"
        >
          <div style={{ display: "flex", flexDirection: "column", minWidth: 0, flex: 1 }}>
            <div
              style={{
                fontSize: 13,
                fontWeight: 600,
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
              data-testid="layout-panel-page-title"
            >
              {pageTitle}
            </div>
            {slug ? (
              <div style={{ fontSize: 11, color: "#94a3b8", fontFamily: "ui-monospace, Menlo, Consolas, monospace" }}>
                {slug}
              </div>
            ) : null}
          </div>
          <button
            type="button"
            onClick={() => setSearchOpen((v) => !v)}
            aria-label="Search documentation"
            aria-expanded={searchOpen}
            data-testid="layout-panel-help-button"
            style={{
              width: 26,
              height: 26,
              borderRadius: 999,
              border: "1px solid #334155",
              background: searchOpen ? "#1e293b" : "transparent",
              color: "#cbd5e1",
              fontSize: 12,
              fontWeight: 700,
              cursor: "pointer",
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              flex: "0 0 auto",
            }}
          >
            ?
          </button>
          <span
            data-testid="layout-panel-status-badge"
            style={{
              fontSize: 11,
              fontWeight: 600,
              padding: "3px 10px",
              borderRadius: 999,
              background: published ? "#14532d" : "#3f3f46",
              color: published ? "#a7f3d0" : "#d4d4d8",
              textTransform: "uppercase",
              letterSpacing: "0.05em",
            }}
          >
            {published ? "Published" : "Draft"}
          </span>
        </div>
        {searchOpen ? (
          <div
            data-testid="layout-panel-help-search"
            style={{
              position: "absolute",
              top: 52,
              right: 14,
              zIndex: 40,
              background: "white",
              border: "1px solid #cbd5e1",
              borderRadius: 8,
              boxShadow: "0 12px 32px -12px rgba(15, 23, 42, 0.35)",
              padding: 10,
            }}
          >
            <DocSearch
              autoFocus
              onPick={() => setSearchOpen(false)}
            />
          </div>
        ) : null}
        <div style={{ flex: 1, minHeight: 0 }}>
          <Puck
            key={page.id}
            config={puckConfig}
            data={puckData}
            onChange={handleChange}
            onPublish={handlePublish}
            overrides={puckOverrides}
          />
        </div>
        {docState ? (
          <DocSheet
            docPath={docState.path}
            {...(docState.anchor !== undefined ? { anchor: docState.anchor } : {})}
            onClose={closeDoc}
            fetchDoc={fetchHelpDoc}
          />
        ) : null}
      </div>
    </HelpProvider>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const LAYOUT_LENS_ID = lensId("layout");

export const layoutLensManifest: LensManifest = {

  id: LAYOUT_LENS_ID,
  name: "Layout",
  icon: "\u25A6",
  category: "visual",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-layout", name: "Switch to Layout Builder", shortcut: ["l"], section: "Navigation" }],
  },
};

export const layoutLensBundle: LensBundle = defineLensBundle(
  layoutLensManifest,
  LayoutPanel,
);
