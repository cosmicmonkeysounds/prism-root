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
import { parseLuaUi, renderUINode } from "./lua-facet-panel.js";

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

// ── Padding map ────────────────────────────────────────────────────────────

const PADDING_MAP: Record<string, string> = {
  none: "0",
  sm: "12px",
  md: "24px",
  lg: "48px",
};

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
  const data = obj.data as { text?: string; level?: string; align?: string };
  const text = data.text ?? obj.name;
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
  const data = obj.data as { content?: string; format?: string };
  const content = data.content ?? "";
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

function LuaBlockRenderer({ obj }: { obj: GraphObject }) {
  const data = obj.data as { source?: string; title?: string };
  const source = data.source ?? "";
  const title = data.title ?? obj.name;
  const result = parseLuaUi(source);

  return (
    <div
      data-testid={`lua-block-preview-${obj.id}`}
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
        <div style={{ color: "#dc2626", fontSize: 12 }}>Lua error: {result.error}</div>
      ) : result.nodes.length === 0 ? (
        <div style={{ color: "#999", fontStyle: "italic", fontSize: 12 }}>
          Empty Lua script
        </div>
      ) : (
        <div>{result.nodes.map((node, i) => renderUINode(node, i))}</div>
      )}
    </div>
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

  return (
    <div
      data-testid={`canvas-block-${obj.id}`}
      onClick={handleClick}
      style={{
        position: "relative",
        outline: isSelected ? "2px solid #3b82f6" : "2px solid transparent",
        outlineOffset: "2px",
        borderRadius: "4px",
        cursor: "pointer",
        transition: "outline-color 0.15s",
      }}
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
    case "lua-block":
      content = <LuaBlockRenderer obj={obj} />;
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
  const data = obj.data as { padding?: string; background?: string; variant?: string };
  const padding = PADDING_MAP[data.padding ?? "md"] ?? PADDING_MAP.md;
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
      style={{
        border: "1px solid #e5e5e5",
        borderRadius: "6px",
        padding,
        marginBottom: "16px",
        background: data.background ?? "transparent",
        outline: isSelected ? "2px solid #3b82f6" : "2px solid transparent",
        outlineOffset: "2px",
        cursor: "pointer",
        transition: "outline-color 0.15s",
      }}
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
  const layout = pageData.layout ?? "single";
  const maxWidth = layout === "full" ? "100%" : layout === "sidebar" ? "960px" : "720px";

  return (
    <div
      data-testid="canvas-panel"
      style={{
        height: "100%",
        overflow: "auto",
        background: "#1e1e1e",
        padding: "24px",
        fontFamily: "system-ui, sans-serif",
      }}
    >
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
  );
}
