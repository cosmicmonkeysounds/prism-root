/**
 * FacetDesigner Panel — visual layout builder for FacetDefinitions.
 *
 * Inspired by FileMaker Pro's Layout Mode. Users create and edit
 * FacetDefinitions: picking layout type, adding layout parts, field slots,
 * portal slots, summary fields, sort/group rules, and automation hooks.
 * Saves the definition to the kernel facet registry.
 */

import { useState, useCallback, useMemo } from "react";
import { useFacetDefinitions } from "../kernel/index.js";
import type {
  FacetLayout,
  LayoutPartKind,
  LayoutPart,
  FacetDefinition,
  FacetSlot,
  FieldSlot,
  PortalSlot,
  SummaryField,
} from "@prism/core/facet";

// ── Styles ──────────────────────────────────────────────────────────────────

const styles = {
  container: {
    display: "flex",
    height: "100%",
    fontFamily: "system-ui, -apple-system, sans-serif",
    color: "#ccc",
    background: "#1e1e1e",
    fontSize: "0.8125rem",
  },
  sidebar: {
    width: 220,
    minWidth: 220,
    borderRight: "1px solid #333",
    display: "flex",
    flexDirection: "column" as const,
    overflow: "hidden",
    flexShrink: 0,
  },
  sidebarHeader: {
    padding: "0.75rem",
    borderBottom: "1px solid #333",
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
  },
  sidebarTitle: {
    fontWeight: 600,
    fontSize: "0.875rem",
    color: "#e5e5e5",
  },
  sidebarList: {
    flex: 1,
    overflow: "auto",
    padding: "0.25rem 0",
  },
  sidebarItem: {
    padding: "0.5rem 0.75rem",
    cursor: "pointer",
    fontSize: "0.8125rem",
    color: "#ccc",
    borderLeft: "2px solid transparent",
  },
  sidebarItemActive: {
    padding: "0.5rem 0.75rem",
    cursor: "pointer",
    fontSize: "0.8125rem",
    color: "#e5e5e5",
    background: "#2a2d35",
    borderLeft: "2px solid #0e639c",
  },
  main: {
    flex: 1,
    display: "flex",
    flexDirection: "column" as const,
    overflow: "hidden",
  },
  topBar: {
    padding: "0.75rem",
    borderBottom: "1px solid #333",
    display: "flex",
    flexDirection: "column" as const,
    gap: "0.5rem",
  },
  topBarRow: {
    display: "flex",
    gap: "0.5rem",
    alignItems: "center",
    flexWrap: "wrap" as const,
  },
  designSurface: {
    flex: 1,
    overflow: "auto",
    padding: "0.75rem",
  },
  card: {
    background: "#252526",
    border: "1px solid #333",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    marginBottom: "0.5rem",
  },
  sectionTitle: {
    fontSize: "0.6875rem",
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#888",
    marginBottom: "0.375rem",
    marginTop: "0.75rem",
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
  btnDanger: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#6c1e1e",
    border: "1px solid #933",
    borderRadius: 3,
    color: "#faa",
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
  select: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.25rem 0.375rem",
    color: "#e5e5e5",
    fontSize: "0.75rem",
    outline: "none",
  },
  row: {
    display: "flex",
    gap: 6,
    alignItems: "center",
    marginBottom: 4,
  },
  inlineField: {
    display: "flex",
    flexDirection: "column" as const,
    gap: "0.125rem",
  },
  label: {
    fontSize: "0.6875rem",
    color: "#999",
    minWidth: 70,
  },
  labelSmall: {
    fontSize: "0.625rem",
    color: "#888",
  },
  badge: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#1a4731",
    color: "#22c55e",
    marginLeft: "0.25rem",
  },
  badgePortal: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#4a1d96",
    color: "#a78bfa",
    marginLeft: "0.25rem",
  },
  meta: {
    fontSize: "0.6875rem",
    color: "#666",
  },
  partHeader: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "0.375rem 0.5rem",
    background: "#2a2d35",
    borderRadius: "0.25rem",
    marginBottom: "0.25rem",
  },
  partKind: {
    fontSize: "0.8125rem",
    fontWeight: 600,
    color: "#e5e5e5",
  },
  bottomBar: {
    padding: "0.75rem",
    borderTop: "1px solid #333",
    display: "flex",
    gap: "0.5rem",
    justifyContent: "flex-end",
  },
  emptyState: {
    color: "#555",
    fontStyle: "italic" as const,
    textAlign: "center" as const,
    padding: "3rem 1rem",
    fontSize: "0.875rem",
  },
  slotCard: {
    background: "#252526",
    border: "1px solid #333",
    borderRadius: "0.375rem",
    padding: "0.5rem 0.75rem",
    marginBottom: "0.375rem",
  },
} as const;

