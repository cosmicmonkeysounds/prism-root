/**
 * Canvas Preview Panel — WYSIWYG page builder preview.
 *
 * Renders the selected page (or the parent page of a selected child) as
 * visual React components. Sections are rendered in position order, each
 * containing its child components (heading, text-block, image, button, card).
 *
 * Clicking a block selects it in the kernel. The selected block gets a
 * blue outline highlight.
 */

import { useState, useEffect, useCallback, useMemo } from "react";
import type { CSSProperties, ReactNode } from "react";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { useKernel, useSelection, useObject } from "../kernel/index.js";
import {
  computeBlockStyle,
  extractBlockStyle,
  mergeCss,
} from "@prism/core/page-builder";
import { resolveObjectRefs, evaluateVisibleWhen } from "../kernel/data-binding.js";
import {
  parseLuauUi,
  renderUINode,
  useLuauParserReady,
} from "./luau-facet-panel.js";
import { KanbanWidgetRenderer } from "../components/kanban-widget-renderer.js";
import { ListWidgetRenderer } from "../components/list-widget-renderer.js";
import {
  TableWidgetRenderer,
  parseTableColumns,
  type TableSortDir,
} from "../components/table-widget-renderer.js";
import { CardGridWidgetRenderer } from "../components/card-grid-widget-renderer.js";
import {
  ReportWidgetRenderer,
  type ReportAggregation,
} from "../components/report-widget-renderer.js";
import { CalendarWidgetRenderer } from "../components/calendar-widget-renderer.js";
import { ChartWidgetRenderer, type ChartType, type ChartAggregation } from "../components/chart-widget-renderer.js";
import { MapWidgetRenderer } from "../components/map-widget-renderer.js";
import { TabContainerRenderer } from "../components/tab-container-renderer.js";
import { PopoverWidgetRenderer } from "../components/popover-widget-renderer.js";
import { SlidePanelRenderer } from "../components/slide-panel-renderer.js";
import {
  TextInputRenderer,
  TextareaInputRenderer,
  SelectInputRenderer,
  CheckboxInputRenderer,
  NumberInputRenderer,
  DateInputRenderer,
} from "../components/form-input-renderers.js";
import {
  ColumnsRenderer,
  DividerRenderer,
  SpacerRenderer,
} from "../components/layout-primitive-renderers.js";
import {
  StatWidgetRenderer,
  BadgeRenderer,
  AlertRenderer,
  ProgressBarRenderer,
  type StatAggregation,
  type BadgeTone,
} from "../components/data-display-renderers.js";
import {
  MarkdownWidgetRenderer,
  IframeWidgetRenderer,
} from "../components/content-renderers.js";
import { CodeBlockRenderer } from "../components/code-block-renderer.js";
import {
  VideoWidgetRenderer,
  AudioWidgetRenderer,
} from "../components/media-renderers.js";
import { PeerCursorsBar } from "../components/peer-cursors-overlay.js";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
// ── Block Toolbar ─────────────────────────────────────────────────────────

const toolbarStyles = {
  container: {
    position: "absolute" as const,
    top: -32,
    right: 0,
    display: "flex",
    gap: 2,
    background: "#252526",
    border: "1px solid #444",
    borderRadius: 4,
    padding: "2px 4px",
    boxShadow: "0 2px 8px rgba(0,0,0,0.3)",
    zIndex: 10,
  },
  btn: {
    background: "none",
    border: "1px solid transparent",
    borderRadius: 3,
    color: "#ccc",
    cursor: "pointer",
    fontSize: 11,
    padding: "2px 6px",
    lineHeight: 1,
  },
  btnDanger: {
    background: "none",
    border: "1px solid transparent",
    borderRadius: 3,
    color: "#f87171",
    cursor: "pointer",
    fontSize: 11,
    padding: "2px 6px",
    lineHeight: 1,
  },
} as const;

