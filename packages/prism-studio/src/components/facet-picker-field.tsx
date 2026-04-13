/**
 * `facetPickerField` — Puck custom field for selecting an existing
 * FacetDefinition or creating a new one inline without leaving the
 * layout builder.
 *
 * Integration points:
 *   - `kernel.listFacetDefinitions()` / `kernel.registerFacetDefinition()` —
 *     the same FacetDefinition registry used by the Facet Designer lens
 *     (Shift+X) and the facet-view / spatial-canvas renderers.
 *   - `kernel.registry.allDefs()` — object-type suggestions come from the
 *     ObjectRegistry so authors can't target a type that doesn't exist.
 *   - `kernel.notifications.add()` — visible feedback on create.
 *
 * Inline create captures only the essentials (id, name, objectType,
 * layout); the full layout authoring path lives in the Facet Designer
 * panel. This field is about *composing* pages with Puck, not designing
 * the facet itself.
 */

import { useCallback, useMemo, useState, type ReactElement } from "react";
import { FieldLabel, type Field } from "@measured/puck";
import { createFacetDefinition, type FacetLayout } from "@prism/core/facet";
import type { StudioKernel } from "../kernel/studio-kernel.js";
import { facetIdFromName, uniqueFacetId } from "./facet-picker-helpers.js";
export { facetIdFromName, uniqueFacetId } from "./facet-picker-helpers.js";

// ── Styles ──────────────────────────────────────────────────────────────────

const baseInput = {
  width: "100%",
  padding: "6px 8px",
  fontSize: 13,
  border: "1px solid #cbd5e1",
  borderRadius: 4,
  background: "#ffffff",
  color: "#0f172a",
  boxSizing: "border-box" as const,
};

const btn = {
  padding: "5px 10px",
  fontSize: 12,
  border: "1px solid #cbd5e1",
  borderRadius: 4,
  background: "#f8fafc",
  color: "#334155",
  cursor: "pointer",
};

const btnPrimary = {
  ...btn,
  background: "#6366f1",
  borderColor: "#6366f1",
  color: "#ffffff",
};

const createBox = {
  marginTop: 6,
  padding: 10,
  border: "1px solid #e2e8f0",
  borderRadius: 6,
  background: "#f8fafc",
  display: "flex",
  flexDirection: "column" as const,
  gap: 6,
};

const fieldLabel = {
  fontSize: 11,
  fontWeight: 600,
  color: "#475569",
  textTransform: "uppercase" as const,
  letterSpacing: "0.04em",
};

// ── Helpers ────────────────────────────────────────────────────────────────

const FACET_LAYOUTS: Array<{ value: FacetLayout; label: string }> = [
  { value: "form", label: "Form" },
  { value: "list", label: "List" },
  { value: "table", label: "Table" },
  { value: "report", label: "Report" },
  { value: "card", label: "Card" },
];

// ── Inner component ───────────────────────────────────────────────────────

export interface FacetPickerFieldInnerProps {
  kernel: StudioKernel;
  value: unknown;
  onChange: (next: string) => void;
  readOnly: boolean;
  label: string | undefined;
}