// ── Constants ───────────────────────────────────────────────────────────────

const LAYOUTS: FacetLayout[] = ["form", "list", "table", "report", "card"];

const PART_KINDS: LayoutPartKind[] = [
  "title-header",
  "header",
  "body",
  "footer",
  "leading-summary",
  "trailing-summary",
  "leading-grand-summary",
  "trailing-grand-summary",
];

const LABEL_POSITIONS: Array<"top" | "left" | "hidden"> = ["top", "left", "hidden"];

const SUMMARY_OPS: Array<SummaryField["operation"]> = [
  "count",
  "sum",
  "average",
  "min",
  "max",
  "list",
];

type SortDirection = "asc" | "desc";

// ── Helpers ─────────────────────────────────────────────────────────────────

function omitProp<T>(obj: T, key: keyof T): Partial<T> {
  return Object.fromEntries(
    Object.entries(obj as Record<string, unknown>).filter(([k]) => k !== key),
  ) as Partial<T>;
}

let idCounter = 0;
function nextId(prefix: string) {
  return `${prefix}_${++idCounter}`;
}

function cloneSlot(s: FacetSlot): FacetSlot {
  switch (s.kind) {
    case "field": {
      const cloned = { ...s.slot };
      if (s.slot.conditionalFormats) {
        cloned.conditionalFormats = s.slot.conditionalFormats.map((cf) => ({ ...cf }));
      }
      return { kind: "field", slot: cloned };
    }
    case "portal":
      return { kind: "portal", slot: { ...s.slot, displayFields: [...s.slot.displayFields] } };
    case "text":
      return { kind: "text", slot: { ...s.slot } };
    case "drawing":
      return { kind: "drawing", slot: { ...s.slot } };
  }
}

function cloneDef(def: FacetDefinition): FacetDefinition {
  return {
    ...def,
    parts: def.parts.map((p) => ({ ...p })),
    slots: def.slots.map(cloneSlot),
    ...(def.summaryFields ? { summaryFields: def.summaryFields.map((sf) => ({ ...sf })) } : {}),
    ...(def.sortFields ? { sortFields: def.sortFields.map((sf) => ({ ...sf })) } : {}),
  };
}

function countSlotsByPart(
  slots: ReadonlyArray<FacetSlot>,
  partKind: LayoutPartKind,
): { fields: number; portals: number; texts: number; drawings: number } {
  let fields = 0;
  let portals = 0;
  let texts = 0;
  let drawings = 0;
  for (const s of slots) {
    if (s.slot.part === partKind) {
      switch (s.kind) {
        case "field": fields++; break;
        case "portal": portals++; break;
        case "text": texts++; break;
        case "drawing": drawings++; break;
      }
    }
  }
  return { fields, portals, texts, drawings };
}

// ── Part Card ───────────────────────────────────────────────────────────────

