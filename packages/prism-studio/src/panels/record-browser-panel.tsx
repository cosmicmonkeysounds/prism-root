/**
 * RecordBrowserPanel — FileMaker Pro-inspired unified data browser.
 *
 * A single panel that lets users switch between Form, List, Table, Report,
 * and Card views of their kernel objects. Includes record navigation,
 * type filtering, search, and new record creation.
 */

import { useState, useCallback, useMemo } from "react";
import { useKernel, useObjects, useSelection, useFacetDefinitions } from "../kernel/index.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";

// ── Types ──────────────────────────────────────────────────────────────────

type BrowseMode = "form" | "list" | "table" | "report" | "card";
type SortDir = "asc" | "desc";

// ── Styles ─────────────────────────────────────────────────────────────────

const styles = {
  container: {
    display: "flex",
    flexDirection: "column" as const,
    height: "100%",
    fontFamily: "system-ui, -apple-system, sans-serif",
    color: "#ccc",
    background: "#1e1e1e",
  },
  toolbar: {
    display: "flex",
    gap: 8,
    alignItems: "center",
    flexWrap: "wrap" as const,
    padding: "0.625rem 1rem",
    borderBottom: "1px solid #333",
    background: "#252526",
  },
  toolbarGroup: {
    display: "flex",
    gap: 4,
    alignItems: "center",
  },
  modeBtn: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#333",
    border: "1px solid #444",
    borderRadius: 3,
    color: "#ccc",
    cursor: "pointer",
  },
  modeBtnActive: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#0e639c",
    border: "1px solid #1177bb",
    borderRadius: 3,
    color: "#fff",
    cursor: "pointer",
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
  },
  content: {
    flex: 1,
    overflow: "auto",
    padding: "1rem",
  },
  statusBar: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "0.375rem 1rem",
    borderTop: "1px solid #333",
    background: "#252526",
    fontSize: "0.6875rem",
    color: "#888",
  },
  card: {
    background: "#252526",
    border: "1px solid #333",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    marginBottom: "0.5rem",
  },
  navLabel: {
    fontSize: "0.6875rem",
    color: "#aaa",
    minWidth: 90,
    textAlign: "center" as const,
  },
  fieldRow: {
    display: "flex",
    flexDirection: "column" as const,
    gap: "0.25rem",
    marginBottom: "0.625rem",
  },
  label: {
    fontSize: "0.75rem",
    fontWeight: 500,
    color: "#aaa",
  },
  fieldInput: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.375rem 0.5rem",
    color: "#e5e5e5",
    fontSize: "0.875rem",
    width: "100%",
    outline: "none",
    boxSizing: "border-box" as const,
  },
  badge: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#1a4731",
    color: "#22c55e",
  },
  badgeBlue: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#1a3350",
    color: "#4fc1ff",
  },
  sectionTitle: {
    fontSize: "0.75rem",
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#888",
    marginBottom: "0.375rem",
    marginTop: "0.75rem",
  },
  meta: {
    fontSize: "0.6875rem",
    color: "#666",
  },
  // Table styles
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
  sortArrow: {
    marginLeft: "0.25rem",
    fontSize: "0.625rem",
    color: "#4fc1ff",
  },
  // List styles
  listItem: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "0.5rem 0.75rem",
    borderBottom: "1px solid #2a2a2a",
    cursor: "pointer",
  },
  listItemSelected: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "0.5rem 0.75rem",
    borderBottom: "1px solid #2a2a2a",
    cursor: "pointer",
    background: "#1a3350",
  },
  listName: {
    fontSize: "0.875rem",
    color: "#e5e5e5",
    fontWeight: 500,
  },
  listMeta: {
    display: "flex",
    gap: 8,
    alignItems: "center",
  },
  // Card grid styles
  cardGrid: {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
    gap: "0.75rem",
  },
  cardItem: {
    background: "#252526",
    border: "1px solid #333",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    cursor: "pointer",
  },
  cardItemSelected: {
    background: "#252526",
    border: "1px solid #0e639c",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    cursor: "pointer",
    boxShadow: "0 0 0 1px #0e639c",
  },
  cardName: {
    fontSize: "0.875rem",
    fontWeight: 600,
    color: "#e5e5e5",
    marginBottom: "0.375rem",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap" as const,
  },
  cardType: {
    fontSize: "0.6875rem",
    color: "#888",
    marginBottom: "0.25rem",
  },
  cardDate: {
    fontSize: "0.625rem",
    color: "#555",
    marginTop: "0.375rem",
  },
  // Report styles
  groupHeader: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "0.5rem 0.75rem",
    background: "#2a2d35",
    borderRadius: "0.25rem",
    marginBottom: "0.25rem",
    marginTop: "0.75rem",
  },
  groupTitle: {
    fontSize: "0.875rem",
    fontWeight: 600,
    color: "#e5e5e5",
  },
  groupCount: {
    fontSize: "0.6875rem",
    color: "#888",
    background: "#333",
    padding: "0.125rem 0.5rem",
    borderRadius: "0.75rem",
  },
  grandSummary: {
    background: "#1a2332",
    border: "1px solid #1a3350",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    marginTop: "1rem",
  },
  empty: {
    color: "#555",
    fontStyle: "italic" as const,
    textAlign: "center" as const,
    padding: "2rem 0",
  },
} as const;

