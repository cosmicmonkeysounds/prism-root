/**
 * ListWidgetRenderer — simple data-bound list of kernel objects.
 *
 * Renders a vertically stacked list of objects with name, type, status,
 * and a timestamp. Click a row to select. Used as a drag-drop Puck widget
 * so users can compose their own record lists in any page layout.
 */

import type { GraphObject } from "@prism/core/object-model";

export interface ListWidgetProps {
  objects: GraphObject[];
  titleField?: string;
  subtitleField?: string;
  showStatus?: boolean;
  showTimestamp?: boolean;
  selectedId?: string | null;
  onSelectObject?: (id: string) => void;
}

/** Read a field from either top-level GraphObject keys or data payload. */
export function readListField(obj: GraphObject, field: string): string {
  if (!field) return "";
  switch (field) {
    case "name":
      return obj.name;
    case "type":
      return obj.type;
    case "status":
      return obj.status ?? "";
    case "tags":
      return obj.tags.join(", ");
    case "description":
      return obj.description;
    default: {
      const val = (obj.data as Record<string, unknown>)[field];
      return val == null ? "" : String(val);
    }
  }
}

function formatDate(iso: string | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return `${d.toLocaleDateString()} ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
}

export function ListWidgetRenderer(props: ListWidgetProps) {
  const {
    objects,
    titleField = "name",
    subtitleField = "type",
    showStatus = true,
    showTimestamp = true,
    selectedId = null,
    onSelectObject,
  } = props;

  if (objects.length === 0) {
    return (
      <div
        data-testid="list-widget-empty"
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
      data-testid="list-widget"
      style={{
        border: "1px solid #334155",
        borderRadius: 6,
        background: "#0f172a",
        color: "#e2e8f0",
        overflow: "hidden",
      }}
    >
      {objects.map((obj) => {
        const isSelected = selectedId === obj.id;
        return (
          <div
            key={obj.id}
            data-testid={`list-widget-row-${obj.id}`}
            onClick={() => onSelectObject?.(obj.id)}
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              padding: "8px 12px",
              borderBottom: "1px solid #1e293b",
              cursor: "pointer",
              background: isSelected ? "#1a3350" : "transparent",
            }}
          >
            <div style={{ minWidth: 0, flex: 1 }}>
              <div
                style={{
                  fontSize: 13,
                  fontWeight: 500,
                  color: "#e2e8f0",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {readListField(obj, titleField) || obj.id}
              </div>
              {subtitleField ? (
                <div style={{ fontSize: 11, color: "#94a3b8" }}>
                  {readListField(obj, subtitleField)}
                </div>
              ) : null}
            </div>
            <div style={{ display: "flex", gap: 8, alignItems: "center", flexShrink: 0 }}>
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
              {showTimestamp ? (
                <span style={{ fontSize: 11, color: "#64748b" }}>{formatDate(obj.updatedAt)}</span>
              ) : null}
            </div>
          </div>
        );
      })}
    </div>
  );
}
