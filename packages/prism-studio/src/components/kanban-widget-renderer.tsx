/**
 * KanbanWidgetRenderer — column view grouping objects by a field.
 *
 * Uses HTML5 drag-drop (native, no dependency) to reassign the group
 * field on drop. Cards render with optional color stripe.
 */

import { useMemo, useState } from "react";
import type { GraphObject } from "@prism/core/object-model";

export interface KanbanWidgetProps {
  objects: GraphObject[];
  groupField: string;
  titleField: string;
  colorField?: string | undefined;
  maxCardsPerColumn?: number;
  onMoveObject?: (objectId: string, newGroupValue: string) => void;
  onSelectObject?: (id: string) => void;
}

export interface KanbanColumn {
  value: string;
  cards: GraphObject[];
}

/** Group objects by a field value. Missing values fall into "—". */
export function groupByField(
  objects: GraphObject[],
  field: string,
): KanbanColumn[] {
  const map = new Map<string, GraphObject[]>();
  for (const obj of objects) {
    const data = obj.data as Record<string, unknown>;
    const raw = data[field];
    const key = raw == null || raw === "" ? "—" : String(raw);
    const bucket = map.get(key);
    if (bucket) {
      bucket.push(obj);
    } else {
      map.set(key, [obj]);
    }
  }
  return Array.from(map.entries()).map(([value, cards]) => ({ value, cards }));
}

export function KanbanWidgetRenderer(props: KanbanWidgetProps) {
  const { objects, groupField, titleField, colorField, maxCardsPerColumn = 50, onMoveObject, onSelectObject } = props;
  const [dragId, setDragId] = useState<string | null>(null);

  const columns = useMemo(() => groupByField(objects, groupField), [objects, groupField]);

  const handleDragStart = (e: React.DragEvent, id: string): void => {
    setDragId(id);
    e.dataTransfer.setData("text/plain", id);
    e.dataTransfer.effectAllowed = "move";
  };

  const handleDragOver = (e: React.DragEvent): void => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
  };

  const handleDrop = (e: React.DragEvent, columnValue: string): void => {
    e.preventDefault();
    const id = e.dataTransfer.getData("text/plain") || dragId;
    setDragId(null);
    if (!id || !onMoveObject) return;
    onMoveObject(id, columnValue);
  };

  return (
    <div
      data-testid="kanban-widget"
      style={{
        border: "1px solid #0ea5e9",
        borderRadius: 6,
        background: "#0f172a",
        padding: 8,
        display: "flex",
        gap: 8,
        overflowX: "auto",
        color: "#e2e8f0",
      }}
    >
      {columns.length === 0 ? (
        <div style={{ padding: 24, color: "#94a3b8", fontSize: 12 }}>No records to display.</div>
      ) : (
        columns.map((col) => (
          <div
            key={col.value}
            data-testid={`kanban-column-${col.value}`}
            onDragOver={handleDragOver}
            onDrop={(e) => handleDrop(e, col.value)}
            style={{
              minWidth: 220,
              maxWidth: 260,
              background: "#1e293b",
              borderRadius: 4,
              padding: 6,
              display: "flex",
              flexDirection: "column",
              gap: 4,
            }}
          >
            <div
              style={{
                fontSize: 11,
                fontWeight: 600,
                textTransform: "uppercase",
                letterSpacing: "0.05em",
                color: "#0ea5e9",
                padding: "2px 4px",
                display: "flex",
                justifyContent: "space-between",
              }}
            >
              <span>{col.value}</span>
              <span style={{ color: "#94a3b8" }}>{col.cards.length}</span>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 4, maxHeight: 400, overflowY: "auto" }}>
              {col.cards.slice(0, maxCardsPerColumn).map((card) => {
                const data = card.data as Record<string, unknown>;
                const title = String(data[titleField] ?? card.id);
                const color = colorField ? String(data[colorField] ?? "") : "";
                return (
                  <div
                    key={card.id}
                    data-testid={`kanban-card-${card.id}`}
                    draggable
                    onDragStart={(e) => handleDragStart(e, card.id)}
                    onClick={() => onSelectObject?.(card.id)}
                    style={{
                      background: "#0f172a",
                      border: "1px solid #334155",
                      borderLeft: color ? `3px solid ${color}` : "1px solid #334155",
                      padding: "6px 8px",
                      borderRadius: 3,
                      cursor: "grab",
                      fontSize: 12,
                    }}
                  >
                    {title}
                  </div>
                );
              })}
              {col.cards.length > maxCardsPerColumn ? (
                <div style={{ fontSize: 10, color: "#94a3b8", textAlign: "center" }}>
                  +{col.cards.length - maxCardsPerColumn} more
                </div>
              ) : null}
            </div>
          </div>
        ))
      )}
    </div>
  );
}