// ── Mode labels ────────────────────────────────────────────────────────────

const MODE_LABELS: Record<BrowseMode, string> = {
  form: "Form",
  list: "List",
  table: "Table",
  report: "Report",
  card: "Card",
};

const ALL_MODES: BrowseMode[] = ["form", "list", "table", "report", "card"];

// ── Table columns ──────────────────────────────────────────────────────────

interface TableColumn {
  id: string;
  label: string;
  accessor: (obj: GraphObject) => string;
}

const TABLE_COLUMNS: TableColumn[] = [
  { id: "name", label: "Name", accessor: (o) => o.name },
  { id: "type", label: "Type", accessor: (o) => o.type },
  { id: "status", label: "Status", accessor: (o) => o.status ?? "" },
  { id: "tags", label: "Tags", accessor: (o) => o.tags.join(", ") },
  {
    id: "updatedAt",
    label: "Updated",
    accessor: (o) => {
      const d = new Date(o.updatedAt);
      return `${d.toLocaleDateString()} ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
    },
  },
];

// ── Helpers ────────────────────────────────────────────────────────────────

function formatDate(iso: string): string {
  const d = new Date(iso);
  return `${d.toLocaleDateString()} ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
}

// ── Form Mode ──────────────────────────────────────────────────────────────

function FormView({
  object,
  formFields,
  onFieldChange,
}: {
  object: GraphObject;
  formFields: string[];
  onFieldChange: (field: string, value: string) => void;
}) {
  const fields = formFields.length > 0
    ? formFields
    : ["name", "type", "status", "tags", "description"];

  return (
    <div data-testid="form-view">
      <div style={styles.card}>
        <div style={{ ...styles.sectionTitle, marginTop: 0 }}>
          Record: {object.name}
        </div>
        {fields.map((field) => {
          const value = getFieldValue(object, field);
          const isReadonly = field === "type" || field === "id" || field === "createdAt" || field === "updatedAt";

          return (
            <div key={field} style={styles.fieldRow}>
              <label style={styles.label}>{field}</label>
              <input
                style={{
                  ...styles.fieldInput,
                  ...(isReadonly ? { opacity: 0.6, cursor: "default" } : {}),
                }}
                value={value}
                readOnly={isReadonly}
                onChange={(e) => onFieldChange(field, e.target.value)}
                data-testid={`form-field-${field}`}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}

function getFieldValue(obj: GraphObject, field: string): string {
  switch (field) {
    case "name": return obj.name;
    case "type": return obj.type;
    case "status": return obj.status ?? "";
    case "tags": return obj.tags.join(", ");
    case "description": return obj.description;
    case "id": return obj.id as string;
    case "createdAt": return formatDate(obj.createdAt);
    case "updatedAt": return formatDate(obj.updatedAt);
    case "position": return String(obj.position);
    case "color": return obj.color ?? "";
    default: {
      const dataVal = obj.data[field];
      return dataVal !== undefined && dataVal !== null ? String(dataVal) : "";
    }
  }
}

// ── List Mode ──────────────────────────────────────────────────────────────

function ListView({
  objects,
  selectedId,
  onSelect,
}: {
  objects: GraphObject[];
  selectedId: ObjectId | null;
  onSelect: (id: ObjectId) => void;
}) {
  if (objects.length === 0) {
    return <div style={styles.empty} data-testid="list-view-empty">No records to display.</div>;
  }

  return (
    <div data-testid="list-view">
      {objects.map((obj) => (
        <div
          key={obj.id as string}
          style={selectedId === obj.id ? styles.listItemSelected : styles.listItem}
          onClick={() => onSelect(obj.id)}
          data-testid={`list-item-${obj.id}`}
        >
          <div>
            <div style={styles.listName}>{obj.name}</div>
            <div style={styles.meta}>{obj.type}</div>
          </div>
          <div style={styles.listMeta}>
            {obj.status && <span style={styles.badge}>{obj.status}</span>}
            <span style={styles.meta}>{formatDate(obj.updatedAt)}</span>
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Table Mode ─────────────────────────────────────────────────────────────

function TableView({
  objects,
  selectedId,
  onSelect,
  sortCol,
  sortDir,
  onSort,
}: {
  objects: GraphObject[];
  selectedId: ObjectId | null;
  onSelect: (id: ObjectId) => void;
  sortCol: string;
  sortDir: SortDir;
  onSort: (colId: string) => void;
}) {
  if (objects.length === 0) {
    return <div style={styles.empty} data-testid="table-view-empty">No records to display.</div>;
  }

  return (
    <div data-testid="table-view">
      <table style={styles.table}>
        <thead>
          <tr>
            {TABLE_COLUMNS.map((col) => (
              <th
                key={col.id}
                style={styles.th}
                onClick={() => onSort(col.id)}
                data-testid={`browser-th-${col.id}`}
              >
                {col.label}
                {sortCol === col.id && (
                  <span style={styles.sortArrow}>
                    {sortDir === "asc" ? "\u25B2" : "\u25BC"}
                  </span>
                )}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {objects.map((obj) => (
            <tr
              key={obj.id as string}
              style={selectedId === obj.id ? styles.rowSelected : undefined}
              onClick={() => onSelect(obj.id)}
              data-testid={`browser-row-${obj.id}`}
            >
              {TABLE_COLUMNS.map((col) => (
                <td key={col.id} style={styles.td}>{col.accessor(obj)}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ── Report Mode ────────────────────────────────────────────────────────────

interface GroupData {
  key: string;
  objects: GraphObject[];
}

function ReportView({ objects }: { objects: GraphObject[] }) {
  const groups = useMemo(() => {
    const map = new Map<string, GraphObject[]>();
    for (const obj of objects) {
      const key = obj.type;
      const list = map.get(key) ?? [];
      list.push(obj);
      map.set(key, list);
    }
    const result: GroupData[] = [];
    for (const [key, objs] of map) {
      result.push({ key, objects: objs });
    }
    result.sort((a, b) => a.key.localeCompare(b.key));
    return result;
  }, [objects]);

  if (objects.length === 0) {
    return <div style={styles.empty} data-testid="report-view-empty">No records to display.</div>;
  }

  return (
    <div data-testid="report-view">
      {groups.map((group, idx) => (
        <div key={group.key} data-testid={`report-group-${idx}`}>
          <div style={styles.groupHeader}>
            <span style={styles.groupTitle}>{group.key}</span>
            <span style={styles.groupCount}>{group.objects.length} items</span>
          </div>
          {group.objects.map((obj) => (
            <div
              key={obj.id as string}
              style={{
                padding: "0.25rem 0.75rem",
                borderBottom: "1px solid #2a2a2a",
                fontSize: "0.8125rem",
                display: "flex",
                justifyContent: "space-between",
              }}
              data-testid={`report-row-${obj.id}`}
            >
              <span style={{ color: "#e5e5e5" }}>{obj.name}</span>
              <span style={{ color: "#4fc1ff", fontSize: "0.6875rem" }}>
                {obj.status ?? "--"}
              </span>
            </div>
          ))}
        </div>
      ))}

      {/* Grand total */}
      <div style={styles.grandSummary} data-testid="report-grand-summary">
        <div style={{ ...styles.sectionTitle, marginTop: 0 }}>Grand Total</div>
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.8125rem" }}>
          <span style={{ color: "#888" }}>Total Records</span>
          <span style={{ color: "#4fc1ff", fontWeight: 600 }}>{objects.length}</span>
        </div>
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.8125rem", marginTop: 4 }}>
          <span style={{ color: "#888" }}>Groups</span>
          <span style={{ color: "#4fc1ff", fontWeight: 600 }}>{groups.length}</span>
        </div>
      </div>
    </div>
  );
}

// ── Card Mode ──────────────────────────────────────────────────────────────

function CardView({
  objects,
  selectedId,
  onSelect,
}: {
  objects: GraphObject[];
  selectedId: ObjectId | null;
  onSelect: (id: ObjectId) => void;
}) {
  if (objects.length === 0) {
    return <div style={styles.empty} data-testid="card-view-empty">No records to display.</div>;
  }

  return (
    <div style={styles.cardGrid} data-testid="card-view">
      {objects.map((obj) => (
        <div
          key={obj.id as string}
          style={selectedId === obj.id ? styles.cardItemSelected : styles.cardItem}
          onClick={() => onSelect(obj.id)}
          data-testid={`card-${obj.id}`}
        >
          <div style={styles.cardName}>{obj.name}</div>
          <div style={styles.cardType}>{obj.type}</div>
          {obj.status && (
            <span style={styles.badge}>{obj.status}</span>
          )}
          <div style={styles.cardDate}>{formatDate(obj.updatedAt)}</div>
        </div>
      ))}
    </div>
  );
}

// ── Main Panel ─────────────────────────────────────────────────────────────

export function RecordBrowserPanel() {
  const kernel = useKernel();
  const objects = useObjects();
  const { selectedId, select } = useSelection();
  const { definitions } = useFacetDefinitions();

  // State
  const [mode, setMode] = useState<BrowseMode>("list");
  const [search, setSearch] = useState("");
  const [typeFilter, setTypeFilter] = useState("");
  const [sortCol, setSortCol] = useState("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");

  // Unique types for filter dropdown
  const types = useMemo(() => {
    const set = new Set(objects.map((o) => o.type));
    return [...set].sort();
  }, [objects]);

  // Filter + sort
  const filtered = useMemo(() => {
    let result = objects;

    if (typeFilter) {
      result = result.filter((o) => o.type === typeFilter);
    }

    if (search) {
      const lower = search.toLowerCase();
      result = result.filter(
        (o) =>
          o.name.toLowerCase().includes(lower) ||
          (o.status ?? "").toLowerCase().includes(lower) ||
          o.type.toLowerCase().includes(lower),
      );
    }

    const col = TABLE_COLUMNS.find((c) => c.id === sortCol);
    if (col) {
      result = [...result].sort((a, b) => {
        const aVal = col.accessor(a);
        const bVal = col.accessor(b);
        const cmp = aVal.localeCompare(bVal, undefined, { numeric: true });
        return sortDir === "asc" ? cmp : -cmp;
      });
    }

    return result;
  }, [objects, typeFilter, search, sortCol, sortDir]);

  // Record navigation index
  const currentIndex = useMemo(() => {
    if (!selectedId) return -1;
    return filtered.findIndex((o) => o.id === selectedId);
  }, [filtered, selectedId]);

  const selectedObject = useMemo(() => {
    if (!selectedId) return undefined;
    return filtered.find((o) => o.id === selectedId);
  }, [filtered, selectedId]);

  // Form field ordering from FacetDefinition
  const formFields = useMemo(() => {
    if (!selectedObject) return [];
    const formDef = definitions.find(
      (d) => d.layout === "form" && d.objectType === selectedObject.type,
    );
    if (formDef && formDef.slots.length > 0) {
      return formDef.slots
        .filter((s) => s.kind === "field")
        .map((s) => {
          if (s.kind === "field") return s.slot.fieldPath;
          return "";
        })
        .filter(Boolean);
    }
    return [];
  }, [definitions, selectedObject]);

  // Handlers
  const handlePrev = useCallback(() => {
    if (currentIndex > 0) {
      const prev = filtered[currentIndex - 1];
      if (prev) select(prev.id);
    }
  }, [currentIndex, filtered, select]);

  const handleNext = useCallback(() => {
    if (currentIndex < filtered.length - 1) {
      const next = filtered[currentIndex + 1];
      if (next) select(next.id);
    }
  }, [currentIndex, filtered, select]);

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

  const handleNewRecord = useCallback(() => {
    const obj = kernel.createObject({
      type: typeFilter || "page",
      name: "New Record",
      parentId: null,
      position: objects.length,
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
    select(obj.id);
    kernel.notifications.add({ title: `Created "${obj.name}"`, kind: "info" });
  }, [kernel, objects.length, typeFilter, select]);

  const handleFieldChange = useCallback(
    (field: string, value: string) => {
      if (!selectedId) return;
      if (field === "tags") {
        kernel.updateObject(selectedId, {
          tags: value.split(",").map((s) => s.trim()).filter(Boolean),
        } as Partial<GraphObject>);
      } else {
        kernel.updateObject(selectedId, { [field]: value } as Partial<GraphObject>);
      }
    },
    [kernel, selectedId],
  );

  // ── Render ─────────────────────────────────────────────────────────────

  return (
    <div style={styles.container} data-testid="record-browser-panel">
      {/* Top toolbar */}
      <div style={styles.toolbar as React.CSSProperties}>
        {/* Mode tabs */}
        <div style={styles.toolbarGroup as React.CSSProperties}>
          {ALL_MODES.map((m) => (
            <button
              key={m}
              style={mode === m ? styles.modeBtnActive : styles.modeBtn}
              onClick={() => setMode(m)}
              data-testid={`mode-btn-${m}`}
            >
              {MODE_LABELS[m]}
            </button>
          ))}
        </div>

        {/* Separator */}
        <div style={{ width: 1, height: 20, background: "#444" }} />

        {/* Record navigation */}
        <div style={styles.toolbarGroup as React.CSSProperties}>
          <button
            style={styles.btn}
            onClick={handlePrev}
            disabled={currentIndex <= 0}
            data-testid="nav-prev-btn"
          >
            &lt; Prev
          </button>
          <span style={styles.navLabel} data-testid="nav-position">
            {currentIndex >= 0
              ? `Record ${currentIndex + 1} of ${filtered.length}`
              : `${filtered.length} records`}
          </span>
          <button
            style={styles.btn}
            onClick={handleNext}
            disabled={currentIndex < 0 || currentIndex >= filtered.length - 1}
            data-testid="nav-next-btn"
          >
            Next &gt;
          </button>
        </div>

        {/* Separator */}
        <div style={{ width: 1, height: 20, background: "#444" }} />

        {/* Type filter */}
        <select
          style={{ ...styles.input, width: 120 }}
          value={typeFilter}
          onChange={(e) => setTypeFilter(e.target.value)}
          data-testid="browser-type-filter"
        >
          <option value="">All types</option>
          {types.map((t) => (
            <option key={t} value={t}>{t}</option>
          ))}
        </select>

        {/* Search */}
        <input
          style={{ ...styles.input, width: 160 }}
          placeholder="Search..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          data-testid="browser-search-input"
        />

        {/* New Record */}
        <button
          style={styles.btnPrimary}
          onClick={handleNewRecord}
          data-testid="new-record-btn"
        >
          + New Record
        </button>
      </div>

      {/* Main content area */}
      <div style={styles.content}>
        {mode === "form" && (
          selectedObject ? (
            <FormView
              object={selectedObject}
              formFields={formFields}
              onFieldChange={handleFieldChange}
            />
          ) : (
            <div style={styles.empty} data-testid="form-view-empty">
              Select a record to view its form. Use the navigation arrows or switch to List mode to pick one.
            </div>
          )
        )}

        {mode === "list" && (
          <ListView
            objects={filtered}
            selectedId={selectedId}
            onSelect={select}
          />
        )}

        {mode === "table" && (
          <TableView
            objects={filtered}
            selectedId={selectedId}
            onSelect={select}
            sortCol={sortCol}
            sortDir={sortDir}
            onSort={handleSort}
          />
        )}

        {mode === "report" && (
          <ReportView objects={filtered} />
        )}

        {mode === "card" && (
          <CardView
            objects={filtered}
            selectedId={selectedId}
            onSelect={select}
          />
        )}
      </div>

      {/* Status bar */}
      <div style={styles.statusBar} data-testid="browser-status-bar">
        <span data-testid="status-total">
          {filtered.length === objects.length
            ? `${objects.length} total records`
            : `${filtered.length} of ${objects.length} records`}
        </span>
        <span data-testid="status-mode">{MODE_LABELS[mode]} View</span>
      </div>
    </div>
  );
}

export default RecordBrowserPanel;
