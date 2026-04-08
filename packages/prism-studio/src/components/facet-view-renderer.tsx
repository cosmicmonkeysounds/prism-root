/**
 * FacetViewRenderer — renders a FacetDefinition in the specified view mode.
 *
 * Embeddable Puck component that projects kernel objects through a
 * FacetDefinition as form/list/table/report/card views.
 */

import { useMemo, useState } from "react";
import type { FacetDefinition, FacetLayout, FacetSlot } from "@prism/core/facet";
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

/**
 * Walk every slot in the facet (including nested tab/popover/slide containers)
 * and yield the child field slots in the order they appear.
 */
function flattenFieldSlots(slots: FacetSlot[]): Array<Extract<FacetSlot, { kind: "field" }>> {
  const out: Array<Extract<FacetSlot, { kind: "field" }>> = [];
  for (const s of [...slots].sort((a, b) => a.slot.order - b.slot.order)) {
    if (s.kind === "field") {
      out.push(s);
    } else if (s.kind === "tab") {
      for (const tab of s.slot.tabs) out.push(...flattenFieldSlots(tab.slots));
    } else if (s.kind === "popover") {
      out.push(...flattenFieldSlots(s.slot.contentSlots));
    } else if (s.kind === "slide") {
      out.push(...flattenFieldSlots(s.slot.contentSlots));
    }
  }
  return out;
}

function getFieldPaths(def: FacetDefinition): string[] {
  return flattenFieldSlots(def.slots)
    .map((s) => s.slot.fieldPath)
    .filter(Boolean);
}

// ── Container slot renderers (tab / popover / slide) ────────────────────────

function TabSlotRenderer({
  slot,
  obj,
}: {
  slot: Extract<FacetSlot, { kind: "tab" }>;
  obj: GraphObject | undefined;
}) {
  const [active, setActive] = useState(0);
  const tabs = slot.slot.tabs;
  const current = tabs[active];

  return (
    <div style={{ border: "1px solid #333", borderRadius: 4, marginBottom: 8 }}>
      <div style={{ display: "flex", borderBottom: "1px solid #333" }}>
        {tabs.map((t, i) => (
          <button
            key={t.id}
            onClick={() => setActive(i)}
            style={{
              padding: "6px 12px",
              background: i === active ? "#2a2a2a" : "transparent",
              color: i === active ? "#e5e5e5" : "#888",
              border: "none",
              borderRight: "1px solid #333",
              cursor: "pointer",
              fontSize: 11,
            }}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div style={{ padding: 8 }}>
        {current && obj ? (
          flattenFieldSlots(current.slots).map((fs) => (
            <div key={fs.slot.fieldPath} style={styles.formField}>
              <span style={styles.formLabel}>{fs.slot.label ?? fs.slot.fieldPath}</span>
              <span style={styles.formValue}>{getFieldValue(obj, fs.slot.fieldPath) || "\u2014"}</span>
            </div>
          ))
        ) : (
          <div style={styles.empty}>No content</div>
        )}
      </div>
    </div>
  );
}

function PopoverSlotRenderer({
  slot,
  obj,
}: {
  slot: Extract<FacetSlot, { kind: "popover" }>;
  obj: GraphObject | undefined;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div style={{ position: "relative", display: "inline-block", marginBottom: 8 }}>
      <button
        onClick={() => setOpen((v) => !v)}
        style={{
          padding: "6px 12px",
          background: "#2a2a2a",
          color: "#e5e5e5",
          border: "1px solid #444",
          borderRadius: 4,
          cursor: "pointer",
          fontSize: 11,
        }}
      >
        {slot.slot.triggerLabel}
      </button>
      {open && (
        <div
          style={{
            position: "absolute",
            top: "100%",
            left: 0,
            marginTop: 4,
            background: "#1e1e1e",
            border: "1px solid #444",
            borderRadius: 4,
            padding: 8,
            minWidth: 200,
            zIndex: 10,
          }}
        >
          {obj && flattenFieldSlots(slot.slot.contentSlots).map((fs) => (
            <div key={fs.slot.fieldPath} style={styles.formField}>
              <span style={styles.formLabel}>{fs.slot.label ?? fs.slot.fieldPath}</span>
              <span style={styles.formValue}>{getFieldValue(obj, fs.slot.fieldPath) || "\u2014"}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function SlideSlotRenderer({
  slot,
  obj,
}: {
  slot: Extract<FacetSlot, { kind: "slide" }>;
  obj: GraphObject | undefined;
}) {
  const [open, setOpen] = useState(!slot.slot.collapsed);
  return (
    <div style={{ border: "1px solid #333", borderRadius: 4, marginBottom: 8 }}>
      <button
        onClick={() => setOpen((v) => !v)}
        style={{
          width: "100%",
          textAlign: "left",
          padding: "6px 10px",
          background: "#2a2a2a",
          color: "#e5e5e5",
          border: "none",
          cursor: "pointer",
          fontSize: 11,
        }}
      >
        {open ? "\u25BC" : "\u25B6"} {slot.slot.label}
      </button>
      {open && (
        <div style={{ padding: 8 }}>
          {obj && flattenFieldSlots(slot.slot.contentSlots).map((fs) => (
            <div key={fs.slot.fieldPath} style={styles.formField}>
              <span style={styles.formLabel}>{fs.slot.label ?? fs.slot.fieldPath}</span>
              <span style={styles.formValue}>{getFieldValue(obj, fs.slot.fieldPath) || "\u2014"}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
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

function FormView({ objects, definition }: { objects: GraphObject[]; definition: FacetDefinition }) {
  const obj = objects[0];
  if (!obj) return <div style={styles.empty}>No record selected</div>;
  // Render top-level slots in order. Container slots (tab/popover/slide)
  // render as interactive containers; field slots render inline.
  const ordered = [...definition.slots].sort((a, b) => a.slot.order - b.slot.order);
  return (
    <div>
      {ordered.map((s, i) => {
        if (s.kind === "field") {
          return (
            <div key={`f-${i}`} style={styles.formField}>
              <span style={styles.formLabel}>{s.slot.label ?? s.slot.fieldPath}</span>
              <span style={styles.formValue}>{getFieldValue(obj, s.slot.fieldPath) || "\u2014"}</span>
            </div>
          );
        }
        if (s.kind === "tab") return <TabSlotRenderer key={`t-${i}`} slot={s} obj={obj} />;
        if (s.kind === "popover") return <PopoverSlotRenderer key={`p-${i}`} slot={s} obj={obj} />;
        if (s.kind === "slide") return <SlideSlotRenderer key={`s-${i}`} slot={s} obj={obj} />;
        return null;
      })}
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
      {mode === "form" && <FormView objects={objects} definition={definition} />}
      {mode === "list" && <ListView objects={objects} fields={fields} maxRows={maxRows} />}
      {mode === "table" && <TableView objects={objects} fields={fields} maxRows={maxRows} />}
      {mode === "report" && <ReportView objects={objects} fields={fields} definition={definition} maxRows={maxRows} />}
      {mode === "card" && <CardView objects={objects} fields={fields} maxRows={maxRows} />}
    </div>
  );
}
