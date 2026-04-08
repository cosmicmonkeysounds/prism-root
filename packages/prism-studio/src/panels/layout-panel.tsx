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

import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { Puck, type Config, type Data, type ComponentConfig, type Fields } from "@measured/puck";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { objectId } from "@prism/core/object-model";
import type { FacetLayout } from "@prism/core/facet";
import { useKernel, useSelection } from "../kernel/index.js";
import { parseLuaUi, renderUINode } from "./lua-facet-panel.js";
import { FacetViewRenderer } from "../components/facet-view-renderer.js";
import { SpatialCanvasRenderer } from "../components/spatial-canvas-renderer.js";
import { DataPortalRenderer } from "../components/data-portal-renderer.js";
import { KanbanWidgetRenderer } from "../components/kanban-widget-renderer.js";
import { CalendarWidgetRenderer } from "../components/calendar-widget-renderer.js";
import { ChartWidgetRenderer, type ChartType, type ChartAggregation } from "../components/chart-widget-renderer.js";
import { MapWidgetRenderer } from "../components/map-widget-renderer.js";
import { TabContainerRenderer } from "../components/tab-container-renderer.js";
import { PopoverWidgetRenderer } from "../components/popover-widget-renderer.js";
import { SlidePanelRenderer } from "../components/slide-panel-renderer.js";

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

// ── Config generation from ObjectRegistry ──────────────────────────────────

/**
 * Build a Puck component config entry from an EntityDef.
 * Each entity field becomes a Puck field; we map our field types to Puck's.
 */
function entityToPuckComponent(def: {
  type: string;
  label: string;
  icon?: string;
  fields: ReadonlyArray<{
    id: string;
    type: string;
    label?: string;
    required?: boolean;
    default?: unknown;
    enumOptions?: ReadonlyArray<{ value: string; label: string }>;
    ui?: { multiline?: boolean; placeholder?: string };
  }>;
}): ComponentConfig {
  const puckFields: Record<string, unknown> = {};
  const defaultProps: Record<string, unknown> = {};

  for (const f of def.fields) {
    switch (f.type) {
      case "string":
      case "url":
      case "color":
        puckFields[f.id] = { type: "text" };
        if (f.default !== undefined) defaultProps[f.id] = f.default;
        break;
      case "text":
        puckFields[f.id] = { type: "textarea" };
        if (f.default !== undefined) defaultProps[f.id] = f.default;
        break;
      case "bool":
        puckFields[f.id] = {
          type: "radio",
          options: [
            { label: "Yes", value: "true" },
            { label: "No", value: "false" },
          ],
        };
        defaultProps[f.id] = f.default !== undefined ? String(f.default) : "false";
        break;
      case "int":
      case "float":
        puckFields[f.id] = { type: "number" };
        if (f.default !== undefined) defaultProps[f.id] = f.default;
        break;
      case "enum":
        if (f.enumOptions && f.enumOptions.length > 0) {
          puckFields[f.id] = {
            type: "select",
            options: f.enumOptions.map((o) => ({
              label: o.label,
              value: o.value,
            })),
          };
          defaultProps[f.id] = f.default ?? f.enumOptions[0]?.value;
        } else {
          puckFields[f.id] = { type: "text" };
        }
        break;
      default:
        puckFields[f.id] = { type: "text" };
        if (f.default !== undefined) defaultProps[f.id] = f.default;
    }
  }

  return {
    fields: puckFields as Fields,
    defaultProps,
    render: (props) => {
      const p = props as Record<string, unknown>;
      // Generic renderer — shows component label + field values
      return (
        <div
          style={{
            border: "1px solid #e0e0e0",
            borderRadius: 6,
            padding: 12,
            margin: "4px 0",
            background: "#fafafa",
          }}
        >
          <div
            style={{
              fontSize: 11,
              fontWeight: 600,
              color: "#888",
              textTransform: "uppercase",
              letterSpacing: "0.05em",
              marginBottom: 6,
            }}
          >
            {def.icon ?? ""} {def.label}
          </div>
          {def.fields.slice(0, 3).map((f) => {
            const val = p[f.id];
            if (val === undefined || val === null || val === "") return null;
            return (
              <div key={f.id} style={{ fontSize: 13, color: "#333", marginBottom: 2 }}>
                {String(val)}
              </div>
            );
          })}
        </div>
      );
    },
  };
}

// ── Kernel → Puck Data projection ──────────────────────────────────────────