function BlockToolbar({
  obj,
  kernel,
}: {
  obj: GraphObject;
  kernel: ReturnType<typeof useKernel>;
}) {
  const handleMoveUp = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      if (obj.position <= 0) return;
      // Swap with sibling above
      const siblings = kernel.store
        .allObjects()
        .filter((o) => o.parentId === obj.parentId && !o.deletedAt && o.id !== obj.id)
        .sort((a, b) => a.position - b.position);
      const above = siblings.find((s) => s.position < obj.position);
      if (above) {
        kernel.updateObject(above.id, { position: obj.position });
      }
      kernel.updateObject(obj.id, { position: obj.position - 1 });
    },
    [obj, kernel],
  );

  const handleMoveDown = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      const siblings = kernel.store
        .allObjects()
        .filter((o) => o.parentId === obj.parentId && !o.deletedAt && o.id !== obj.id)
        .sort((a, b) => a.position - b.position);
      const below = siblings.find((s) => s.position > obj.position);
      if (below) {
        kernel.updateObject(below.id, { position: obj.position });
      }
      kernel.updateObject(obj.id, { position: obj.position + 1 });
    },
    [obj, kernel],
  );

  const handleDuplicate = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      const siblingCount = kernel.store.listObjects({ parentId: obj.parentId ?? null }).length;
      kernel.createObject({
        type: obj.type,
        name: `${obj.name} (copy)`,
        parentId: obj.parentId,
        position: siblingCount,
        status: obj.status,
        tags: [...obj.tags],
        date: obj.date,
        endDate: obj.endDate,
        description: obj.description,
        color: obj.color,
        image: obj.image,
        pinned: obj.pinned,
        data: { ...(obj.data as Record<string, unknown>) },
      });
      kernel.notifications.add({ title: `Duplicated "${obj.name}"`, kind: "success" });
    },
    [obj, kernel],
  );

  const handleDelete = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      const name = obj.name;
      kernel.deleteObject(obj.id);
      kernel.select(obj.parentId);
      kernel.notifications.add({ title: `Deleted "${name}"`, kind: "info" });
    },
    [obj, kernel],
  );

  return (
    <div style={toolbarStyles.container} data-testid={`block-toolbar-${obj.id}`}>
      <button
        style={toolbarStyles.btn}
        onClick={handleMoveUp}
        title="Move up"
        data-testid="toolbar-move-up"
      >
        {"\u25B2"}
      </button>
      <button
        style={toolbarStyles.btn}
        onClick={handleMoveDown}
        title="Move down"
        data-testid="toolbar-move-down"
      >
        {"\u25BC"}
      </button>
      <button
        style={toolbarStyles.btn}
        onClick={handleDuplicate}
        title="Duplicate"
        data-testid="toolbar-duplicate"
      >
        {"\u29C9"}
      </button>
      <button
        style={toolbarStyles.btnDanger}
        onClick={handleDelete}
        title="Delete"
        data-testid="toolbar-delete"
      >
        {"\u2715"}
      </button>
    </div>
  );
}

// ── Markdown helpers ───────────────────────────────────────────────────────

/** Minimal markdown-to-HTML for text-block content. */
function markdownToHtml(md: string): string {
  return md
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/\*(.+?)\*/g, "<em>$1</em>")
    .replace(/\n\n/g, "</p><p>")
    .replace(/\n/g, "<br/>")
    .replace(/^(.+)$/, "<p>$1</p>");
}

// ── Button variant styles ──────────────────────────────────────────────────

const BUTTON_VARIANTS: Record<string, CSSProperties> = {
  primary: {
    background: "#3b82f6",
    color: "#fff",
    border: "none",
  },
  secondary: {
    background: "#6b7280",
    color: "#fff",
    border: "none",
  },
  outline: {
    background: "transparent",
    color: "#3b82f6",
    border: "2px solid #3b82f6",
  },
  ghost: {
    background: "transparent",
    color: "#3b82f6",
    border: "none",
  },
};

// ── Hook: reactive children by parentId ────────────────────────────────────

function useChildren(parentId: ObjectId | null): GraphObject[] {
  const kernel = useKernel();
  const [children, setChildren] = useState<GraphObject[]>([]);

  useEffect(() => {
    if (!parentId) {
      setChildren([]);
      return;
    }

    const refresh = () => {
      setChildren(
        kernel.store
          .listObjects({ parentId })
          .filter((o) => !o.deletedAt)
          .sort((a, b) => a.position - b.position),
      );
    };

    refresh();
    return kernel.store.onChange(() => refresh());
  }, [kernel, parentId]);

  return children;
}

// ── Hook: find the page to render ──────────────────────────────────────────

function useResolvePage(): GraphObject | undefined {
  const kernel = useKernel();
  const { selectedId } = useSelection();
  const selectedObj = useObject(selectedId);

  return useMemo(() => {
    if (!selectedObj) return undefined;

    // If the selected object is already a page, use it directly
    if (selectedObj.type === "page") return selectedObj;

    // Walk up via parentId to find the nearest page ancestor
    let current: GraphObject | undefined = selectedObj;
    while (current && current.parentId) {
      const parent = kernel.store.getObject(current.parentId);
      if (!parent) break;
      if (parent.type === "page") return parent;
      current = parent;
    }

    return undefined;
  }, [kernel, selectedObj]);
}

// ── Component renderers ────────────────────────────────────────────────────

function HeadingRenderer({ obj }: { obj: GraphObject }) {
  const kernel = useKernel();
  const data = obj.data as { text?: string; level?: string; align?: string };
  const pool = kernel.store.allObjects();
  const text = resolveObjectRefs(data.text ?? obj.name, pool, obj);
  const level = data.level ?? "h2";
  const align = (data.align ?? "left") as CSSProperties["textAlign"];

  const style: CSSProperties = {
    textAlign: align,
    margin: "0 0 8px 0",
    color: "#1a1a1a",
  };

  switch (level) {
    case "h1":
      return <h1 style={{ ...style, fontSize: "2em" }}>{text}</h1>;
    case "h3":
      return <h3 style={{ ...style, fontSize: "1.25em" }}>{text}</h3>;
    case "h4":
      return <h4 style={{ ...style, fontSize: "1.1em" }}>{text}</h4>;
    default:
      return <h2 style={{ ...style, fontSize: "1.5em" }}>{text}</h2>;
  }
}

