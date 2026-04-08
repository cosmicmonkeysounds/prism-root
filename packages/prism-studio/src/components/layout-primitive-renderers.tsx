/**
 * Layout primitive renderers — structural widgets (columns, divider, spacer).
 *
 * These widgets compose other components into responsive arrangements.
 * Columns accepts child content via a `children` ReactNode slot so Puck
 * can nest components inside each column. Divider + Spacer are leaf nodes.
 */

import type { CSSProperties, ReactNode } from "react";

// ── Columns ────────────────────────────────────────────────────────────────

export interface ColumnsProps {
  /** Column count (1-6). */
  columnCount?: number;
  /** Gap in pixels. */
  gap?: number;
  /** Align items on the cross axis. */
  align?: "start" | "center" | "end" | "stretch";
  children?: ReactNode;
}

/** Clamp to a safe column range (1-6). */
export function clampColumns(count: unknown): number {
  if (count == null) return 2;
  const n = typeof count === "number" ? count : Number(count);
  if (!Number.isFinite(n)) return 2;
  if (n < 1) return 1;
  if (n > 6) return 6;
  return Math.floor(n);
}

export function ColumnsRenderer(props: ColumnsProps) {
  const { columnCount = 2, gap = 16, align = "stretch", children } = props;
  const cols = clampColumns(columnCount);
  const alignMap: Record<string, CSSProperties["alignItems"]> = {
    start: "flex-start",
    center: "center",
    end: "flex-end",
    stretch: "stretch",
  };
  return (
    <div
      data-testid="columns-layout"
      data-column-count={cols}
      style={{
        display: "grid",
        gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`,
        gap,
        alignItems: alignMap[align] ?? "stretch",
        margin: "0 0 8px 0",
      }}
    >
      {children ?? (
        <>
          {Array.from({ length: cols }).map((_, i) => (
            <div
              key={i}
              data-testid={`columns-placeholder-${i}`}
              style={{
                minHeight: 48,
                borderRadius: 4,
                border: "1px dashed #94a3b8",
                background: "#f8fafc",
                color: "#94a3b8",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                fontSize: 11,
              }}
            >
              Column {i + 1}
            </div>
          ))}
        </>
      )}
    </div>
  );
}

// ── Divider ────────────────────────────────────────────────────────────────

export interface DividerProps {
  style?: "solid" | "dashed" | "dotted" | undefined;
  thickness?: number | undefined;
  color?: string | undefined;
  spacing?: number | undefined;
  label?: string | undefined;
}

export function DividerRenderer(props: DividerProps) {
  const { style = "solid", thickness = 1, color = "#cbd5e1", spacing = 12, label } = props;
  if (label) {
    return (
      <div
        data-testid="divider"
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          margin: `${spacing}px 0`,
          color: "#64748b",
          fontSize: 12,
          fontWeight: 600,
          textTransform: "uppercase",
          letterSpacing: "0.05em",
        }}
      >
        <hr
          style={{
            flex: 1,
            border: "none",
            borderTop: `${thickness}px ${style} ${color}`,
            margin: 0,
          }}
        />
        <span>{label}</span>
        <hr
          style={{
            flex: 1,
            border: "none",
            borderTop: `${thickness}px ${style} ${color}`,
            margin: 0,
          }}
        />
      </div>
    );
  }
  return (
    <hr
      data-testid="divider"
      style={{
        border: "none",
        borderTop: `${thickness}px ${style} ${color}`,
        margin: `${spacing}px 0`,
      }}
    />
  );
}

// ── Spacer ─────────────────────────────────────────────────────────────────

export interface SpacerProps {
  size?: number;
  axis?: "vertical" | "horizontal";
}

export function SpacerRenderer(props: SpacerProps) {
  const { size = 16, axis = "vertical" } = props;
  const clamped = Math.max(0, Math.min(512, Number(size) || 0));
  return (
    <div
      data-testid="spacer"
      aria-hidden="true"
      style={
        axis === "horizontal"
          ? { display: "inline-block", width: clamped, height: 1 }
          : { width: "100%", height: clamped }
      }
    />
  );
}
