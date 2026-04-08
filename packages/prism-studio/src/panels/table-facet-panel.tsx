/**
 * TableFacet Panel — data grid view of kernel objects.
 *
 * Displays objects in a sortable, filterable table with inline editing.
 * Column definitions derived from EntityFieldDef via ObjectRegistry.
 * Keyboard navigation: arrow keys, tab, enter to edit.
 */

import { useState, useCallback, useMemo, useRef } from "react";
import { useKernel, useObjects, useSelection } from "../kernel/index.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
// ── Styles ──────────────────────────────────────────────────────────────────

const styles = {
  container: {
    padding: "1rem",
    height: "100%",
    overflow: "auto",
    fontFamily: "system-ui, -apple-system, sans-serif",
    color: "#ccc",
    background: "#1e1e1e",
  },
  header: {
    fontSize: "1.25rem",
    fontWeight: 600,
    marginBottom: "1rem",
    color: "#e5e5e5",
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
  },
  btn: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#333",
    border: "1px solid #444",
    borderRadius: 3,
    color: "#ccc",
    cursor: "pointer",
  },
  btnPrimary: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#0e639c",
    border: "1px solid #1177bb",
    borderRadius: 3,
    color: "#fff",
    cursor: "pointer",
  },
  input: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.25rem 0.375rem",
    color: "#e5e5e5",
    fontSize: "0.75rem",
    outline: "none",
    boxSizing: "border-box" as const,
    width: "100%",
  },
  table: {
    width: "100%",
    borderCollapse: "collapse" as const,
    fontSize: "0.8125rem",
  },
  th: {
    textAlign: "left" as const,
    padding: "0.375rem 0.5rem",
    borderBottom: "2px solid #333",
    fontSize: "0.6875rem",
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#888",
    cursor: "pointer",
    userSelect: "none" as const,
    whiteSpace: "nowrap" as const,
  },
  td: {
    padding: "0.375rem 0.5rem",
    borderBottom: "1px solid #2a2a2a",
    maxWidth: 200,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap" as const,
  },
  rowSelected: {
    background: "#1a3350",
  },
  rowHover: {
    background: "#252526",
  },
  sortArrow: {
    marginLeft: "0.25rem",
    fontSize: "0.625rem",
    color: "#4fc1ff",
  },
  badge: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#1a4731",
    color: "#22c55e",
    marginLeft: "0.375rem",
  },
  filterBar: {
    display: "flex",
    gap: 6,
    marginBottom: "0.75rem",
    alignItems: "center",
  },
  meta: {
    fontSize: "0.6875rem",
    color: "#666",
  },
} as const;

// ── Column definition ───────────────────────────────────────────────────────

interface Column {
  id: string;
  label: string;
  accessor: (obj: GraphObject) => string;
  editable?: boolean;
  width?: number;
}