function TextBlockRenderer({ obj }: { obj: GraphObject }) {
  const kernel = useKernel();
  const data = obj.data as { content?: string; format?: string };
  const pool = kernel.store.allObjects();
  const content = resolveObjectRefs(data.content ?? "", pool, obj);
  const format = data.format ?? "markdown";

  if (format === "plain") {
    return (
      <div style={{ color: "#333", lineHeight: 1.6, margin: "0 0 8px 0" }}>
        {content}
      </div>
    );
  }

  return (
    <div
      style={{ color: "#333", lineHeight: 1.6, margin: "0 0 8px 0" }}
      dangerouslySetInnerHTML={{ __html: markdownToHtml(content) }}
    />
  );
}

function ImageRenderer({ obj }: { obj: GraphObject }) {
  const data = obj.data as {
    src?: string;
    alt?: string;
    caption?: string;
    width?: number;
    height?: number;
  };

  return (
    <figure style={{ margin: "0 0 8px 0", textAlign: "center" }}>
      <img
        src={data.src ?? ""}
        alt={data.alt ?? ""}
        style={{
          maxWidth: "100%",
          height: "auto",
          width: data.width ? `${data.width}px` : undefined,
          borderRadius: "4px",
        }}
      />
      {data.caption ? (
        <figcaption style={{ color: "#666", fontSize: "0.85em", marginTop: "4px" }}>
          {data.caption}
        </figcaption>
      ) : null}
    </figure>
  );
}

function ButtonRenderer({ obj }: { obj: GraphObject }) {
  const data = obj.data as {
    label?: string;
    href?: string;
    variant?: string;
    size?: string;
  };
  const label = data.label ?? "Click me";
  const variant = data.variant ?? "primary";
  const size = data.size ?? "md";

  const sizeStyles: Record<string, CSSProperties> = {
    sm: { padding: "6px 12px", fontSize: "0.85em" },
    md: { padding: "10px 20px", fontSize: "1em" },
    lg: { padding: "14px 28px", fontSize: "1.15em" },
  };

  const variantStyle = BUTTON_VARIANTS[variant] ?? BUTTON_VARIANTS.primary;

  return (
    <div style={{ margin: "0 0 8px 0" }}>
      <a
        href={data.href ?? "#"}
        onClick={(e) => e.preventDefault()}
        style={{
          display: "inline-block",
          textDecoration: "none",
          borderRadius: "6px",
          cursor: "pointer",
          fontWeight: 600,
          ...variantStyle,
          ...(sizeStyles[size] ?? sizeStyles.md),
        }}
      >
        {label}
      </a>
    </div>
  );
}

function CardRenderer({ obj }: { obj: GraphObject }) {
  const data = obj.data as {
    title?: string;
    body?: string;
    imageUrl?: string;
    linkUrl?: string;
  };

  return (
    <div
      style={{
        border: "1px solid #e0e0e0",
        borderRadius: "8px",
        overflow: "hidden",
        margin: "0 0 8px 0",
        background: "#fafafa",
      }}
    >
      {data.imageUrl ? (
        <img
          src={data.imageUrl}
          alt={data.title ?? ""}
          style={{ width: "100%", height: "180px", objectFit: "cover" }}
        />
      ) : null}
      <div style={{ padding: "16px" }}>
        {data.title ? (
          <h3 style={{ margin: "0 0 8px 0", color: "#1a1a1a", fontSize: "1.2em" }}>
            {data.title}
          </h3>
        ) : null}
        {data.body ? (
          <p style={{ margin: 0, color: "#555", lineHeight: 1.5 }}>{data.body}</p>
        ) : null}
      </div>
    </div>
  );
}

function LuauBlockRenderer({ obj }: { obj: GraphObject }) {
  const data = obj.data as { source?: string; title?: string };
  const source = data.source ?? "";
  const title = data.title ?? obj.name;
  // Re-render once the Luau parser finishes async WASM init.
  useLuauParserReady();
  const result = parseLuauUi(source);

  return (
    <div
      data-testid={`luau-block-preview-${obj.id}`}
      style={{
        border: "1px solid #06b6d4",
        borderRadius: 6,
        padding: 12,
        margin: "0 0 8px 0",
        background: "#f0fdff",
      }}
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
        <div style={{ color: "#dc2626", fontSize: 12 }}>Luau error: {result.error}</div>
      ) : result.nodes.length === 0 ? (
        <div style={{ color: "#999", fontStyle: "italic", fontSize: 12 }}>
          Empty Luau script
        </div>
      ) : (
        <div>{result.nodes.map((node, i) => renderUINode(node, i))}</div>
      )}
    </div>
  );
}

// ── Data-aware widget blocks ───────────────────────────────────────────────

function KanbanWidgetBlock({ obj, kernel }: { obj: GraphObject; kernel: ReturnType<typeof useKernel> }) {
  const data = obj.data as {
    collectionType?: string;
    groupField?: string;
    titleField?: string;
    colorField?: string;
    maxCardsPerColumn?: number;
  };
  const collectionType = data.collectionType ?? "";
  const groupField = data.groupField ?? "status";
  const objects = collectionType
    ? kernel.store.allObjects().filter((o) => o.type === collectionType && !o.deletedAt)
    : [];
  return (
    <KanbanWidgetRenderer
      objects={objects}
      groupField={groupField}
      titleField={data.titleField ?? "name"}
      colorField={data.colorField || undefined}
      maxCardsPerColumn={data.maxCardsPerColumn ?? 50}
      onMoveObject={(id, newValue) => kernel.updateObject(id as ObjectId, { [groupField]: newValue })}
    />
  );
}