/**
 * Project kernel objects (children of a page, recursively) into Puck Data.
 * Sections become Puck zones; components become content items.
 */
function kernelToPuckData(
  pageChildren: GraphObject[],
  allObjects: GraphObject[],
): Data {
  const content: Data["content"] = [];

  for (const child of pageChildren) {
    // Map each kernel object to a Puck content item
    // The "type" field in Puck corresponds to the Puck component key
    // We use PascalCase as Puck convention
    const puckType = kebabToPascal(child.type);
    const props: Record<string, unknown> = { ...(child.data as Record<string, unknown>) };

    // If it's a section, include its children as nested content
    if (child.type === "section") {
      const sectionChildren = allObjects
        .filter((o) => o.parentId === child.id && !o.deletedAt)
        .sort((a, b) => a.position - b.position);

      for (const sc of sectionChildren) {
        content.push({
          type: kebabToPascal(sc.type),
          props: {
            id: sc.id,
            ...(sc.data as Record<string, unknown>),
          },
        });
      }
    } else {
      content.push({
        type: puckType,
        props: {
          id: child.id,
          ...props,
        },
      });
    }
  }

  return {
    content,
    root: { props: {} },
  };
}

// ── Puck Data → Kernel diff ────────────────────────────────────────────────

/**
 * Diff Puck data against the current kernel state and apply CRUD ops.
 * This is the critical sync path: Puck edits → kernel mutations.
 */
function syncPuckToKernel(
  newData: Data,
  pageId: ObjectId,
  kernel: {
    store: { listObjects(opts: { parentId: ObjectId }): GraphObject[]; allObjects(): GraphObject[] };
    createObject(obj: Omit<GraphObject, "id" | "createdAt" | "updatedAt">): GraphObject;
    updateObject(id: ObjectId, patch: Partial<GraphObject>): GraphObject | undefined;
    deleteObject(id: ObjectId): boolean;
  },
) {
  // Get all existing children (flat — sections + their children)
  const allObjs = kernel.store.allObjects().filter((o) => !o.deletedAt);
  const existingByPage = allObjs.filter((o) => {
    if (o.parentId === pageId) return true;
    // Also include grandchildren (components inside sections)
    const parent = allObjs.find((p) => p.id === o.parentId);
    return parent?.parentId === pageId;
  });

  const existingById = new Map(existingByPage.map((o) => [o.id, o]));
  const puckIds = new Set<string>();

  // Process each Puck content item
  for (let i = 0; i < newData.content.length; i++) {
    const item = newData.content[i];
    if (!item) continue;
    const props = (item.props ?? {}) as Record<string, unknown>;
    const id = props.id as string | undefined;
    const kernelType = pascalToKebab(item.type);

    // Strip our tracking props before storing as data
    const data = { ...props };
    delete data.id;

    const oid = id ? objectId(id) : null;
    if (oid && existingById.has(oid)) {
      // Update existing object
      kernel.updateObject(oid, {
        position: i,
        data,
      });
      puckIds.add(id as string);
      existingById.delete(oid);
    } else {
      // Create new object from Puck
      const newObj = kernel.createObject({
        type: kernelType,
        name: (data.text as string) ?? (data.title as string) ?? (data.label as string) ?? `New ${kernelType}`,
        parentId: pageId,
        position: i,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data,
      });
      puckIds.add(newObj.id);
    }
  }

  // Delete objects removed from Puck
  for (const [id] of existingById) {
    kernel.deleteObject(objectId(id));
  }
}

// ── Helpers ────────────────────────────────────────────────────────────────

function kebabToPascal(s: string): string {
  return s
    .split("-")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join("");
}

