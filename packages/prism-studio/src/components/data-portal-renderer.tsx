/**
 * DataPortalRenderer — inline display of related records via edge relationships.
 *
 * FileMaker Pro portal equivalent: shows related records in a scrollable
 * table, with optional inline creation.
 */

import { useMemo } from "react";
import type { GraphObject, ObjectEdge } from "@prism/core/object-model";

// ── Types ───────────────────────────────────────────────────────────────────

export interface DataPortalProps {
  /** All objects available for relationship lookup. */
  objects: GraphObject[];
  /** All edges (from kernel). */
  edges: readonly ObjectEdge[];
  /** The current record's ID (source of the relationship). */
  sourceId?: string;
  /** Edge relation type to filter by. */
  relationshipId: string;
  /** Which fields to display from related objects. */
  displayFields: string[];
  /** Max visible rows before scrolling. */
  visibleRows?: number;
  /** Allow inline creation of related records. */
  allowCreation?: boolean;
  /** Sort field for related records. */
  sortField?: string | undefined;
  /** Sort direction. */
  sortDirection?: "asc" | "desc" | undefined;
  /** Called when user requests creating a new related record. */
  onCreateRelated?: () => void;
}

// ── Styles ──────────────────────────────────────────────────────────────────

const styles = {
  container: {
    border: "1px solid #8b5cf6",
    borderRadius: 6,
    background: "#1e1e1e",
    overflow: "hidden",
    fontSize: 12,
    color: "#ccc",
  },
  header: {
    fontSize: 11,
    fontWeight: 600 as const,
    color: "#8b5cf6",
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    padding: "6px 10px",
    borderBottom: "1px solid #333",
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
  },
  scrollArea: {
    overflowY: "auto" as const,
  },
  row: {
    display: "flex",
    borderBottom: "1px solid #2a2a2a",
    padding: "4px 0",
  },
  cell: {
    flex: 1,
    padding: "2px 8px",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap" as const,
  },
  headerRow: {
    display: "flex",
    borderBottom: "1px solid #444",
    padding: "4px 0",
    background: "#252526",
  },
  headerCell: {
    flex: 1,
    padding: "2px 8px",
    fontWeight: 600 as const,
    color: "#888",
    fontSize: 10,
    textTransform: "uppercase" as const,
  },
  empty: {
    padding: "12px 10px",
    color: "#666",
    fontStyle: "italic" as const,
    textAlign: "center" as const,
  },
  addBtn: {
    fontSize: 10,
    padding: "2px 8px",
    background: "#333",
    border: "1px solid #444",
    borderRadius: 3,
    color: "#ccc",
    cursor: "pointer",
  },
} as const;

// ── Helpers ─────────────────────────────────────────────────────────────────

function getFieldValue(obj: GraphObject, path: string): string {
  const data = obj.data as Record<string, unknown>;
  const parts = path.split(".");
  let current: unknown = data;
  for (const part of parts) {
    if (current === null || current === undefined) return "";
    current = (current as Record<string, unknown>)[part];
  }
  if (current === null || current === undefined) return "";
  return String(current);
}

// ── Main Component ──────────────────────────────────────────────────────────

export function DataPortalRenderer({
  objects,
  edges,
  sourceId,
  relationshipId,
  displayFields,
  visibleRows = 5,
  allowCreation = false,
  sortField,
  sortDirection = "asc",
  onCreateRelated,
}: DataPortalProps) {
  const relatedObjects = useMemo(() => {
    if (!sourceId) return [];

    // Find target IDs linked by this relationship
    const targetIds = new Set(
      edges
        .filter((e) => e.sourceId === sourceId && e.relation === relationshipId)
        .map((e) => e.targetId),
    );

    // Also check reverse direction
    const reverseIds = new Set(
      edges
        .filter((e) => e.targetId === sourceId && e.relation === relationshipId)
        .map((e) => e.sourceId),
    );

    const allRelated = objects.filter(
      (o) => targetIds.has(o.id) || reverseIds.has(o.id),
    );

    // Sort if requested
    if (sortField) {
      allRelated.sort((a, b) => {
        const va = getFieldValue(a, sortField);
        const vb = getFieldValue(b, sortField);
        const cmp = va.localeCompare(vb);
        return sortDirection === "desc" ? -cmp : cmp;
      });
    }

    return allRelated;
  }, [objects, edges, sourceId, relationshipId, sortField, sortDirection]);

  const rowHeight = 28;
  const maxHeight = visibleRows * rowHeight;

  return (
    <div style={styles.container} data-testid="data-portal">
      <div style={styles.header}>
        <span>{"\uD83D\uDD17"} {relationshipId}</span>
        <span style={{ color: "#666", fontSize: 10 }}>
          {relatedObjects.length} record{relatedObjects.length !== 1 ? "s" : ""}
        </span>
        {allowCreation && (
          <button
            style={styles.addBtn}
            onClick={onCreateRelated}
            data-testid="portal-add"
          >
            + New
          </button>
        )}
      </div>

      {/* Column headers */}
      {displayFields.length > 0 && (
        <div style={styles.headerRow}>
          {displayFields.map((f) => (
            <div key={f} style={styles.headerCell}>{f}</div>
          ))}
        </div>
      )}

      {/* Scrollable data rows */}
      <div style={{ ...styles.scrollArea, maxHeight }}>
        {relatedObjects.length === 0 ? (
          <div style={styles.empty}>No related records</div>
        ) : (
          relatedObjects.map((obj) => (
            <div key={obj.id} style={styles.row}>
              {displayFields.map((f) => (
                <div key={f} style={styles.cell}>{getFieldValue(obj, f)}</div>
              ))}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
