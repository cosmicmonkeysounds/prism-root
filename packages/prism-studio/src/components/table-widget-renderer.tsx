/**
 * TableWidgetRenderer — sortable tabular view of kernel objects.
 *
 * Columns are configured as a comma-separated `columns` string, with each
 * entry pointing to either a top-level GraphObject field or a data payload
 * key. Click a column header to sort; click a row to select.
 */

import { useMemo, useState } from "react";
import type { GraphObject } from "@prism/core/object-model";

export type TableSortDir = "asc" | "desc";

export interface TableColumnConfig {
  id: string;
  label: string;
}

export interface TableWidgetProps {
  objects: GraphObject[];
  columns: TableColumnConfig[];
  sortField?: string;
  sortDir?: TableSortDir;
  selectedId?: string | null;
  onSelectObject?: (id: string) => void;
}

/** Parse a comma-separated column spec: "name:Name, status:Status, data.priority:Priority". */
export function parseTableColumns(spec: string): TableColumnConfig[] {
  if (!spec.trim()) {
    return [
      { id: "name", label: "Name" },
      { id: "type", label: "Type" },
      { id: "status", label: "Status" },
      { id: "updatedAt", label: "Updated" },
    ];
  }
  return spec
    .split(",")
    .map((entry) => entry.trim())
    .filter(Boolean)
    .map((entry) => {
      const [id = "", label = ""] = entry.split(":").map((s) => s.trim());
      return { id, label: label || id } as TableColumnConfig;
    });
}

/** Read a column value from GraphObject, looking up top-level keys or data. */
export function readCellValue(obj: GraphObject, field: string): string {
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
    case "createdAt":
      return obj.createdAt;
    case "updatedAt":
      return obj.updatedAt;
    case "position":
      return String(obj.position);
    case "color":
      return obj.color ?? "";
    default: {
      const val = (obj.data as Record<string, unknown>)[field];
      return val == null ? "" : String(val);
    }
  }
}

/** Pure sort helper — returns a new array. */
export function sortObjects(
  objects: GraphObject[],
  sortField: string,
  sortDir: TableSortDir,
): GraphObject[] {
  if (!sortField) return objects;
  const copy = [...objects];
  copy.sort((a, b) => {
    const aVal = readCellValue(a, sortField);
    const bVal = readCellValue(b, sortField);
    const cmp = aVal.localeCompare(bVal, undefined, { numeric: true });
    return sortDir === "asc" ? cmp : -cmp;
  });
  return copy;
}

export function TableWidgetRenderer(props: TableWidgetProps) {
  const {
    objects,
    columns,
    sortField: initialSort,
    sortDir: initialDir = "asc",
    selectedId = null,
    onSelectObject,
  } = props;

  const [sortField, setSortField] = useState<string>(initialSort ?? "");
  const [sortDir, setSortDir] = useState<TableSortDir>(initialDir);

  const sorted = useMemo(
    () => sortObjects(objects, sortField, sortDir),
    [objects, sortField, sortDir],
  );

  const handleSort = (colId: string) => {
    if (sortField === colId) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortField(colId);
      setSortDir("asc");
    }
  };

  if (objects.length === 0) {
    return (
      <div
        data-testid="table-widget-empty"
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
      data-testid="table-widget"
      style={{
        border: "1px solid #334155",
        borderRadius: 6,
        background: "#0f172a",
        color: "#e2e8f0",
        overflow: "auto",
      }}
    >
      <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
        <thead>
          <tr>
            {columns.map((col) => (
              <th
                key={col.id}
                data-testid={`table-widget-th-${col.id}`}
                onClick={() => handleSort(col.id)}
                style={{
                  textAlign: "left",
                  padding: "8px 10px",
                  borderBottom: "2px solid #334155",
                  fontSize: 11,
                  fontWeight: 600,
                  textTransform: "uppercase",
                  letterSpacing: "0.05em",
                  color: "#94a3b8",
                  cursor: "pointer",
                  userSelect: "none",
                  whiteSpace: "nowrap",
                }}
              >
                {col.label}
                {sortField === col.id ? (
                  <span style={{ marginLeft: 4, fontSize: 10, color: "#0ea5e9" }}>
                    {sortDir === "asc" ? "\u25B2" : "\u25BC"}
                  </span>
                ) : null}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sorted.map((obj) => {
            const isSelected = selectedId === obj.id;
            return (
              <tr
                key={obj.id}
                data-testid={`table-widget-row-${obj.id}`}
                onClick={() => onSelectObject?.(obj.id)}
                style={{
                  cursor: "pointer",
                  background: isSelected ? "#1a3350" : "transparent",
                  borderBottom: "1px solid #1e293b",
                }}
              >
                {columns.map((col) => (
                  <td
                    key={col.id}
                    style={{
                      padding: "6px 10px",
                      maxWidth: 240,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {readCellValue(obj, col.id)}
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