function pascalToKebab(s: string): string {
  return s.replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase();
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

// ── Main Panel ─────────────────────────────────────────────────────────────

export function LayoutPanel() {
  const kernel = useKernel();
  const page = useResolvePage();
  const pageChildren = usePageChildren(page?.id ?? null);

  // Debounce ref for live onChange sync
  const syncTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Build Puck config from ObjectRegistry — component-category entities become Puck components
  const puckConfig = useMemo<Config>(() => {
    const components: Record<string, ComponentConfig> = {};
    const allDefs = kernel.registry.allDefs();

    for (const def of allDefs) {
      // Only component + section category entities become Puck components
      if (def.category === "component" || def.category === "section") {
        // Special renderer for lua-block: parse and render Lua UI
        if (def.type === "lua-block") {
          components[kebabToPascal(def.type)] = {
            fields: {
              source: { type: "textarea" } as unknown as Fields[string],
              title: { type: "text" } as unknown as Fields[string],
            },
            defaultProps: { source: "return ui.label(\"Hello from Lua!\")", title: "Lua Block" },
            render: (props) => {
              const p = props as Record<string, unknown>;
              const source = (p["source"] as string) ?? "";
              const title = (p["title"] as string) ?? "Lua Block";
              const result = parseLuaUi(source);
              return (
                <div
                  style={{
                    border: "1px solid #06b6d4",
                    borderRadius: 6,
                    padding: 12,
                    margin: "4px 0",
                    background: "#0a1929",
                  }}
                  data-testid="puck-lua-block"
                >
                  <div
                    style={{
                      fontSize: 11,
                      fontWeight: 600,
                      color: "#06b6d4",
                      textTransform: "uppercase",
                      letterSpacing: "0.05em",
                      marginBottom: 6,
                      display: "flex",
                      alignItems: "center",
                      gap: 4,
                    }}
                  >
                    {"\uD83C\uDF19"} {title}
                  </div>
                  {result.error ? (
                    <div style={{ color: "#f87171", fontSize: 12 }}>Error: {result.error}</div>
                  ) : (
                    <div>{result.nodes.map((node, i) => renderUINode(node, i))}</div>
                  )}
                </div>
              );
            },
          };
          continue;
        }

        // Special renderer for facet-view: renders a FacetDefinition as a data view
        if (def.type === "facet-view") {
          components[kebabToPascal(def.type)] = {
            fields: {
              facetId: { type: "text" } as unknown as Fields[string],
              viewMode: {
                type: "select",
                options: [
                  { label: "Form", value: "form" },
                  { label: "List", value: "list" },
                  { label: "Table", value: "table" },
                  { label: "Report", value: "report" },
                  { label: "Card", value: "card" },
                ],
              } as unknown as Fields[string],
              maxRows: { type: "number" } as unknown as Fields[string],
            },
            defaultProps: { facetId: "", viewMode: "form", maxRows: 25 },
            render: (props) => {
              const p = props as Record<string, unknown>;
              const facetId = (p["facetId"] as string) ?? "";
              const viewMode = (p["viewMode"] as FacetLayout) ?? "form";
              const maxRows = (p["maxRows"] as number) ?? 25;
              const facetDef = facetId ? kernel.getFacetDefinition(facetId) : undefined;
              const objects = facetDef
                ? kernel.store.allObjects().filter(
                    (o) => o.type === facetDef.objectType && !o.deletedAt,
                  )
                : [];
              return (
                <div data-testid="puck-facet-view" style={{ margin: "4px 0" }}>
                  <FacetViewRenderer
                    definition={facetDef}
                    objects={objects}
                    viewMode={viewMode}
                    maxRows={maxRows}
                  />
                </div>
              );
            },
          };
          continue;
        }

        // Special renderer for spatial-canvas: free-form positioned fields
        if (def.type === "spatial-canvas") {
          components[kebabToPascal(def.type)] = {
            fields: {
              facetId: { type: "text" } as unknown as Fields[string],
              canvasWidth: { type: "number" } as unknown as Fields[string],
              canvasHeight: { type: "number" } as unknown as Fields[string],
              gridSize: { type: "number" } as unknown as Fields[string],
              showGrid: {
                type: "radio",
                options: [
                  { label: "Yes", value: "true" },
                  { label: "No", value: "false" },
                ],
              } as unknown as Fields[string],
            },
            defaultProps: { facetId: "", canvasWidth: 612, canvasHeight: 400, gridSize: 8, showGrid: "true" },
            render: (props) => {
              const p = props as Record<string, unknown>;
              const facetId = (p["facetId"] as string) ?? "";
              const canvasWidth = (p["canvasWidth"] as number) ?? 612;
              const canvasHeight = (p["canvasHeight"] as number) ?? 400;
              const gridSize = (p["gridSize"] as number) ?? 8;
              const showGrid = p["showGrid"] === "true" || p["showGrid"] === true;
              const facetDef = facetId ? kernel.getFacetDefinition(facetId) : undefined;
              if (!facetDef) {
                return (
                  <div
                    style={{
                      border: "1px solid #f97316",
                      borderRadius: 6,
                      padding: 12,
                      margin: "4px 0",
                      background: "#1a1a2e",
                      color: "#888",
                      textAlign: "center",
                      minHeight: 80,
                    }}
                    data-testid="puck-spatial-canvas"
                  >
                    <div style={{ fontSize: 11, fontWeight: 600, color: "#f97316", textTransform: "uppercase", marginBottom: 6 }}>
                      {"\uD83D\uDCD0"} Spatial Canvas
                    </div>
                    <div style={{ fontSize: 12 }}>Set a Facet Definition ID to render the canvas.</div>
                  </div>
                );
              }
              return (
                <div data-testid="puck-spatial-canvas" style={{ margin: "4px 0" }}>
                  <SpatialCanvasRenderer
                    definition={facetDef}
                    editable={false}
                    canvasWidth={canvasWidth}
                    canvasHeight={canvasHeight}
                    gridSize={gridSize}
                    showGrid={showGrid}
                  />
                </div>
              );
            },
          };
          continue;
        }

        // Special renderer for data-portal: related records inline
        if (def.type === "data-portal") {
          components[kebabToPascal(def.type)] = {
            fields: {
              relationshipId: { type: "text" } as unknown as Fields[string],
              displayFields: { type: "text" } as unknown as Fields[string],
              visibleRows: { type: "number" } as unknown as Fields[string],
              allowCreation: {
                type: "radio",
                options: [
                  { label: "Yes", value: "true" },
                  { label: "No", value: "false" },
                ],
              } as unknown as Fields[string],
              sortField: { type: "text" } as unknown as Fields[string],
            },
            defaultProps: { relationshipId: "", displayFields: "", visibleRows: 5, allowCreation: "false", sortField: "" },
            render: (props) => {
              const p = props as Record<string, unknown>;
              const relationshipId = (p["relationshipId"] as string) ?? "";
              const displayFieldsStr = (p["displayFields"] as string) ?? "";
              const displayFields = displayFieldsStr.split(",").map((s) => s.trim()).filter(Boolean);
              const visibleRows = (p["visibleRows"] as number) ?? 5;
              const sortField = (p["sortField"] as string) ?? "";
              if (!relationshipId) {
                return (
                  <div
                    style={{
                      border: "1px solid #8b5cf6",
                      borderRadius: 6,
                      padding: 12,
                      margin: "4px 0",
                      background: "#1e1e1e",
                      color: "#888",
                      textAlign: "center",
                    }}
                    data-testid="puck-data-portal"
                  >
                    <div style={{ fontSize: 11, fontWeight: 600, color: "#8b5cf6", textTransform: "uppercase", marginBottom: 6 }}>
                      {"\uD83D\uDD17"} Data Portal
                    </div>
                    <div style={{ fontSize: 12 }}>Set a Relationship Edge Type to display related records.</div>
                  </div>
                );
              }
              const allObjects = kernel.store.allObjects().filter((o) => !o.deletedAt);
              const allEdges = kernel.store.allEdges();
              return (
                <div data-testid="puck-data-portal" style={{ margin: "4px 0" }}>
                  <DataPortalRenderer
                    objects={allObjects}
                    edges={allEdges}
                    relationshipId={relationshipId}
                    displayFields={displayFields}
                    visibleRows={visibleRows}
                    sortField={sortField || undefined}
                  />
                </div>
              );
            },
          };
          continue;
        }

        // ── Data-aware widgets ─────────────────────────────────────────────

        if (def.type === "kanban-widget") {
          components[kebabToPascal(def.type)] = {
            fields: {
              collectionType: { type: "text" } as unknown as Fields[string],
              groupField: { type: "text" } as unknown as Fields[string],
              titleField: { type: "text" } as unknown as Fields[string],
              colorField: { type: "text" } as unknown as Fields[string],
              maxCardsPerColumn: { type: "number" } as unknown as Fields[string],
            },
            defaultProps: { collectionType: "", groupField: "status", titleField: "name", colorField: "", maxCardsPerColumn: 50 },
            render: (props) => {
              const p = props as Record<string, unknown>;
              const collectionType = (p["collectionType"] as string) ?? "";
              const groupField = (p["groupField"] as string) ?? "status";
              const titleField = (p["titleField"] as string) ?? "name";
              const colorField = (p["colorField"] as string) || undefined;
              const maxCards = (p["maxCardsPerColumn"] as number) ?? 50;
              const objects = collectionType
                ? kernel.store.allObjects().filter((o) => o.type === collectionType && !o.deletedAt)
                : [];
              return (
                <div data-testid="puck-kanban-widget" style={{ margin: "4px 0" }}>
                  <KanbanWidgetRenderer
                    objects={objects}
                    groupField={groupField}
                    titleField={titleField}
                    colorField={colorField}
                    maxCardsPerColumn={maxCards}
                    onMoveObject={(id, newValue) => {
                      kernel.updateObject(id as ObjectId, { [groupField]: newValue });
                    }}
                  />
                </div>
              );
            },
          };
          continue;
        }

        if (def.type === "calendar-widget") {
          components[kebabToPascal(def.type)] = {
            fields: {
              collectionType: { type: "text" } as unknown as Fields[string],
              dateField: { type: "text" } as unknown as Fields[string],
              titleField: { type: "text" } as unknown as Fields[string],
              viewType: {
                type: "select",
                options: [
                  { label: "Month", value: "month" },
                  { label: "Week", value: "week" },
                  { label: "Day", value: "day" },
                ],
              } as unknown as Fields[string],
            },
            defaultProps: { collectionType: "", dateField: "date", titleField: "name", viewType: "month" },
            render: (props) => {
              const p = props as Record<string, unknown>;
              const collectionType = (p["collectionType"] as string) ?? "";
              const dateField = (p["dateField"] as string) ?? "date";
              const titleField = (p["titleField"] as string) ?? "name";
              const viewType = (p["viewType"] as "month" | "week" | "day") ?? "month";
              const objects = collectionType
                ? kernel.store.allObjects().filter((o) => o.type === collectionType && !o.deletedAt)
                : [];
              return (
                <div data-testid="puck-calendar-widget" style={{ margin: "4px 0" }}>
                  <CalendarWidgetRenderer
                    objects={objects}
                    dateField={dateField}
                    titleField={titleField}
                    viewType={viewType}
                  />
                </div>
              );
            },
          };
          continue;
        }

        if (def.type === "chart-widget") {
          components[kebabToPascal(def.type)] = {
            fields: {
              collectionType: { type: "text" } as unknown as Fields[string],
              chartType: {
                type: "select",
                options: [
                  { label: "Bar", value: "bar" },
                  { label: "Line", value: "line" },
                  { label: "Pie", value: "pie" },
                  { label: "Area", value: "area" },
                ],
              } as unknown as Fields[string],
              groupField: { type: "text" } as unknown as Fields[string],
              valueField: { type: "text" } as unknown as Fields[string],
              aggregation: {
                type: "select",
                options: [
                  { label: "Count", value: "count" },
                  { label: "Sum", value: "sum" },
                  { label: "Average", value: "avg" },
                  { label: "Min", value: "min" },
                  { label: "Max", value: "max" },
                ],
              } as unknown as Fields[string],
            },
            defaultProps: { collectionType: "", chartType: "bar", groupField: "", valueField: "", aggregation: "count" },
            render: (props) => {
              const p = props as Record<string, unknown>;
              const collectionType = (p["collectionType"] as string) ?? "";
              const chartType = (p["chartType"] as ChartType) ?? "bar";
              const groupField = (p["groupField"] as string) ?? "";
              const valueField = (p["valueField"] as string) || undefined;
              const aggregation = (p["aggregation"] as ChartAggregation) ?? "count";
              const objects = collectionType
                ? kernel.store.allObjects().filter((o) => o.type === collectionType && !o.deletedAt)
                : [];
              return (
                <div data-testid="puck-chart-widget" style={{ margin: "4px 0" }}>
                  <ChartWidgetRenderer
                    objects={objects}
                    chartType={chartType}
                    groupField={groupField}
                    valueField={valueField}
                    aggregation={aggregation}
                  />
                </div>
              );
            },
          };
          continue;
        }

        if (def.type === "map-widget") {
          components[kebabToPascal(def.type)] = {
            fields: {
              collectionType: { type: "text" } as unknown as Fields[string],
              latField: { type: "text" } as unknown as Fields[string],
              lngField: { type: "text" } as unknown as Fields[string],
              titleField: { type: "text" } as unknown as Fields[string],
              initialZoom: { type: "number" } as unknown as Fields[string],
            },
            defaultProps: { collectionType: "", latField: "lat", lngField: "lng", titleField: "name", initialZoom: 10 },
            render: (props) => {
              const p = props as Record<string, unknown>;
              const collectionType = (p["collectionType"] as string) ?? "";
              const latField = (p["latField"] as string) ?? "lat";
              const lngField = (p["lngField"] as string) ?? "lng";
              const titleField = (p["titleField"] as string) ?? "name";
              const objects = collectionType
                ? kernel.store.allObjects().filter((o) => o.type === collectionType && !o.deletedAt)
                : [];
              return (
                <div data-testid="puck-map-widget" style={{ margin: "4px 0" }}>
                  <MapWidgetRenderer
                    objects={objects}
                    latField={latField}
                    lngField={lngField}
                    titleField={titleField}
                  />
                </div>
              );
            },
          };
          continue;
        }

        if (def.type === "tab-container") {
          components[kebabToPascal(def.type)] = {
            fields: {
              tabs: { type: "text" } as unknown as Fields[string],
              activeTab: { type: "number" } as unknown as Fields[string],
            },
            defaultProps: { tabs: "Tab 1,Tab 2", activeTab: 0 },
            render: (props) => {
              const p = props as Record<string, unknown>;
              const tabs = (p["tabs"] as string) ?? "";
              const activeTab = (p["activeTab"] as number) ?? 0;
              return (
                <div data-testid="puck-tab-container" style={{ margin: "4px 0" }}>
                  <TabContainerRenderer tabs={tabs} activeTab={activeTab} />
                </div>
              );
            },
          };
          continue;
        }

        if (def.type === "popover-widget") {
          components[kebabToPascal(def.type)] = {
            fields: {
              triggerLabel: { type: "text" } as unknown as Fields[string],
              content: { type: "textarea" } as unknown as Fields[string],
            },
            defaultProps: { triggerLabel: "Open", content: "" },
            render: (props) => {
              const p = props as Record<string, unknown>;
              const triggerLabel = (p["triggerLabel"] as string) ?? "Open";
              const content = (p["content"] as string) ?? "";
              return (
                <div data-testid="puck-popover-widget" style={{ margin: "4px 0" }}>
                  <PopoverWidgetRenderer triggerLabel={triggerLabel} content={content} />
                </div>
              );
            },
          };
          continue;
        }

        if (def.type === "slide-panel") {
          components[kebabToPascal(def.type)] = {
            fields: {
              label: { type: "text" } as unknown as Fields[string],
              content: { type: "textarea" } as unknown as Fields[string],
              collapsed: {
                type: "radio",
                options: [
                  { label: "Open", value: "false" },
                  { label: "Collapsed", value: "true" },
                ],
              } as unknown as Fields[string],
            },
            defaultProps: { label: "Details", content: "", collapsed: "false" },
            render: (props) => {
              const p = props as Record<string, unknown>;
              const label = (p["label"] as string) ?? "Details";
              const content = (p["content"] as string) ?? "";
              const collapsed = p["collapsed"] === "true" || p["collapsed"] === true;
              return (
                <div data-testid="puck-slide-panel" style={{ margin: "4px 0" }}>
                  <SlidePanelRenderer label={label} content={content} collapsed={collapsed} />
                </div>
              );
            },
          };
          continue;
        }

        const iconStr = typeof def.icon === "string" ? def.icon : "";
        components[kebabToPascal(def.type)] = entityToPuckComponent({
          type: def.type,
          label: def.label,
          icon: iconStr,
          fields: def.fields ?? [],
        });
      }
    }

    // Fallback: if no registry components, add basic ones
    if (Object.keys(components).length === 0) {
      components.Heading = {
        fields: { text: { type: "text" } },
        defaultProps: { text: "Heading" },
        render: (props) => <h2>{(props as Record<string, unknown>).text as string}</h2>,
      };
      components.Text = {
        fields: { content: { type: "textarea" } },
        defaultProps: { content: "" },
        render: (props) => <p>{(props as Record<string, unknown>).content as string}</p>,
      };
    }

    return { components };
  }, [kernel.registry]);

  // Project kernel objects → Puck data
  const allObjects = useMemo(
    () => kernel.store.allObjects().filter((o) => !o.deletedAt),
    [kernel.store, pageChildren],
  );

  const puckData = useMemo<Data>(
    () => (page ? kernelToPuckData(pageChildren, allObjects) : { content: [], root: { props: {} } }),
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

  if (!page) {
    return (
      <div style={emptyStyle} data-testid="layout-panel">
        <div style={{ textAlign: "center" }}>
          <div style={{ fontSize: "2em", marginBottom: 8, opacity: 0.5 }}>
            {"\uD83D\uDD28"}
          </div>
          <div>Select a page to edit its layout</div>
        </div>
      </div>
    );
  }

  return (
    <div style={{ height: "100%" }} data-testid="layout-panel">
      <Puck
        config={puckConfig}
        data={puckData}
        onChange={handleChange}
        onPublish={handlePublish}
      />
    </div>
  );
}