const BASE_COLUMNS: Column[] = [
  { id: "name", label: "Name", accessor: (o) => o.name, editable: true },
  { id: "type", label: "Type", accessor: (o) => o.type },
  { id: "status", label: "Status", accessor: (o) => o.status ?? "", editable: true },
  { id: "tags", label: "Tags", accessor: (o) => o.tags?.join(", ") ?? "" },
  { id: "position", label: "Pos", accessor: (o) => String(o.position ?? 0), width: 40 },
  { id: "updatedAt", label: "Updated", accessor: (o) => {
    if (!o.updatedAt) return "";
    const d = new Date(o.updatedAt);
    return `${d.toLocaleDateString()} ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
  }},
];

type SortDir = "asc" | "desc";

// ── Inline Edit Cell ────────────────────────────────────────────────────────

function EditableCell({
  value,
  onCommit,
  testId,
}: {
  value: string;
  onCommit: (value: string) => void;
  testId: string;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const inputRef = useRef<HTMLInputElement>(null);

  const startEdit = useCallback(() => {
    setDraft(value);
    setEditing(true);
    setTimeout(() => inputRef.current?.focus(), 0);
  }, [value]);

  const commit = useCallback(() => {
    setEditing(false);
    if (draft !== value) onCommit(draft);
  }, [draft, value, onCommit]);

  if (editing) {
    return (
      <input
        ref={inputRef}
        style={styles.input}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
          if (e.key === "Escape") setEditing(false);
        }}
        data-testid={`${testId}-input`}
      />
    );
  }

  return (
    <span
      onDoubleClick={startEdit}
      style={{ cursor: "pointer" }}
      data-testid={testId}
    >
      {value || <span style={{ color: "#555" }}>—</span>}
    </span>
  );
}

// ── Main Panel ──────────────────────────────────────────────────────────────

export function TableFacetPanel() {
  const kernel = useKernel();
  const objects = useObjects();
  const { selectedId, select } = useSelection();

  const [sortCol, setSortCol] = useState<string>("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [filter, setFilter] = useState("");
  const [typeFilter, setTypeFilter] = useState<string>("");

  // Unique types for filter dropdown
  const types = useMemo(() => {
    const set = new Set(objects.map((o) => o.type));
    return [...set].sort();
  }, [objects]);

  // Filter + sort objects
  const rows = useMemo(() => {
    let filtered = objects;

    if (typeFilter) {
      filtered = filtered.filter((o) => o.type === typeFilter);
    }

    if (filter) {
      const lower = filter.toLowerCase();
      filtered = filtered.filter(
        (o) =>
          o.name.toLowerCase().includes(lower) ||
          (o.status ?? "").toLowerCase().includes(lower) ||
          o.type.toLowerCase().includes(lower),
      );
    }

    const col = BASE_COLUMNS.find((c) => c.id === sortCol);
    if (col) {
      filtered = [...filtered].sort((a, b) => {
        const aVal = col.accessor(a);
        const bVal = col.accessor(b);
        const cmp = aVal.localeCompare(bVal, undefined, { numeric: true });
        return sortDir === "asc" ? cmp : -cmp;
      });
    }

    return filtered;
  }, [objects, filter, typeFilter, sortCol, sortDir]);

  const handleSort = useCallback(
    (colId: string) => {
      if (sortCol === colId) {
        setSortDir((d) => (d === "asc" ? "desc" : "asc"));
      } else {
        setSortCol(colId);
        setSortDir("asc");
      }
    },
    [sortCol],
  );

  const handleCellEdit = useCallback(
    (objId: ObjectId, field: string, value: string) => {
      kernel.updateObject(objId, { [field]: value } as Partial<GraphObject>);
    },
    [kernel],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTableElement>) => {
      if (!selectedId || rows.length === 0) return;

      const currentIdx = rows.findIndex((o) => o.id === selectedId);
      if (currentIdx === -1) return;

      if (e.key === "ArrowDown" && currentIdx < rows.length - 1) {
        const next = rows[currentIdx + 1];
        if (next) { e.preventDefault(); select(next.id); }
      } else if (e.key === "ArrowUp" && currentIdx > 0) {
        const prev = rows[currentIdx - 1];
        if (prev) { e.preventDefault(); select(prev.id); }
      }
    },
    [selectedId, rows, select],
  );

  return (
    <div style={styles.container} data-testid="table-facet-panel">
      <div style={styles.header as React.CSSProperties}>
        <span>Table Facet</span>
        <span style={styles.meta}>{rows.length} / {objects.length} objects</span>
      </div>

      {/* Filter bar */}
      <div style={styles.filterBar as React.CSSProperties}>
        <input
          style={{ ...styles.input, width: 200 }}
          placeholder="Filter..."
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          data-testid="table-filter-input"
        />
        <select
          style={{ ...styles.input, width: 120 }}
          value={typeFilter}
          onChange={(e) => setTypeFilter(e.target.value)}
          data-testid="table-type-filter"
        >
          <option value="">All types</option>
          {types.map((t) => (
            <option key={t} value={t}>{t}</option>
          ))}
        </select>
      </div>

      {/* Table */}
      <table
        style={styles.table}
        tabIndex={0}
        onKeyDown={handleKeyDown}
        data-testid="table-grid"
      >
        <thead>
          <tr>
            {BASE_COLUMNS.map((col) => (
              <th
                key={col.id}
                style={{ ...styles.th, width: col.width }}
                onClick={() => handleSort(col.id)}
                data-testid={`th-${col.id}`}
              >
                {col.label}
                {sortCol === col.id && (
                  <span style={styles.sortArrow}>{sortDir === "asc" ? "\u25B2" : "\u25BC"}</span>
                )}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.length === 0 ? (
            <tr>
              <td colSpan={BASE_COLUMNS.length} style={{ ...styles.td, color: "#555", textAlign: "center" }}>
                No objects to display.
              </td>
            </tr>
          ) : (
            rows.map((obj) => (
              <tr
                key={obj.id as string}
                style={selectedId === obj.id ? styles.rowSelected : undefined}
                onClick={() => select(obj.id)}
                data-testid={`row-${obj.id}`}
              >
                {BASE_COLUMNS.map((col) =>
                  col.editable ? (
                    <td key={col.id} style={styles.td}>
                      <EditableCell
                        value={col.accessor(obj)}
                        onCommit={(v) => handleCellEdit(obj.id, col.id, v)}
                        testId={`cell-${obj.id}-${col.id}`}
                      />
                    </td>
                  ) : (
                    <td key={col.id} style={{ ...styles.td, width: col.width }}>
                      {col.accessor(obj)}
                    </td>
                  ),
                )}
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const TABLE_FACET_LENS_ID = lensId("table-facet");

export const tableFacetLensManifest: LensManifest = {

  id: TABLE_FACET_LENS_ID,
  name: "Table",
  icon: "\uD83D\uDCCA",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-table-facet", name: "Switch to Table Facet", shortcut: ["b"], section: "Navigation" }],
  },
};

export const tableFacetLensBundle: LensBundle = defineLensBundle(
  tableFacetLensManifest,
  TableFacetPanel,
);
