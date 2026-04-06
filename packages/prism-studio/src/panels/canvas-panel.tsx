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

// ── Block wrapper (selection highlight + click) ────────────────────────────

function BlockWrapper({
  obj,
  isSelected,
  onSelect,
  children,
}: {
  obj: GraphObject;
  isSelected: boolean;
  onSelect: (id: ObjectId) => void;
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
        outline: isSelected ? "2px solid #3b82f6" : "2px solid transparent",
        outlineOffset: "2px",
        borderRadius: "4px",
        cursor: "pointer",
        transition: "outline-color 0.15s",
      }}
    >
      {children}
    </div>
  );
}

// ── Component dispatcher ───────────────────────────────────────────────────

function ComponentBlock({
  obj,
  isSelected,
  onSelect,
}: {
  obj: GraphObject;
  isSelected: boolean;
  onSelect: (id: ObjectId) => void;
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
    default:
      content = (
        <div style={{ color: "#999", fontStyle: "italic", padding: "8px" }}>
          Unknown component: {obj.type}
        </div>
      );
  }

  return (
    <BlockWrapper obj={obj} isSelected={isSelected} onSelect={onSelect}>
      {content}
    </BlockWrapper>
  );
}

// ── Section renderer ───────────────────────────────────────────────────────

function SectionBlock({
  obj,
  selectedId,
  onSelect,
}: {
  obj: GraphObject;
  selectedId: ObjectId | null;
  onSelect: (id: ObjectId) => void;
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
          />
        ))
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
            />
          ))
        )}
      </div>
    </div>
  );
}