export function FacetPickerFieldInner(props: FacetPickerFieldInnerProps): ReactElement {
  const { kernel, value, onChange, readOnly, label } = props;
  const current = typeof value === "string" ? value : "";

  // Snapshot on every render — the layout panel re-renders often enough
  // that we don't need a reactive subscription here; the field only shows
  // while its block is selected.
  const facets = useMemo(() => kernel.listFacetDefinitions(), [kernel, current]);
  const objectTypes = useMemo(
    () =>
      kernel.registry
        .allDefs()
        .filter((d) => d.category !== "workspace" && d.category !== "section")
        .map((d) => ({ value: d.type, label: d.label ?? d.type })),
    [kernel],
  );

  const [creating, setCreating] = useState(false);
  const [draftName, setDraftName] = useState("");
  const [draftType, setDraftType] = useState<string>(
    objectTypes[0]?.value ?? "",
  );
  const [draftLayout, setDraftLayout] = useState<FacetLayout>("form");
  const [error, setError] = useState<string | null>(null);

  const createFacet = useCallback(() => {
    setError(null);
    const name = draftName.trim();
    if (!name) {
      setError("Name is required.");
      return;
    }
    if (!draftType) {
      setError("Object type is required.");
      return;
    }
    const baseId = facetIdFromName(name, draftType);
    const id = uniqueFacetId(baseId, kernel.listFacetDefinitions());
    const def = createFacetDefinition(id, draftType, draftLayout);
    def.name = name;
    try {
      kernel.registerFacetDefinition(def);
      kernel.notifications.add({
        title: `Facet created: ${name}`,
        body: `${draftType} · ${draftLayout}`,
        kind: "success",
      });
      onChange(id);
      setCreating(false);
      setDraftName("");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not create facet");
    }
  }, [draftName, draftType, draftLayout, kernel, onChange]);

  const selected = facets.find((f) => f.id === current);

  return (
    <FieldLabel label={label ?? ""} el="div" readOnly={readOnly}>
      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <div style={{ display: "flex", gap: 6 }}>
          <select
            value={current}
            disabled={readOnly}
            onChange={(e) => onChange(e.target.value)}
            style={{ ...baseInput, flex: 1 }}
            data-testid="facet-picker-select"
            aria-label={label ?? "Facet"}
          >
            <option value="">— Select a facet —</option>
            {facets.map((f) => (
              <option key={f.id} value={f.id}>
                {f.name || f.id} ({f.objectType} · {f.layout})
              </option>
            ))}
          </select>
          <button
            type="button"
            style={btn}
            disabled={readOnly}
            onClick={() => {
              setCreating((c) => !c);
              setError(null);
            }}
            data-testid="facet-picker-new"
          >
            {creating ? "Cancel" : "New…"}
          </button>
        </div>

        {selected ? (
          <div style={{ fontSize: 11, color: "#64748b" }}>
            Binds <code>{selected.objectType}</code> as{" "}
            <code>{selected.layout}</code>
            {selected.slots && selected.slots.length > 0
              ? ` · ${selected.slots.length} slot${selected.slots.length === 1 ? "" : "s"}`
              : " · no slots yet — open the Facet Designer to add fields."}
          </div>
        ) : current ? (
          <div style={{ fontSize: 11, color: "#b45309" }}>
            Facet id <code>{current}</code> not found in registry.
          </div>
        ) : null}

        {creating ? (
          <div style={createBox} data-testid="facet-picker-create-form">
            <div>
              <div style={fieldLabel}>Name</div>
              <input
                type="text"
                value={draftName}
                placeholder="Contact Form"
                onChange={(e) => setDraftName(e.target.value)}
                style={baseInput}
                data-testid="facet-picker-create-name"
                aria-label="Facet name"
              />
            </div>
            <div>
              <div style={fieldLabel}>Object type</div>
              <select
                value={draftType}
                onChange={(e) => setDraftType(e.target.value)}
                style={baseInput}
                data-testid="facet-picker-create-type"
                aria-label="Object type"
              >
                {objectTypes.length === 0 ? (
                  <option value="">— no types registered —</option>
                ) : null}
                {objectTypes.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <div style={fieldLabel}>Layout</div>
              <select
                value={draftLayout}
                onChange={(e) => setDraftLayout(e.target.value as FacetLayout)}
                style={baseInput}
                data-testid="facet-picker-create-layout"
                aria-label="Layout"
              >
                {FACET_LAYOUTS.map((l) => (
                  <option key={l.value} value={l.value}>
                    {l.label}
                  </option>
                ))}
              </select>
            </div>
            {error ? (
              <div style={{ fontSize: 11, color: "#dc2626" }} data-testid="facet-picker-create-error">
                {error}
              </div>
            ) : null}
            <div style={{ display: "flex", gap: 6, justifyContent: "flex-end" }}>
              <button
                type="button"
                style={btn}
                onClick={() => {
                  setCreating(false);
                  setDraftName("");
                  setError(null);
                }}
              >
                Cancel
              </button>
              <button
                type="button"
                style={btnPrimary}
                onClick={createFacet}
                data-testid="facet-picker-create-submit"
              >
                Create facet
              </button>
            </div>
            <div style={{ fontSize: 11, color: "#64748b" }}>
              New facets start empty. Open the Facet Designer (Shift+X) to
              add parts, fields, and automation hooks.
            </div>
          </div>
        ) : null}
      </div>
    </FieldLabel>
  );
}

// ── Field factory ──────────────────────────────────────────────────────────

export function facetPickerField(
  kernel: StudioKernel,
  opts: { label?: string } = {},
): Field<string> {
  return {
    type: "custom",
    ...(opts.label !== undefined ? { label: opts.label } : {}),
    render: ({ value, onChange, readOnly }): ReactElement => (
      <FacetPickerFieldInner
        kernel={kernel}
        value={value}
        onChange={onChange}
        readOnly={readOnly ?? false}
        label={opts.label}
      />
    ),
  };
}