function ListWidgetBlock({ obj, kernel }: { obj: GraphObject; kernel: ReturnType<typeof useKernel> }) {
  const { selectedId } = useSelection();
  const data = obj.data as {
    collectionType?: string;
    titleField?: string;
    subtitleField?: string;
    showStatus?: boolean;
    showTimestamp?: boolean;
  };
  const collectionType = data.collectionType ?? "";
  const objects = collectionType
    ? kernel.store.allObjects().filter((o) => o.type === collectionType && !o.deletedAt)
    : [];
  return (
    <ListWidgetRenderer
      objects={objects}
      titleField={data.titleField ?? "name"}
      subtitleField={data.subtitleField ?? "type"}
      showStatus={data.showStatus !== false}
      showTimestamp={data.showTimestamp !== false}
      selectedId={selectedId}
      onSelectObject={(id) => kernel.select(id as ObjectId)}
    />
  );
}

function TableWidgetBlock({ obj, kernel }: { obj: GraphObject; kernel: ReturnType<typeof useKernel> }) {
  const { selectedId } = useSelection();
  const data = obj.data as {
    collectionType?: string;
    columns?: string;
    sortField?: string;
    sortDir?: TableSortDir;
  };
  const collectionType = data.collectionType ?? "";
  const objects = collectionType
    ? kernel.store.allObjects().filter((o) => o.type === collectionType && !o.deletedAt)
    : [];
  return (
    <TableWidgetRenderer
      objects={objects}
      columns={parseTableColumns(data.columns ?? "")}
      sortField={data.sortField ?? "name"}
      sortDir={data.sortDir ?? "asc"}
      selectedId={selectedId}
      onSelectObject={(id) => kernel.select(id as ObjectId)}
    />
  );
}

function CardGridWidgetBlock({ obj, kernel }: { obj: GraphObject; kernel: ReturnType<typeof useKernel> }) {
  const { selectedId } = useSelection();
  const data = obj.data as {
    collectionType?: string;
    titleField?: string;
    subtitleField?: string;
    minColumnWidth?: number;
    showStatus?: boolean;
  };
  const collectionType = data.collectionType ?? "";
  const objects = collectionType
    ? kernel.store.allObjects().filter((o) => o.type === collectionType && !o.deletedAt)
    : [];
  return (
    <CardGridWidgetRenderer
      objects={objects}
      titleField={data.titleField ?? "name"}
      subtitleField={data.subtitleField ?? "type"}
      minColumnWidth={data.minColumnWidth ?? 220}
      showStatus={data.showStatus !== false}
      selectedId={selectedId}
      onSelectObject={(id) => kernel.select(id as ObjectId)}
    />
  );
}

function ReportWidgetBlock({ obj, kernel }: { obj: GraphObject; kernel: ReturnType<typeof useKernel> }) {
  const data = obj.data as {
    collectionType?: string;
    groupField?: string;
    titleField?: string;
    valueField?: string;
    aggregation?: ReportAggregation;
  };
  const collectionType = data.collectionType ?? "";
  const objects = collectionType
    ? kernel.store.allObjects().filter((o) => o.type === collectionType && !o.deletedAt)
    : [];
  return (
    <ReportWidgetRenderer
      objects={objects}
      groupField={data.groupField ?? "type"}
      titleField={data.titleField ?? "name"}
      valueField={data.valueField || undefined}
      aggregation={data.aggregation ?? "count"}
      onSelectObject={(id) => kernel.select(id as ObjectId)}
    />
  );
}

function CalendarWidgetBlock({ obj, kernel }: { obj: GraphObject; kernel: ReturnType<typeof useKernel> }) {
  const data = obj.data as {
    collectionType?: string;
    dateField?: string;
    titleField?: string;
    viewType?: "month" | "week" | "day";
  };
  const collectionType = data.collectionType ?? "";
  const objects = collectionType
    ? kernel.store.allObjects().filter((o) => o.type === collectionType && !o.deletedAt)
    : [];
  return (
    <CalendarWidgetRenderer
      objects={objects}
      dateField={data.dateField ?? "date"}
      titleField={data.titleField ?? "name"}
      viewType={data.viewType ?? "month"}
    />
  );
}

function ChartWidgetBlock({ obj, kernel }: { obj: GraphObject; kernel: ReturnType<typeof useKernel> }) {
  const data = obj.data as {
    collectionType?: string;
    chartType?: ChartType;
    groupField?: string;
    valueField?: string;
    aggregation?: ChartAggregation;
  };
  const collectionType = data.collectionType ?? "";
  const objects = collectionType
    ? kernel.store.allObjects().filter((o) => o.type === collectionType && !o.deletedAt)
    : [];
  return (
    <ChartWidgetRenderer
      objects={objects}
      chartType={data.chartType ?? "bar"}
      groupField={data.groupField ?? ""}
      valueField={data.valueField || undefined}
      aggregation={data.aggregation ?? "count"}
    />
  );
}

function MapWidgetBlock({ obj, kernel }: { obj: GraphObject; kernel: ReturnType<typeof useKernel> }) {
  const data = obj.data as {
    collectionType?: string;
    latField?: string;
    lngField?: string;
    titleField?: string;
  };
  const collectionType = data.collectionType ?? "";
  const objects = collectionType
    ? kernel.store.allObjects().filter((o) => o.type === collectionType && !o.deletedAt)
    : [];
  return (
    <MapWidgetRenderer
      objects={objects}
      latField={data.latField ?? "lat"}
      lngField={data.lngField ?? "lng"}
      titleField={data.titleField ?? "name"}
    />
  );
}

