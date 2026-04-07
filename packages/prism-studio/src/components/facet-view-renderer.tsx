/**
 * FacetViewRenderer — renders a FacetDefinition in the specified view mode.
 *
 * Embeddable Puck component that projects kernel objects through a
 * FacetDefinition as form/list/table/report/card views.
 */

import { useMemo } from "react";
import type { FacetDefinition, FacetLayout } from "@prism/core/facet";
import type { GraphObject } from "@prism/core/object-model";

// ── Types ───────────────────────────────────────────────────────────────────

export interface FacetViewProps {
  definition: FacetDefinition | undefined;
  objects: GraphObject[];
  viewMode?: FacetLayout;
  maxRows?: number;
}

// ── Styles ──────────────────────────────────────────────────────────────────

const styles = {
  container: {
    border: "1px solid #333",
    borderRadius: 6,
    background: "#1e1e1e",
    padding: 12,
    color: "#ccc",
    fontSize: 12,
    minHeight: 80,
  },
  header: {
    fontSize: 11,
    fontWeight: 600 as const,
    color: "#10b981",
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    marginBottom: 8,
    display: "flex",
    alignItems: "center",
    gap: 4,
  },
  empty: {
    color: "#666",
    fontStyle: "italic" as const,
  },
  formField: {
    marginBottom: 6,
    display: "flex",
    gap: 8,
  },
  formLabel: {
    width: 100,
    flexShrink: 0,
    color: "#888",
    fontSize: 11,
  },
  formValue: {
    color: "#e5e5e5",
    fontSize: 12,
  },
  tableRow: {
    display: "flex",
    borderBottom: "1px solid #2a2a2a",
    padding: "4px 0",
  },
  tableCell: {
    flex: 1,
    padding: "2px 6px",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap" as const,
  },
  tableHeader: {
    fontWeight: 600 as const,
    color: "#888",
    fontSize: 10,
    textTransform: "uppercase" as const,
  },
  cardGrid: {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fill, minmax(160px, 1fr))",
    gap: 8,
  },
  card: {
    border: "1px solid #333",
    borderRadius: 4,
    padding: 8,
    background: "#252526",
  },
  cardTitle: {
    fontWeight: 600 as const,
    color: "#e5e5e5",
    marginBottom: 4,
    fontSize: 12,
  },
} as const;

// ── Field extraction ────────────────────────────────────────────────────────

function getFieldPaths(def: FacetDefinition): string[] {
  return def.slots
    .filter((s) => s.kind === "field")
    .sort((a, b) => a.slot.order - b.slot.order)
    .map((s) => {
      if (s.kind === "field") return s.slot.fieldPath;
      return "";
    })
    .filter(Boolean);
}

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

// ── View renderers ──────────────────────────────────────────────────────────

function FormView({ objects, fields }: { objects: GraphObject[]; fields: string[] }) {
  const obj = objects[0];
  if (!obj) return <div style={styles.empty}>No record selected</div>;
  return (
    <div>
      {fields.map((f) => (
        <div key={f} style={styles.formField}>
          <span style={styles.formLabel}>{f}</span>
          <span style={styles.formValue}>{getFieldValue(obj, f) || "\u2014"}</span>
        </div>
      ))}
    </div>
  );
}

function TableView({ objects, fields, maxRows }: { objects: GraphObject[]; fields: string[]; maxRows: number }) {
  const rows = objects.slice(0, maxRows);
  return (
    <div>
      <div style={{ ...styles.tableRow, borderBottom: "1px solid #444" }}>
        {fields.map((f) => (
          <div key={f} style={{ ...styles.tableCell, ...styles.tableHeader }}>{f}</div>
        ))}
      </div>
      {rows.length === 0 && <div style={styles.empty}>No records</div>}
      {rows.map((obj) => (
        <div key={obj.id} style={styles.tableRow}>
          {fields.map((f) => (
            <div key={f} style={styles.tableCell}>{getFieldValue(obj, f)}</div>
          ))}
        </div>
      ))}
    </div>
  );
}

