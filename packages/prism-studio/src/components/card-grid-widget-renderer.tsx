/**
 * CardGridWidgetRenderer — responsive grid of object cards.
 *
 * Lays out objects in a CSS grid with auto-fill columns. Each card shows
 * title, type, optional status badge, and timestamp. Configurable min
 * column width. Click a card to select.
 */

import type { GraphObject } from "@prism/core/object-model";
import { readListField } from "./list-widget-renderer.js";

export interface CardGridWidgetProps {
  objects: GraphObject[];
  titleField?: string;
  subtitleField?: string;
  minColumnWidth?: number;
  showStatus?: boolean;
  selectedId?: string | null;
  onSelectObject?: (id: string) => void;
}

/** Clamp minColumnWidth into a sane pixel range. */
export function clampColumnWidth(value: unknown): number {
  const n = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(n)) return 220;
  if (n < 80) return 80;
  if (n > 480) return 480;
  return Math.round(n);
}

function formatDate(iso: string | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleDateString();
}

export function CardGridWidgetRenderer(props: CardGridWidgetProps) {
  const {
    objects,
    titleField = "name",
    subtitleField = "type",
    minColumnWidth = 220,
    showStatus = true,
    selectedId = null,
    onSelectObject,
  } = props;

  const width = clampColumnWidth(minColumnWidth);

  if (objects.length === 0) {
    return (
      <div
        data-testid="card-grid-widget-empty"
        style={{
          padding: 24,
          color: "#94a3b8",
          fontSize: 12,
          fontStyle: "italic",
          textAlign: "center",
          border: "1px solid #334155",
          borderRadius: 6,
          background: "#0f172a",
        }}
      >
        No records to display.
      </div>
    );
  }

  return (
    <div
      data-testid="card-grid-widget"
      style={{
        display: "grid",
        gridTemplateColumns: `repeat(auto-fill, minmax(${width}px, 1fr))`,
        gap: 12,
        padding: 8,
        border: "1px solid #334155",
        borderRadius: 6,
        background: "#0f172a",
        color: "#e2e8f0",
      }}
    >
      {objects.map((obj) => {
        const isSelected = selectedId === obj.id;
        const title = readListField(obj, titleField) || obj.id;
        const subtitle = subtitleField ? readListField(obj, subtitleField) : "";
        return (
          <div
            key={obj.id}
            data-testid={`card-grid-item-${obj.id}`}
            onClick={() => onSelectObject?.(obj.id)}
            style={{
              background: "#1e293b",
              border: isSelected ? "1px solid #0ea5e9" : "1px solid #334155",
              borderRadius: 6,
              padding: 12,
              cursor: "pointer",
              boxShadow: isSelected ? "0 0 0 1px #0ea5e9" : "none",
              display: "flex",
              flexDirection: "column",
              gap: 4,
            }}
          >
            <div
              style={{
                fontSize: 13,
                fontWeight: 600,
                color: "#e2e8f0",
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {title}
            </div>
            {subtitle ? (
              <div style={{ fontSize: 11, color: "#94a3b8" }}>{subtitle}</div>
            ) : null}
            <div style={{ display: "flex", alignItems: "center", gap: 6, marginTop: 4 }}>
              {showStatus && obj.status ? (
                <span
                  style={{
                    fontSize: 10,
                    padding: "2px 6px",
                    borderRadius: 3,
                    background: "#1a4731",
                    color: "#22c55e",
                  }}
                >
                  {obj.status}
                </span>
              ) : null}
              <span style={{ fontSize: 10, color: "#64748b" }}>{formatDate(obj.updatedAt)}</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}