function PartCard({
  part,
  index,
  slotCounts,
  onRemove,
}: {
  part: LayoutPart;
  index: number;
  slotCounts: { fields: number; portals: number; texts: number; drawings: number };
  onRemove: () => void;
}) {
  return (
    <div style={styles.card} data-testid={`part-card-${index}`}>
      <div style={styles.partHeader}>
        <span style={styles.partKind}>{part.kind}</span>
        <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
          {part.height !== undefined && (
            <span style={styles.meta}>{part.height}px</span>
          )}
          <span style={styles.badge}>{slotCounts.fields}F</span>
          <span style={styles.badgePortal}>{slotCounts.portals}P</span>
          {slotCounts.texts > 0 && <span style={styles.badge}>{slotCounts.texts}T</span>}
          {slotCounts.drawings > 0 && <span style={styles.badge}>{slotCounts.drawings}D</span>}
          <button
            style={styles.btnDanger}
            onClick={onRemove}
            data-testid={`remove-part-${index}`}
          >
            x
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Field Slot Editor ───────────────────────────────────────────────────────

function FieldSlotEditor({
  slot,
  index,
  onChange,
  onRemove,
}: {
  slot: FieldSlot;
  index: number;
  onChange: (updated: FieldSlot) => void;
  onRemove: () => void;
}) {
  return (
    <div style={styles.slotCard} data-testid={`field-slot-${index}`}>
      <div style={{ display: "flex", gap: 6, alignItems: "flex-end", flexWrap: "wrap" as const }}>
        <div style={styles.inlineField}>
          <span style={styles.labelSmall}>Field Path</span>
          <input
            style={{ ...styles.input, width: 110 }}
            value={slot.fieldPath}
            onChange={(e) => onChange({ ...slot, fieldPath: e.target.value })}
            placeholder="e.g. name"
            data-testid={`field-${index}-path`}
          />
        </div>
        <div style={styles.inlineField}>
          <span style={styles.labelSmall}>Label</span>
          <input
            style={{ ...styles.input, width: 90 }}
            value={slot.label ?? ""}
            onChange={(e) => {
              const val = e.target.value;
              onChange(val ? { ...slot, label: val } : { ...slot, label: slot.fieldPath });
            }}
            placeholder="override"
            data-testid={`field-${index}-label`}
          />
        </div>
        <div style={styles.inlineField}>
          <span style={styles.labelSmall}>Label Pos</span>
          <select
            style={styles.select}
            value={slot.labelPosition ?? "top"}
            onChange={(e) =>
              onChange({ ...slot, labelPosition: e.target.value as "top" | "left" | "hidden" })
            }
            data-testid={`field-${index}-labelpos`}
          >
            {LABEL_POSITIONS.map((lp) => (
              <option key={lp} value={lp}>
                {lp}
              </option>
            ))}
          </select>
        </div>
        <div style={styles.inlineField}>
          <span style={styles.labelSmall}>Part</span>
          <select
            style={styles.select}
            value={slot.part}
            onChange={(e) => onChange({ ...slot, part: e.target.value as LayoutPartKind })}
            data-testid={`field-${index}-part`}
          >
            {PART_KINDS.map((pk) => (
              <option key={pk} value={pk}>
                {pk}
              </option>
            ))}
          </select>
        </div>
        <div style={styles.inlineField}>
          <span style={styles.labelSmall}>Width</span>
          <input
            style={{ ...styles.input, width: 55 }}
            value={slot.width ?? ""}
            onChange={(e) => {
              const val = e.target.value;
              if (!val) {
                onChange(omitProp(slot, "width") as FieldSlot);
              } else if (val.includes("%")) {
                onChange({ ...slot, width: val });
              } else {
                const num = Number(val);
                if (Number.isFinite(num)) {
                  onChange({ ...slot, width: num });
                } else {
                  onChange(omitProp(slot, "width") as FieldSlot);
                }
              }
            }}
            placeholder="px/%"
            data-testid={`field-${index}-width`}
          />
        </div>
        <div style={styles.inlineField}>
          <span style={styles.labelSmall}>Order</span>
          <input
            style={{ ...styles.input, width: 36 }}
            type="number"
            value={slot.order}
            onChange={(e) => onChange({ ...slot, order: Number(e.target.value) || 0 })}
            data-testid={`field-${index}-order`}
          />
        </div>
        <div style={{ ...styles.inlineField, alignItems: "center" }}>
          <span style={styles.labelSmall}>RO</span>
          <input
            type="checkbox"
            checked={slot.readOnly ?? false}
            onChange={(e) => {
              if (e.target.checked) {
                onChange({ ...slot, readOnly: true });
              } else {
                onChange(omitProp(slot, "readOnly") as FieldSlot);
              }
            }}
            data-testid={`field-${index}-readonly`}
          />
        </div>
        <button
          style={styles.btnDanger}
          onClick={onRemove}
          data-testid={`remove-field-${index}`}
        >
          x
        </button>
      </div>
    </div>
  );
}

// ── Portal Slot Editor ──────────────────────────────────────────────────────

function PortalSlotEditor({
  slot,
  index,
  onChange,
  onRemove,
}: {
  slot: PortalSlot;
  index: number;
  onChange: (updated: PortalSlot) => void;
  onRemove: () => void;
}) {
  return (
    <div style={styles.slotCard} data-testid={`portal-slot-${index}`}>
      <div style={{ display: "flex", gap: 6, alignItems: "flex-end", flexWrap: "wrap" as const }}>
        <div style={styles.inlineField}>
          <span style={styles.labelSmall}>Relationship ID</span>
          <input
            style={{ ...styles.input, width: 130 }}
            value={slot.relationshipId}
            onChange={(e) => onChange({ ...slot, relationshipId: e.target.value })}
            placeholder="e.g. invoiced-to"
            data-testid={`portal-${index}-rel`}
          />
        </div>
        <div style={styles.inlineField}>
          <span style={styles.labelSmall}>Display Fields</span>
          <input
            style={{ ...styles.input, width: 150 }}
            value={slot.displayFields.join(", ")}
            onChange={(e) =>
              onChange({
                ...slot,
                displayFields: e.target.value
                  .split(",")
                  .map((s) => s.trim())
                  .filter(Boolean),
              })
            }
            placeholder="field1, field2"
            data-testid={`portal-${index}-fields`}
          />
        </div>
        <div style={styles.inlineField}>
          <span style={styles.labelSmall}>Part</span>
          <select
            style={styles.select}
            value={slot.part}
            onChange={(e) => onChange({ ...slot, part: e.target.value as LayoutPartKind })}
            data-testid={`portal-${index}-part`}
          >
            {PART_KINDS.map((pk) => (
              <option key={pk} value={pk}>
                {pk}
              </option>
            ))}
          </select>
        </div>
        <div style={styles.inlineField}>
          <span style={styles.labelSmall}>Rows</span>
          <input
            style={{ ...styles.input, width: 36 }}
            type="number"
            value={slot.rows ?? 5}
            onChange={(e) => onChange({ ...slot, rows: Number(e.target.value) || 5 })}
            data-testid={`portal-${index}-rows`}
          />
        </div>
        <div style={styles.inlineField}>
          <span style={styles.labelSmall}>Order</span>
          <input
            style={{ ...styles.input, width: 36 }}
            type="number"
            value={slot.order}
            onChange={(e) => onChange({ ...slot, order: Number(e.target.value) || 0 })}
            data-testid={`portal-${index}-order`}
          />
        </div>
        <button
          style={styles.btnDanger}
          onClick={onRemove}
          data-testid={`remove-portal-${index}`}
        >
          x
        </button>
      </div>
    </div>
  );
}

// ── Summary Field Editor ────────────────────────────────────────────────────

function SummaryFieldEditor({
  sf,
  index,
  onChange,
  onRemove,
}: {
  sf: SummaryField;
  index: number;
  onChange: (updated: SummaryField) => void;
  onRemove: () => void;
}) {
  return (
    <div
      style={{ display: "flex", gap: 6, alignItems: "flex-end", marginBottom: 4 }}
      data-testid={`summary-row-${index}`}
    >
      <div style={styles.inlineField}>
        <span style={styles.labelSmall}>Field Path</span>
        <input
          style={{ ...styles.input, width: 100 }}
          value={sf.fieldPath}
          onChange={(e) => onChange({ ...sf, fieldPath: e.target.value })}
          placeholder="e.g. amount"
          data-testid={`summary-${index}-path`}
        />
      </div>
      <div style={styles.inlineField}>
        <span style={styles.labelSmall}>Operation</span>
        <select
          style={styles.select}
          value={sf.operation}
          onChange={(e) =>
            onChange({ ...sf, operation: e.target.value as SummaryField["operation"] })
          }
          data-testid={`summary-${index}-op`}
        >
          {SUMMARY_OPS.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      </div>
      <div style={styles.inlineField}>
        <span style={styles.labelSmall}>Label</span>
        <input
          style={{ ...styles.input, width: 90 }}
          value={sf.label ?? ""}
          onChange={(e) => {
            const val = e.target.value;
            onChange(val ? { ...sf, label: val } : { ...sf, label: sf.fieldPath });
          }}
          placeholder="optional"
          data-testid={`summary-${index}-label`}
        />
      </div>
      <button
        style={styles.btnDanger}
        onClick={onRemove}
        data-testid={`remove-summary-${index}`}
      >
        x
      </button>
    </div>
  );
}

// ── Sort Field Editor ───────────────────────────────────────────────────────

function SortFieldEditor({
  sf,
  index,
  onChange,
  onRemove,
}: {
  sf: { field: string; direction: SortDirection };
  index: number;
  onChange: (updated: { field: string; direction: SortDirection }) => void;
  onRemove: () => void;
}) {
  return (
    <div
      style={{ display: "flex", gap: 6, alignItems: "flex-end", marginBottom: 4 }}
      data-testid={`sort-row-${index}`}
    >
      <div style={styles.inlineField}>
        <span style={styles.labelSmall}>Field</span>
        <input
          style={{ ...styles.input, width: 120 }}
          value={sf.field}
          onChange={(e) => onChange({ ...sf, field: e.target.value })}
          placeholder="e.g. name"
          data-testid={`sort-${index}-field`}
        />
      </div>
      <div style={styles.inlineField}>
        <span style={styles.labelSmall}>Direction</span>
        <select
          style={styles.select}
          value={sf.direction}
          onChange={(e) => onChange({ ...sf, direction: e.target.value as SortDirection })}
          data-testid={`sort-${index}-dir`}
        >
          <option value="asc">asc</option>
          <option value="desc">desc</option>
        </select>
      </div>
      <button
        style={styles.btnDanger}
        onClick={onRemove}
        data-testid={`remove-sort-${index}`}
      >
        x
      </button>
    </div>
  );
}

// ── Main Panel ──────────────────────────────────────────────────────────────

export function FacetDesignerPanel() {
  const { definitions, register, remove, buildDefinition } = useFacetDefinitions();

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<FacetDefinition | null>(null);

  const selected = draft ?? definitions.find((d) => d.id === selectedId) ?? null;

  // ── Actions ─────────────────────────────────────────────────────────────

  const createNew = useCallback(() => {
    const id = nextId("facet");
    const def = buildDefinition(id, "page", "form")
      .name("New Facet")
      .addPart({ kind: "body" })
      .build();
    setDraft(def);
    setSelectedId(id);
  }, [buildDefinition]);

  const selectExisting = useCallback(
    (id: string) => {
      setSelectedId(id);
      const def = definitions.find((d) => d.id === id);
      setDraft(def ? cloneDef(def) : null);
    },
    [definitions],
  );

  const updateDraft = useCallback((patch: Partial<FacetDefinition>) => {
    setDraft((prev) => (prev ? { ...prev, ...patch } : null));
  }, []);

  const saveDraft = useCallback(() => {
    if (!draft) return;
    register(draft);
    setSelectedId(draft.id);
  }, [draft, register]);

  const deleteFacet = useCallback(() => {
    if (!selectedId) return;
    remove(selectedId);
    setSelectedId(null);
    setDraft(null);
  }, [selectedId, remove]);

  // ── Part mutations ──────────────────────────────────────────────────────

  const addPart = useCallback(
    (kind: LayoutPartKind) => {
      if (!draft) return;
      updateDraft({ parts: [...draft.parts, { kind, visible: true }] });
    },
    [draft, updateDraft],
  );

  const removePart = useCallback(
    (idx: number) => {
      if (!draft) return;
      updateDraft({ parts: draft.parts.filter((_, i) => i !== idx) });
    },
    [draft, updateDraft],
  );

  // ── Slot mutations ──────────────────────────────────────────────────────

  const addField = useCallback(() => {
    if (!draft) return;
    const defaultPart: LayoutPartKind =
      draft.parts.length > 0 ? (draft.parts[0]?.kind ?? "body") : "body";
    const slot: FieldSlot = {
      fieldPath: "",
      part: defaultPart,
      order: draft.slots.filter((s) => s.kind === "field").length,
    };
    updateDraft({ slots: [...draft.slots, { kind: "field" as const, slot }] });
  }, [draft, updateDraft]);

  const addPortal = useCallback(() => {
    if (!draft) return;
    const defaultPart: LayoutPartKind =
      draft.parts.length > 0 ? (draft.parts[0]?.kind ?? "body") : "body";
    const slot: PortalSlot = {
      relationshipId: "",
      displayFields: [],
      part: defaultPart,
      order: draft.slots.filter((s) => s.kind === "portal").length,
      rows: 5,
    };
    updateDraft({ slots: [...draft.slots, { kind: "portal" as const, slot }] });
  }, [draft, updateDraft]);

  const updateSlot = useCallback(
    (idx: number, updated: FacetSlot) => {
      if (!draft) return;
      const slots = [...draft.slots];
      slots[idx] = updated;
      updateDraft({ slots });
    },
    [draft, updateDraft],
  );

  const removeSlot = useCallback(
    (idx: number) => {
      if (!draft) return;
      updateDraft({ slots: draft.slots.filter((_, i) => i !== idx) });
    },
    [draft, updateDraft],
  );

  // ── Summary mutations ───────────────────────────────────────────────────

  const addSummary = useCallback(() => {
    if (!draft) return;
    const sf: SummaryField = { fieldPath: "", operation: "count" };
    updateDraft({ summaryFields: [...(draft.summaryFields ?? []), sf] });
  }, [draft, updateDraft]);

  const updateSummary = useCallback(
    (idx: number, updated: SummaryField) => {
      if (!draft) return;
      const arr = [...(draft.summaryFields ?? [])];
      arr[idx] = updated;
      updateDraft({ summaryFields: arr });
    },
    [draft, updateDraft],
  );

  const removeSummary = useCallback(
    (idx: number) => {
      if (!draft) return;
      updateDraft({
        summaryFields: (draft.summaryFields ?? []).filter((_, i) => i !== idx),
      });
    },
    [draft, updateDraft],
  );

  // ── Sort mutations ──────────────────────────────────────────────────────

  const addSortField = useCallback(() => {
    if (!draft) return;
    updateDraft({
      sortFields: [
        ...(draft.sortFields ?? []),
        { field: "", direction: "asc" as const },
      ],
    });
  }, [draft, updateDraft]);

  const updateSortField = useCallback(
    (idx: number, updated: { field: string; direction: SortDirection }) => {
      if (!draft) return;
      const arr = [...(draft.sortFields ?? [])];
      arr[idx] = updated;
      updateDraft({ sortFields: arr });
    },
    [draft, updateDraft],
  );

  const removeSortField = useCallback(
    (idx: number) => {
      if (!draft) return;
      updateDraft({
        sortFields: (draft.sortFields ?? []).filter((_, i) => i !== idx),
      });
    },
    [draft, updateDraft],
  );

  // ── Derived data ────────────────────────────────────────────────────────

  const availablePartKinds = useMemo(() => {
    if (!selected) return PART_KINDS;
    const used = new Set(selected.parts.map((p) => p.kind));
    return PART_KINDS.filter((pk) => !used.has(pk));
  }, [selected]);

  const fieldSlots = useMemo(() => {
    if (!selected) return [];
    return selected.slots
      .map((s, i) => ({ slot: s, originalIndex: i }))
      .filter(
        (entry): entry is { slot: { kind: "field"; slot: FieldSlot }; originalIndex: number } =>
          entry.slot.kind === "field",
      );
  }, [selected]);

  const portalSlots = useMemo(() => {
    if (!selected) return [];
    return selected.slots
      .map((s, i) => ({ slot: s, originalIndex: i }))
      .filter(
        (entry): entry is { slot: { kind: "portal"; slot: PortalSlot }; originalIndex: number } =>
          entry.slot.kind === "portal",
      );
  }, [selected]);

  // ── Render ──────────────────────────────────────────────────────────────

  return (
    <div style={styles.container} data-testid="facet-designer-panel">
      {/* ── Left Sidebar ── */}
      <div style={styles.sidebar}>
        <div style={styles.sidebarHeader}>
          <span style={styles.sidebarTitle}>Facets</span>
          <button
            style={styles.btnPrimary}
            onClick={createNew}
            data-testid="new-facet-btn"
          >
            + New
          </button>
        </div>
        <div style={styles.sidebarList}>
          {definitions.length === 0 && !draft && (
            <div
              style={{
                color: "#555",
                fontStyle: "italic",
                fontSize: "0.75rem",
                padding: "0.5rem 0.75rem",
              }}
            >
              No facets defined
            </div>
          )}
          {definitions.map((d) => (
            <div
              key={d.id}
              style={selectedId === d.id ? styles.sidebarItemActive : styles.sidebarItem}
              onClick={() => selectExisting(d.id)}
              data-testid={`facet-item-${d.id}`}
            >
              <div style={{ fontWeight: 500 }}>{d.name}</div>
              <div style={styles.meta}>
                {d.layout} / {d.objectType || "untyped"}
              </div>
            </div>
          ))}
          {/* Unsaved draft indicator */}
          {draft && !definitions.some((d) => d.id === draft.id) && (
            <div style={styles.sidebarItemActive} data-testid="facet-item-draft">
              <div style={{ fontWeight: 500 }}>{draft.name}</div>
              <div style={styles.meta}>
                {draft.layout} / {draft.objectType || "untyped"}
                <span
                  style={{
                    ...styles.badge,
                    background: "#7c3aed",
                    color: "#c4b5fd",
                  }}
                >
                  draft
                </span>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* ── Main Design Area ── */}
      <div style={styles.main}>
        {!selected ? (
          <div style={styles.emptyState} data-testid="empty-state">
            Select a facet from the sidebar or create a new one.
          </div>
        ) : (
          <>
            {/* ── Top Bar: metadata ── */}
            <div style={styles.topBar}>
              <div style={styles.topBarRow}>
                <div style={styles.inlineField}>
                  <span style={styles.labelSmall}>Name</span>
                  <input
                    style={{ ...styles.input, width: 160 }}
                    value={selected.name}
                    onChange={(e) => updateDraft({ name: e.target.value })}
                    data-testid="facet-name-input"
                  />
                </div>
                <div style={styles.inlineField}>
                  <span style={styles.labelSmall}>Layout</span>
                  <select
                    style={styles.select}
                    value={selected.layout}
                    onChange={(e) => updateDraft({ layout: e.target.value as FacetLayout })}
                    data-testid="facet-layout-select"
                  >
                    {LAYOUTS.map((l) => (
                      <option key={l} value={l}>
                        {l}
                      </option>
                    ))}
                  </select>
                </div>
                <div style={styles.inlineField}>
                  <span style={styles.labelSmall}>Object Type</span>
                  <input
                    style={{ ...styles.input, width: 120 }}
                    value={selected.objectType}
                    onChange={(e) => updateDraft({ objectType: e.target.value })}
                    placeholder="e.g. contact"
                    data-testid="facet-objtype-input"
                  />
                </div>
              </div>
              <div style={styles.topBarRow}>
                <div style={{ ...styles.inlineField, flex: 1 }}>
                  <span style={styles.labelSmall}>Description</span>
                  <input
                    style={{ ...styles.input, width: "100%" }}
                    value={selected.description ?? ""}
                    onChange={(e) =>
                      updateDraft(e.target.value ? { description: e.target.value } : { description: "" })
                    }
                    placeholder="Optional description"
                    data-testid="facet-desc-input"
                  />
                </div>
              </div>
            </div>

            {/* ── Design Surface ── */}
            <div style={styles.designSurface}>
              {/* ── Layout Parts ── */}
              <div style={{ ...styles.sectionTitle, marginTop: 0 }}>
                Layout Parts ({selected.parts.length})
              </div>
              {selected.parts.map((part, idx) => (
                <PartCard
                  key={`${part.kind}-${idx}`}
                  part={part}
                  index={idx}
                  slotCounts={countSlotsByPart(selected.slots, part.kind)}
                  onRemove={() => removePart(idx)}
                />
              ))}
              {availablePartKinds.length > 0 && (
                <div
                  style={{
                    display: "flex",
                    gap: 4,
                    flexWrap: "wrap",
                    marginBottom: "0.75rem",
                  }}
                >
                  {availablePartKinds.map((pk) => (
                    <button
                      key={pk}
                      style={styles.btn}
                      onClick={() => addPart(pk)}
                      data-testid={`add-part-${pk}`}
                    >
                      + {pk}
                    </button>
                  ))}
                </div>
              )}

              {/* ── Field Slots ── */}
              <div style={styles.sectionTitle}>
                Field Slots ({fieldSlots.length})
                <button
                  style={{ ...styles.btn, marginLeft: 8 }}
                  onClick={addField}
                  data-testid="add-field-btn"
                >
                  + Field
                </button>
              </div>
              {fieldSlots.map(({ slot: facetSlot, originalIndex }, displayIdx) => (
                <FieldSlotEditor
                  key={originalIndex}
                  slot={facetSlot.slot}
                  index={displayIdx}
                  onChange={(updated) =>
                    updateSlot(originalIndex, { kind: "field", slot: updated })
                  }
                  onRemove={() => removeSlot(originalIndex)}
                />
              ))}

              {/* ── Portal Slots ── */}
              <div style={styles.sectionTitle}>
                Portal Slots ({portalSlots.length})
                <button
                  style={{ ...styles.btn, marginLeft: 8 }}
                  onClick={addPortal}
                  data-testid="add-portal-btn"
                >
                  + Portal
                </button>
              </div>
              {portalSlots.map(({ slot: facetSlot, originalIndex }, displayIdx) => (
                <PortalSlotEditor
                  key={originalIndex}
                  slot={facetSlot.slot}
                  index={displayIdx}
                  onChange={(updated) =>
                    updateSlot(originalIndex, { kind: "portal", slot: updated })
                  }
                  onRemove={() => removeSlot(originalIndex)}
                />
              ))}

              {/* ── Summary Fields ── */}
              <div style={styles.sectionTitle}>
                Summary Fields ({(selected.summaryFields ?? []).length})
                <button
                  style={{ ...styles.btn, marginLeft: 8 }}
                  onClick={addSummary}
                  data-testid="add-summary-btn"
                >
                  + Summary
                </button>
              </div>
              {(selected.summaryFields ?? []).map((sf, i) => (
                <SummaryFieldEditor
                  key={i}
                  sf={sf}
                  index={i}
                  onChange={(updated) => updateSummary(i, updated)}
                  onRemove={() => removeSummary(i)}
                />
              ))}

              {/* ── Sort Rules ── */}
              <div style={styles.sectionTitle}>
                Sort Rules ({(selected.sortFields ?? []).length})
                <button
                  style={{ ...styles.btn, marginLeft: 8 }}
                  onClick={addSortField}
                  data-testid="add-sort-btn"
                >
                  + Sort
                </button>
              </div>
              {(selected.sortFields ?? []).map((sf, i) => (
                <SortFieldEditor
                  key={i}
                  sf={sf}
                  index={i}
                  onChange={(updated) => updateSortField(i, updated)}
                  onRemove={() => removeSortField(i)}
                />
              ))}

              {/* ── Group By ── */}
              <div style={styles.sectionTitle}>Group By</div>
              <input
                style={{ ...styles.input, width: 200 }}
                value={selected.groupByField ?? ""}
                onChange={(e) =>
                  updateDraft(e.target.value ? { groupByField: e.target.value } : { groupByField: "" })
                }
                placeholder="e.g. type, status"
                data-testid="facet-groupby-input"
              />

              {/* ── Automation Hooks ── */}
              <div style={styles.sectionTitle}>Automation Hooks</div>
              <div style={styles.card}>
                <div style={styles.row}>
                  <span style={styles.label}>onRecordLoad</span>
                  <input
                    style={{ ...styles.input, flex: 1 }}
                    value={selected.onRecordLoad ?? ""}
                    onChange={(e) =>
                      updateDraft(e.target.value ? { onRecordLoad: e.target.value } : { onRecordLoad: "" })
                    }
                    placeholder="automation ID"
                    data-testid="hook-load-input"
                  />
                </div>
                <div style={styles.row}>
                  <span style={styles.label}>onRecordCommit</span>
                  <input
                    style={{ ...styles.input, flex: 1 }}
                    value={selected.onRecordCommit ?? ""}
                    onChange={(e) =>
                      updateDraft(e.target.value ? { onRecordCommit: e.target.value } : { onRecordCommit: "" })
                    }
                    placeholder="automation ID"
                    data-testid="hook-commit-input"
                  />
                </div>
              </div>
            </div>

            {/* ── Bottom Bar ── */}
            <div style={styles.bottomBar}>
              <button
                style={styles.btnDanger}
                onClick={deleteFacet}
                data-testid="delete-facet-btn"
              >
                Delete
              </button>
              <button
                style={styles.btnPrimary}
                onClick={saveDraft}
                data-testid="save-facet-btn"
              >
                Save Facet
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

export default FacetDesignerPanel;