function TabContainerBlock({ obj }: { obj: GraphObject }) {
  const data = obj.data as { tabs?: string; activeTab?: number };
  return <TabContainerRenderer tabs={data.tabs ?? ""} activeTab={data.activeTab ?? 0} />;
}

function PopoverWidgetBlock({ obj }: { obj: GraphObject }) {
  const data = obj.data as { triggerLabel?: string; content?: string };
  return <PopoverWidgetRenderer triggerLabel={data.triggerLabel ?? "Open"} content={data.content ?? ""} />;
}

function SlidePanelBlock({ obj }: { obj: GraphObject }) {
  const data = obj.data as { label?: string; content?: string; collapsed?: boolean };
  return <SlidePanelRenderer label={data.label ?? "Details"} content={data.content ?? ""} collapsed={!!data.collapsed} />;
}

// ── Form input blocks ──────────────────────────────────────────────────────

function TextInputBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as {
    label?: string;
    placeholder?: string;
    defaultValue?: string;
    inputType?: "text" | "email" | "url" | "tel" | "password";
    required?: boolean;
    help?: string;
  };
  return (
    <TextInputRenderer
      label={d.label}
      placeholder={d.placeholder}
      defaultValue={d.defaultValue}
      inputType={d.inputType ?? "text"}
      required={!!d.required}
      help={d.help}
    />
  );
}

function TextareaInputBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as {
    label?: string;
    placeholder?: string;
    defaultValue?: string;
    rows?: number;
    required?: boolean;
    help?: string;
  };
  return (
    <TextareaInputRenderer
      label={d.label}
      placeholder={d.placeholder}
      defaultValue={d.defaultValue}
      rows={d.rows ?? 4}
      required={!!d.required}
      help={d.help}
    />
  );
}

function SelectInputBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as {
    label?: string;
    options?: string;
    defaultValue?: string;
    required?: boolean;
    help?: string;
  };
  return (
    <SelectInputRenderer
      label={d.label}
      options={d.options ?? ""}
      defaultValue={d.defaultValue}
      required={!!d.required}
      help={d.help}
    />
  );
}

function CheckboxInputBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as { label?: string; defaultChecked?: boolean; help?: string };
  return (
    <CheckboxInputRenderer
      label={d.label}
      defaultChecked={!!d.defaultChecked}
      help={d.help}
    />
  );
}

function NumberInputBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as {
    label?: string;
    defaultValue?: number;
    min?: number;
    max?: number;
    step?: number;
    required?: boolean;
    help?: string;
  };
  return (
    <NumberInputRenderer
      label={d.label}
      defaultValue={d.defaultValue}
      min={d.min}
      max={d.max}
      step={d.step}
      required={!!d.required}
      help={d.help}
    />
  );
}

function DateInputBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as {
    label?: string;
    defaultValue?: string;
    dateKind?: "date" | "datetime-local" | "time";
    required?: boolean;
    help?: string;
  };
  return (
    <DateInputRenderer
      label={d.label}
      defaultValue={d.defaultValue}
      dateKind={d.dateKind ?? "date"}
      required={!!d.required}
      help={d.help}
    />
  );
}

// ── Layout primitive blocks ────────────────────────────────────────────────

function ColumnsBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as {
    columnCount?: number;
    gap?: number;
    align?: "start" | "center" | "end" | "stretch";
  };
  return (
    <ColumnsRenderer
      columnCount={d.columnCount ?? 2}
      gap={d.gap ?? 16}
      align={d.align ?? "stretch"}
    />
  );
}

function DividerBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as {
    dividerStyle?: "solid" | "dashed" | "dotted";
    thickness?: number;
    color?: string;
    spacing?: number;
    label?: string;
  };
  return (
    <DividerRenderer
      style={d.dividerStyle ?? "solid"}
      thickness={d.thickness ?? 1}
      color={d.color ?? "#cbd5e1"}
      spacing={d.spacing ?? 12}
      label={d.label}
    />
  );
}

function SpacerBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as { size?: number; axis?: "vertical" | "horizontal" };
  return <SpacerRenderer size={d.size ?? 16} axis={d.axis ?? "vertical"} />;
}

// ── Data display blocks ────────────────────────────────────────────────────

function StatWidgetBlock({ obj, kernel }: { obj: GraphObject; kernel: ReturnType<typeof useKernel> }) {
  const d = obj.data as {
    collectionType?: string;
    label?: string;
    aggregation?: StatAggregation;
    valueField?: string;
    prefix?: string;
    suffix?: string;
    decimals?: number;
    thousands?: boolean;
  };
  const collectionType = d.collectionType ?? "";
  const objects = collectionType
    ? kernel.store.allObjects().filter((o) => o.type === collectionType && !o.deletedAt)
    : [];
  return (
    <StatWidgetRenderer
      objects={objects}
      label={d.label ?? "Total"}
      aggregation={d.aggregation ?? "count"}
      valueField={d.valueField || undefined}
      prefix={d.prefix ?? ""}
      suffix={d.suffix ?? ""}
      decimals={d.decimals ?? 0}
      thousands={d.thousands !== false}
    />
  );
}

function BadgeBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as { label?: string; tone?: BadgeTone; icon?: string; outline?: boolean };
  return (
    <BadgeRenderer
      label={d.label ?? "Badge"}
      tone={d.tone ?? "neutral"}
      icon={d.icon || undefined}
      outline={!!d.outline}
    />
  );
}

function AlertBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as { title?: string; message?: string; tone?: BadgeTone; icon?: string };
  return (
    <AlertRenderer
      title={d.title || undefined}
      message={d.message ?? ""}
      tone={d.tone ?? "info"}
      icon={d.icon || undefined}
    />
  );
}

function ProgressBarBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as {
    label?: string;
    value?: number;
    max?: number;
    tone?: BadgeTone;
    showPercent?: boolean;
  };
  return (
    <ProgressBarRenderer
      label={d.label || undefined}
      value={d.value ?? 0}
      max={d.max ?? 100}
      tone={d.tone ?? "info"}
      showPercent={d.showPercent !== false}
    />
  );
}

// ── Content blocks ─────────────────────────────────────────────────────────

function MarkdownWidgetBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as { source?: string };
  return <MarkdownWidgetRenderer source={d.source ?? ""} />;
}

function IframeWidgetBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as { src?: string; title?: string; height?: number; allowFullscreen?: boolean };
  return (
    <IframeWidgetRenderer
      src={d.src ?? ""}
      title={d.title ?? "Embedded content"}
      height={d.height ?? 360}
      allowFullscreen={d.allowFullscreen !== false}
    />
  );
}

function CodeBlockBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as {
    source?: string;
    language?: string;
    caption?: string;
    lineNumbers?: boolean;
    wrap?: boolean;
  };
  return (
    <CodeBlockRenderer
      source={d.source ?? ""}
      language={d.language}
      caption={d.caption}
      lineNumbers={d.lineNumbers !== false}
      wrap={d.wrap === true}
    />
  );
}

function VideoWidgetBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as {
    src?: string;
    poster?: string;
    caption?: string;
    width?: number;
    height?: number;
    controls?: boolean;
    autoplay?: boolean;
    loop?: boolean;
    muted?: boolean;
  };
  return (
    <VideoWidgetRenderer
      src={d.src}
      poster={d.poster}
      caption={d.caption}
      width={d.width}
      height={d.height}
      controls={d.controls !== false}
      autoplay={d.autoplay === true}
      loop={d.loop === true}
      muted={d.muted === true}
    />
  );
}

function AudioWidgetBlock({ obj }: { obj: GraphObject }) {
  const d = obj.data as {
    src?: string;
    caption?: string;
    controls?: boolean;
    autoplay?: boolean;
    loop?: boolean;
    muted?: boolean;
  };
  return (
    <AudioWidgetRenderer
      src={d.src}
      caption={d.caption}
      controls={d.controls !== false}
      autoplay={d.autoplay === true}
      loop={d.loop === true}
      muted={d.muted === true}
    />
  );
}

// ── Block wrapper (selection highlight + click) ────────────────────────────

function BlockWrapper({
  obj,
  isSelected,
  onSelect,
  kernel,
  children,
}: {
  obj: GraphObject;
  isSelected: boolean;
  onSelect: (id: ObjectId) => void;
  kernel: ReturnType<typeof useKernel>;
  children: ReactNode;
}) {
  const handleClick = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      onSelect(obj.id);
    },
    [obj.id, onSelect],
  );

  const styleOverride = computeBlockStyle(extractBlockStyle(obj.data));
  const data = obj.data as { visibleWhen?: string };
  const visible = evaluateVisibleWhen(data.visibleWhen, kernel.store.allObjects(), obj);
  if (!visible && !isSelected) return null;

  return (
    <div
      data-testid={`canvas-block-${obj.id}`}
      onClick={handleClick}
      style={mergeCss(
        {
          position: "relative",
          outline: isSelected ? "2px solid #3b82f6" : "2px solid transparent",
          outlineOffset: "2px",
          borderRadius: "4px",
          cursor: "pointer",
          transition: "outline-color 0.15s",
          opacity: visible ? 1 : 0.35,
        },
        styleOverride,
      )}
    >
      {isSelected && <BlockToolbar obj={obj} kernel={kernel} />}
      {children}
    </div>
  );
}

// ── Component dispatcher ───────────────────────────────────────────────────