function ListView({ objects, fields, maxRows }: { objects: GraphObject[]; fields: string[]; maxRows: number }) {
  const rows = objects.slice(0, maxRows);
  const primary = fields[0] ?? "name";
  return (
    <div>
      {rows.length === 0 && <div style={styles.empty}>No records</div>}
      {rows.map((obj) => (
        <div key={obj.id} style={{ padding: "4px 0", borderBottom: "1px solid #2a2a2a" }}>
          <div style={{ color: "#e5e5e5", fontWeight: 500 }}>
            {getFieldValue(obj, primary) || obj.name}
          </div>
          {fields.slice(1, 3).map((f) => (
            <div key={f} style={{ color: "#888", fontSize: 11 }}>{getFieldValue(obj, f)}</div>
          ))}
        </div>
      ))}
    </div>
  );
}

function CardView({ objects, fields, maxRows }: { objects: GraphObject[]; fields: string[]; maxRows: number }) {
  const rows = objects.slice(0, maxRows);
  const primary = fields[0] ?? "name";
  return (
    <div style={styles.cardGrid}>
      {rows.length === 0 && <div style={styles.empty}>No records</div>}
      {rows.map((obj) => (
        <div key={obj.id} style={styles.card}>
          <div style={styles.cardTitle}>
            {getFieldValue(obj, primary) || obj.name}
          </div>
          {fields.slice(1, 4).map((f) => (
            <div key={f} style={{ color: "#888", fontSize: 11 }}>{getFieldValue(obj, f)}</div>
          ))}
        </div>
      ))}
    </div>
  );
}

function ReportView({ objects, fields, definition, maxRows }: {
  objects: GraphObject[];
  fields: string[];
  definition: FacetDefinition;
  maxRows: number;
}) {
  const groupField = definition.groupByField;
  const rows = objects.slice(0, maxRows);

  const groups = useMemo(() => {
    if (!groupField) return [{ key: "All", items: rows }];
    const map = new Map<string, GraphObject[]>();
    for (const obj of rows) {
      const key = getFieldValue(obj, groupField) || "(blank)";
      const list = map.get(key) ?? [];
      list.push(obj);
      map.set(key, list);
    }
    return Array.from(map.entries()).map(([key, items]) => ({ key, items }));
  }, [groupField, rows]);

  return (
    <div>
      {groups.map((group) => (
        <div key={group.key} style={{ marginBottom: 8 }}>
          <div style={{ fontWeight: 600, color: "#e5e5e5", borderBottom: "1px solid #444", paddingBottom: 2, marginBottom: 4 }}>
            {group.key} ({group.items.length})
          </div>
          {group.items.map((obj) => (
            <div key={obj.id} style={styles.tableRow}>
              {fields.map((f) => (
                <div key={f} style={styles.tableCell}>{getFieldValue(obj, f)}</div>
              ))}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

// ── Main Component ──────────────────────────────────────────────────────────

export function FacetViewRenderer({
  definition,
  objects,
  viewMode,
  maxRows = 25,
}: FacetViewProps) {
  const mode = viewMode ?? definition?.layout ?? "form";
  const fields = useMemo(() => (definition ? getFieldPaths(definition) : []), [definition]);

  if (!definition) {
    return (
      <div style={styles.container} data-testid="facet-view">
        <div style={styles.header}>{"\uD83D\uDCCB"} Facet View</div>
        <div style={styles.empty}>No facet definition bound. Set a Facet Definition ID.</div>
      </div>
    );
  }

  return (
    <div style={styles.container} data-testid="facet-view">
      <div style={styles.header}>
        {"\uD83D\uDCCB"} {definition.name} ({mode})
      </div>
      {mode === "form" && <FormView objects={objects} fields={fields} />}
      {mode === "list" && <ListView objects={objects} fields={fields} maxRows={maxRows} />}
      {mode === "table" && <TableView objects={objects} fields={fields} maxRows={maxRows} />}
      {mode === "report" && <ReportView objects={objects} fields={fields} definition={definition} maxRows={maxRows} />}
      {mode === "card" && <CardView objects={objects} fields={fields} maxRows={maxRows} />}
    </div>
  );
}
