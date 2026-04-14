/**
 * FacetViewRenderer — projects a FacetDefinition + data through the
 * same Puck form-input primitives used elsewhere in the builder.
 *
 * Form mode composes `TextInputRenderer` / `TextareaInputRenderer` /
 * `NumberInputRenderer` / `CheckboxInputRenderer` / `DateInputRenderer`
 * for visual consistency with standalone form-input Puck widgets —
 * facets become thin data-bound wrappers around the same primitives,
 * not a parallel rendering stack.
 *
 * The list/table/report/card modes render with lightweight shared
 * markup for multi-record browsing; those modes aren't forms and don't
 * pass through to input primitives.
 */

import { useMemo, useState } from "react";
import type { FacetDefinition, FacetLayout, FacetSlot } from "@prism/core/facet";
import type { GraphObject } from "@prism/core/object-model";
import {
  TextInputRenderer,
  TextareaInputRenderer,
  NumberInputRenderer,
  CheckboxInputRenderer,
  DateInputRenderer,
} from "./form-input-renderers.js";

// ── Types ───────────────────────────────────────────────────────────────────

export interface FacetViewProps {
  definition: FacetDefinition | undefined;
  objects: GraphObject[];
  viewMode?: FacetLayout;
  maxRows?: number;
}

type FieldSlot = Extract<FacetSlot, { kind: "field" }>;
type InputKind = "text" | "textarea" | "number" | "checkbox" | "date";

// ── Styles ──────────────────────────────────────────────────────────────────

const styles = {
  container: {
    border: "1px solid #e2e8f0",
    borderRadius: 6,
    background: "#f8fafc",
    padding: 12,
    color: "#0f172a",
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
    color: "#94a3b8",
    fontStyle: "italic" as const,
  },
  tableRow: {
    display: "flex",
    borderBottom: "1px solid #e2e8f0",
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
    color: "#64748b",
    fontSize: 10,
    textTransform: "uppercase" as const,
  },
  cardGrid: {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fill, minmax(160px, 1fr))",
    gap: 8,
  },
  card: {
    border: "1px solid #e2e8f0",
    borderRadius: 4,
    padding: 8,
    background: "#ffffff",
  },
  cardTitle: {
    fontWeight: 600 as const,
    color: "#0f172a",
    marginBottom: 4,
    fontSize: 12,
  },
} as const;

// ── Field helpers ───────────────────────────────────────────────────────────

/**
 * Walk every slot in the facet (including nested tab/popover/slide containers)
 * and yield the child field slots in the order they appear.
 */