function ComponentBlock({
  obj,
  isSelected,
  onSelect,
  kernel,
}: {
  obj: GraphObject;
  isSelected: boolean;
  onSelect: (id: ObjectId) => void;
  kernel: ReturnType<typeof useKernel>;
}) {
  let content: ReactNode;

  switch (obj.type) {
    case "heading":
      content = <HeadingRenderer obj={obj} />;
      break;
    case "text-block":
      content = <TextBlockRenderer obj={obj} />;
      break;
    case "image":
      content = <ImageRenderer obj={obj} />;
      break;
    case "button":
      content = <ButtonRenderer obj={obj} />;
      break;
    case "card":
      content = <CardRenderer obj={obj} />;
      break;
    case "luau-block":
      content = <LuauBlockRenderer obj={obj} />;
      break;
    case "kanban-widget":
      content = <KanbanWidgetBlock obj={obj} kernel={kernel} />;
      break;
    case "list-widget":
      content = <ListWidgetBlock obj={obj} kernel={kernel} />;
      break;
    case "table-widget":
      content = <TableWidgetBlock obj={obj} kernel={kernel} />;
      break;
    case "card-grid-widget":
      content = <CardGridWidgetBlock obj={obj} kernel={kernel} />;
      break;
    case "report-widget":
      content = <ReportWidgetBlock obj={obj} kernel={kernel} />;
      break;
    case "calendar-widget":
      content = <CalendarWidgetBlock obj={obj} kernel={kernel} />;
      break;
    case "chart-widget":
      content = <ChartWidgetBlock obj={obj} kernel={kernel} />;
      break;
    case "map-widget":
      content = <MapWidgetBlock obj={obj} kernel={kernel} />;
      break;
    case "tab-container":
      content = <TabContainerBlock obj={obj} />;
      break;
    case "popover-widget":
      content = <PopoverWidgetBlock obj={obj} />;
      break;
    case "slide-panel":
      content = <SlidePanelBlock obj={obj} />;
      break;
    case "text-input":
      content = <TextInputBlock obj={obj} />;
      break;
    case "textarea-input":
      content = <TextareaInputBlock obj={obj} />;
      break;
    case "select-input":
      content = <SelectInputBlock obj={obj} />;
      break;
    case "checkbox-input":
      content = <CheckboxInputBlock obj={obj} />;
      break;
    case "number-input":
      content = <NumberInputBlock obj={obj} />;
      break;
    case "date-input":
      content = <DateInputBlock obj={obj} />;
      break;
    case "columns":
      content = <ColumnsBlock obj={obj} />;
      break;
    case "divider":
      content = <DividerBlock obj={obj} />;
      break;
    case "spacer":
      content = <SpacerBlock obj={obj} />;
      break;
    case "stat-widget":
      content = <StatWidgetBlock obj={obj} kernel={kernel} />;
      break;
    case "badge":
      content = <BadgeBlock obj={obj} />;
      break;
    case "alert":
      content = <AlertBlock obj={obj} />;
      break;
    case "progress-bar":
      content = <ProgressBarBlock obj={obj} />;
      break;
    case "markdown-widget":
      content = <MarkdownWidgetBlock obj={obj} />;
      break;
    case "iframe-widget":
      content = <IframeWidgetBlock obj={obj} />;
      break;
    case "code-block":
      content = <CodeBlockBlock obj={obj} />;
      break;
    case "video-widget":
      content = <VideoWidgetBlock obj={obj} />;
      break;
    case "audio-widget":
      content = <AudioWidgetBlock obj={obj} />;
      break;
    default:
      content = (
        <div style={{ color: "#999", fontStyle: "italic", padding: "8px" }}>
          Unknown component: {obj.type}
        </div>
      );
  }

  return (
    <BlockWrapper obj={obj} isSelected={isSelected} onSelect={onSelect} kernel={kernel}>
      {content}
    </BlockWrapper>
  );
}

// ── Section renderer ───────────────────────────────────────────────────────

function SectionBlock({
  obj,
  selectedId,
  onSelect,
  kernel,
}: {
  obj: GraphObject;
  selectedId: ObjectId | null;
  onSelect: (id: ObjectId) => void;
  kernel: ReturnType<typeof useKernel>;
}) {
  const children = useChildren(obj.id);
  const styleOverride = computeBlockStyle(extractBlockStyle(obj.data));
  const isSelected = selectedId === obj.id;

  const handleSectionClick = useCallback(
    (e: React.MouseEvent) => {
      // Only select the section if the click target is the section itself
      if (e.target === e.currentTarget) {
        onSelect(obj.id);
      }
    },
    [obj.id, onSelect],
  );

  return (
    <div
      data-testid={`canvas-section-${obj.id}`}
      onClick={handleSectionClick}
      style={mergeCss(
        {
          border: "1px solid #e5e5e5",
          borderRadius: "6px",
          padding: "24px",
          marginBottom: "16px",
          background: "transparent",
          outline: isSelected ? "2px solid #3b82f6" : "2px solid transparent",
          outlineOffset: "2px",
          cursor: "pointer",
          transition: "outline-color 0.15s",
        },
        styleOverride,
      )}
    >
      {children.length === 0 ? (
        <div
          style={{
            color: "#bbb",
            textAlign: "center",
            padding: "24px",
            fontStyle: "italic",
          }}
        >
          Empty section
        </div>
      ) : (
        children.map((child) => (
          <ComponentBlock
            key={child.id}
            obj={child}
            isSelected={selectedId === child.id}
            onSelect={onSelect}
            kernel={kernel}
          />
        ))
      )}
      <QuickCreate parentId={obj.id} parentType={obj.type} kernel={kernel} />
    </div>
  );
}

// ── Quick-Create Combobox ──────────────────────────────────────────────────

function QuickCreate({
  parentId,
  parentType,
  kernel,
}: {
  parentId: ObjectId;
  parentType: string;
  kernel: ReturnType<typeof useKernel>;
}) {
  const [open, setOpen] = useState(false);

  const allowedTypes = useMemo(
    () => kernel.registry.getAllowedChildTypes(parentType),
    [kernel.registry, parentType],
  );

  const handleAdd = useCallback(
    (type: string) => {
      const def = kernel.registry.get(type);
      const siblings = kernel.store.listObjects({ parentId }).length;
      const newObj = kernel.createObject({
        type,
        name: `New ${def?.label ?? type}`,
        parentId,
        position: siblings,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
      });
      kernel.select(newObj.id);
      setOpen(false);
    },
    [parentId, kernel],
  );

  if (allowedTypes.length === 0) return null;

  return (
    <div
      style={{ textAlign: "center", padding: "8px 0" }}
      data-testid={`quick-create-${parentId}`}
    >
      {!open ? (
        <button
          onClick={(e) => {
            e.stopPropagation();
            setOpen(true);
          }}
          style={{
            background: "transparent",
            border: "1px dashed #ccc",
            borderRadius: 4,
            color: "#999",
            cursor: "pointer",
            fontSize: 12,
            padding: "6px 16px",
          }}
          data-testid="quick-create-trigger"
        >
          + Add block
        </button>
      ) : (
        <div
          style={{
            display: "inline-flex",
            gap: 4,
            flexWrap: "wrap",
            justifyContent: "center",
            padding: "4px 0",
          }}
          data-testid="quick-create-menu"
        >
          {allowedTypes.map((type) => {
            const def = kernel.registry.get(type);
            const icon = typeof def?.icon === "string" ? def.icon : "\u25CB";
            return (
              <button
                key={type}
                onClick={(e) => {
                  e.stopPropagation();
                  handleAdd(type);
                }}
                style={{
                  background: "#f0f0f0",
                  border: "1px solid #ddd",
                  borderRadius: 4,
                  color: "#333",
                  cursor: "pointer",
                  fontSize: 11,
                  padding: "4px 10px",
                }}
                data-testid={`quick-create-option-${type}`}
              >
                {icon} {def?.label ?? type}
              </button>
            );
          })}
          <button
            onClick={(e) => {
              e.stopPropagation();
              setOpen(false);
            }}
            style={{
              background: "transparent",
              border: "1px solid #ddd",
              borderRadius: 4,
              color: "#999",
              cursor: "pointer",
              fontSize: 11,
              padding: "4px 8px",
            }}
          >
            {"\u2715"}
          </button>
        </div>
      )}
    </div>
  );
}

// ── Main panel ─────────────────────────────────────────────────────────────

export function CanvasPanel() {
  const kernel = useKernel();
  const { selectedId } = useSelection();
  const page = useResolvePage();
  const sections = useChildren(page?.id ?? null);

  const handleSelect = useCallback(
    (id: ObjectId) => {
      kernel.select(id);
    },
    [kernel],
  );

  const handleCanvasClick = useCallback(() => {
    if (page) {
      kernel.select(page.id);
    }
  }, [kernel, page]);

  if (!page) {
    return (
      <div
        data-testid="canvas-panel"
        style={{
          height: "100%",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          background: "#1e1e1e",
          color: "#888",
          fontFamily: "system-ui, sans-serif",
        }}
      >
        <div style={{ textAlign: "center" }}>
          <div style={{ fontSize: "2em", marginBottom: "8px", opacity: 0.5 }}>
            {"\uD83D\uDCC4"}
          </div>
          <div>Select a page to preview</div>
        </div>
      </div>
    );
  }

  const pageData = page.data as { title?: string; layout?: string };
  const layout = pageData.layout ?? "flow";
  const maxWidth = layout === "shell" ? "100%" : "720px";

  return (
    <div
      data-testid="canvas-panel"
      style={{
        height: "100%",
        overflow: "auto",
        background: "#1e1e1e",
        fontFamily: "system-ui, sans-serif",
      }}
    >
      <PeerCursorsBar />
      <div style={{ padding: "24px" }}>
      <div
        onClick={handleCanvasClick}
        style={{
          maxWidth,
          margin: "0 auto",
          background: "#ffffff",
          borderRadius: "8px",
          boxShadow: "0 2px 12px rgba(0,0,0,0.3)",
          padding: "32px",
          minHeight: "400px",
          outline: selectedId === page.id ? "2px solid #3b82f6" : "none",
          outlineOffset: "2px",
        }}
      >
        {sections.length === 0 ? (
          <div
            style={{
              color: "#bbb",
              textAlign: "center",
              padding: "64px 24px",
              fontStyle: "italic",
            }}
          >
            This page has no sections. Add a section to start building.
          </div>
        ) : (
          sections.map((section) => (
            <SectionBlock
              key={section.id}
              obj={section}
              selectedId={selectedId}
              onSelect={handleSelect}
              kernel={kernel}
            />
          ))
        )}
        <QuickCreate parentId={page.id} parentType={page.type} kernel={kernel} />
      </div>
      </div>
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const CANVAS_LENS_ID = lensId("canvas");

export const canvasLensManifest: LensManifest = {

  id: CANVAS_LENS_ID,
  name: "Canvas",
  icon: "\uD83D\uDDBC",
  category: "visual",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-canvas", name: "Switch to Canvas Preview", shortcut: ["v"], section: "Navigation" }],
  },
};

export const canvasLensBundle: LensBundle = defineLensBundle(
  canvasLensManifest,
  CanvasPanel,
);