export function flattenFieldSlots(slots: FacetSlot[]): FieldSlot[] {
  const out: FieldSlot[] = [];
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

export function getFieldPaths(def: FacetDefinition): string[] {
  return flattenFieldSlots(def.slots)
    .map((s) => s.slot.fieldPath)
    .filter(Boolean);
}

export function readFieldValue(obj: GraphObject, path: string): unknown {
  const parts = path.split(".");
  let current: unknown = obj.data as Record<string, unknown>;
  for (const part of parts) {
    if (current === null || current === undefined) return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

function formatFieldValue(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "boolean") return value ? "yes" : "no";
  return String(value);
}

/**
 * Infer which input primitive to use for a given runtime value. Falls
 * back to `text` when the value is absent so an empty record still
 * produces a full form shell. Multi-line strings route to textarea;
 * ISO-ish date strings route to date.
 */
export function inferInputKind(value: unknown): InputKind {
  if (typeof value === "boolean") return "checkbox";
  if (typeof value === "number") return "number";
  if (typeof value === "string") {
    if (value.includes("\n")) return "textarea";
    if (/^\d{4}-\d{2}-\d{2}/.test(value)) return "date";
  }
  return "text";
}

// ── Container slot renderers (tab / popover / slide) ────────────────────────

function FieldSlotRenderer({ slot, obj }: { slot: FieldSlot; obj: GraphObject | undefined }) {
  const value = obj ? readFieldValue(obj, slot.slot.fieldPath) : undefined;
  const label = slot.slot.label ?? slot.slot.fieldPath;
  const kind = inferInputKind(value);
  const stringValue = formatFieldValue(value);

  if (kind === "checkbox") {
    return <CheckboxInputRenderer label={label} defaultChecked={value === true} />;
  }
  if (kind === "number") {
    return (
      <NumberInputRenderer
        label={label}
        defaultValue={typeof value === "number" ? value : undefined}
      />
    );
  }
  if (kind === "textarea") {
    return <TextareaInputRenderer label={label} defaultValue={stringValue} />;
  }
  if (kind === "date") {
    return <DateInputRenderer label={label} defaultValue={stringValue} />;
  }
  return <TextInputRenderer label={label} defaultValue={stringValue} />;
}

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
    <div style={{ border: "1px solid #e2e8f0", borderRadius: 4, marginBottom: 8 }}>
      <div style={{ display: "flex", borderBottom: "1px solid #e2e8f0" }}>
        {tabs.map((t, i) => (
          <button
            key={t.id}
            onClick={() => setActive(i)}
            style={{
              padding: "6px 12px",
              background: i === active ? "#e2e8f0" : "transparent",
              color: i === active ? "#0f172a" : "#64748b",
              border: "none",
              borderRight: "1px solid #e2e8f0",
              cursor: "pointer",
              fontSize: 11,
            }}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div style={{ padding: 8 }}>
        {current
          ? flattenFieldSlots(current.slots).map((fs) => (
              <FieldSlotRenderer key={fs.slot.fieldPath} slot={fs} obj={obj} />
            ))
          : null}
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
          background: "#e2e8f0",
          color: "#0f172a",
          border: "1px solid #cbd5e1",
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
            background: "#ffffff",
            border: "1px solid #cbd5e1",
            borderRadius: 4,
            padding: 8,
            minWidth: 200,
            zIndex: 10,
          }}
        >
          {flattenFieldSlots(slot.slot.contentSlots).map((fs) => (
            <FieldSlotRenderer key={fs.slot.fieldPath} slot={fs} obj={obj} />
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
    <div style={{ border: "1px solid #e2e8f0", borderRadius: 4, marginBottom: 8 }}>
      <button
        onClick={() => setOpen((v) => !v)}
        style={{
          width: "100%",
          textAlign: "left",
          padding: "6px 10px",
          background: "#e2e8f0",
          color: "#0f172a",
          border: "none",
          cursor: "pointer",
          fontSize: 11,
        }}
      >
        {open ? "\u25BC" : "\u25B6"} {slot.slot.label}
      </button>
      {open && (
        <div style={{ padding: 8 }}>
          {flattenFieldSlots(slot.slot.contentSlots).map((fs) => (
            <FieldSlotRenderer key={fs.slot.fieldPath} slot={fs} obj={obj} />
          ))}
        </div>
      )}
    </div>
  );
}

// ── View renderers ──────────────────────────────────────────────────────────

function FormView({
  objects,
  definition,
}: {
  objects: GraphObject[];
  definition: FacetDefinition;
}) {
  const obj = objects[0];
  const ordered = [...definition.slots].sort((a, b) => a.slot.order - b.slot.order);
  if (ordered.length === 0) {
    return <div style={styles.empty}>No slots defined</div>;
  }
  return (
    <div>
      {ordered.map((s, i) => {
        if (s.kind === "field") {
          return <FieldSlotRenderer key={`f-${i}`} slot={s} obj={obj} />;
        }
        if (s.kind === "tab") return <TabSlotRenderer key={`t-${i}`} slot={s} obj={obj} />;
        if (s.kind === "popover") return <PopoverSlotRenderer key={`p-${i}`} slot={s} obj={obj} />;
        if (s.kind === "slide") return <SlideSlotRenderer key={`s-${i}`} slot={s} obj={obj} />;
        return null;
      })}
    </div>
  );
}

function TableView({
  objects,
  fields,
  maxRows,
}: {
  objects: GraphObject[];
  fields: string[];
  maxRows: number;
}) {
  const rows = objects.slice(0, maxRows);
  return (
    <div>
      <div style={{ ...styles.tableRow, borderBottom: "1px solid #cbd5e1" }}>
        {fields.map((f) => (
          <div key={f} style={{ ...styles.tableCell, ...styles.tableHeader }}>
            {f}
          </div>
        ))}
      </div>
      {rows.length === 0 && <div style={styles.empty}>No records</div>}
      {rows.map((obj) => (
        <div key={obj.id} style={styles.tableRow}>
          {fields.map((f) => (
            <div key={f} style={styles.tableCell}>
              {formatFieldValue(readFieldValue(obj, f))}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

function ListView({
  objects,
  fields,
  maxRows,
}: {
  objects: GraphObject[];
  fields: string[];
  maxRows: number;
}) {
  const rows = objects.slice(0, maxRows);
  const primary = fields[0] ?? "name";
  return (
    <div>
      {rows.length === 0 && <div style={styles.empty}>No records</div>}
      {rows.map((obj) => (
        <div key={obj.id} style={{ padding: "4px 0", borderBottom: "1px solid #e2e8f0" }}>
          <div style={{ color: "#0f172a", fontWeight: 500 }}>
            {formatFieldValue(readFieldValue(obj, primary)) || obj.name}
          </div>
          {fields.slice(1, 3).map((f) => (
            <div key={f} style={{ color: "#64748b", fontSize: 11 }}>
              {formatFieldValue(readFieldValue(obj, f))}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

function CardView({
  objects,
  fields,
  maxRows,
}: {
  objects: GraphObject[];
  fields: string[];
  maxRows: number;
}) {
  const rows = objects.slice(0, maxRows);
  const primary = fields[0] ?? "name";
  return (
    <div style={styles.cardGrid}>
      {rows.length === 0 && <div style={styles.empty}>No records</div>}
      {rows.map((obj) => (
        <div key={obj.id} style={styles.card}>
          <div style={styles.cardTitle}>
            {formatFieldValue(readFieldValue(obj, primary)) || obj.name}
          </div>
          {fields.slice(1, 4).map((f) => (
            <div key={f} style={{ color: "#64748b", fontSize: 11 }}>
              {formatFieldValue(readFieldValue(obj, f))}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

function ReportView({
  objects,
  fields,
  definition,
  maxRows,
}: {
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
      const key = formatFieldValue(readFieldValue(obj, groupField)) || "(blank)";
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
          <div
            style={{
              fontWeight: 600,
              color: "#0f172a",
              borderBottom: "1px solid #cbd5e1",
              paddingBottom: 2,
              marginBottom: 4,
            }}
          >
            {group.key} ({group.items.length})
          </div>
          {group.items.map((obj) => (
            <div key={obj.id} style={styles.tableRow}>
              {fields.map((f) => (
                <div key={f} style={styles.tableCell}>
                  {formatFieldValue(readFieldValue(obj, f))}
                </div>
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
      {mode === "report" && (
        <ReportView objects={objects} fields={fields} definition={definition} maxRows={maxRows} />
      )}
      {mode === "card" && <CardView objects={objects} fields={fields} maxRows={maxRows} />}
    </div>
  );
}
