/**
 * Entity Builder panel (Tier 9A).
 *
 * Lets an author define their own custom entity type at runtime:
 * pick a name, category, icon, color, and a list of typed fields.
 * Clicking "Register" hands the assembled `EntityDef` to the kernel
 * registry so the new type immediately appears in the component
 * palette and can be created like any built-in type.
 *
 * The definitions live only in-memory for now — persisting custom
 * entities to the manifest is a separate, follow-on task.
 */

import { useCallback, useState } from "react";
import type { EntityDef, EntityFieldDef, EntityFieldType } from "@prism/core/object-model";
import type { LensPuckConfig } from "@prism/core/puck";
import { useKernel, useRegistration } from "../kernel/index.js";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
interface FieldDraft {
  id: string;
  type: EntityFieldType;
  label: string;
  required: boolean;
}

const FIELD_TYPES: EntityFieldType[] = [
  "string",
  "text",
  "int",
  "float",
  "bool",
  "color",
  "enum",
  "url",
  "date",
  "datetime",
  "object_ref",
  "lookup",
  "rollup",
];

const CATEGORIES = ["custom", "component", "section", "section-wrapper", "container"];

export function EntityBuilderPanel() {
  const kernel = useKernel();

  const [typeName, setTypeName] = useState("");
  const [label, setLabel] = useState("");
  const [category, setCategory] = useState("custom");
  const [icon, setIcon] = useState("\u{1F9F1}");
  const [color, setColor] = useState("#a78bfa");
  const [fields, setFields] = useState<FieldDraft[]>([]);

  const [newFieldId, setNewFieldId] = useState("");
  const [newFieldType, setNewFieldType] = useState<EntityFieldType>("string");
  const [newFieldLabel, setNewFieldLabel] = useState("");
  const [newFieldRequired, setNewFieldRequired] = useState(false);

  const addField = useCallback(() => {
    if (!newFieldId.trim()) return;
    setFields((prev) => [
      ...prev,
      {
        id: newFieldId.trim(),
        type: newFieldType,
        label: newFieldLabel.trim() || newFieldId.trim(),
        required: newFieldRequired,
      },
    ]);
    setNewFieldId("");
    setNewFieldLabel("");
    setNewFieldRequired(false);
  }, [newFieldId, newFieldType, newFieldLabel, newFieldRequired]);

  const removeField = useCallback((id: string) => {
    setFields((prev) => prev.filter((f) => f.id !== id));
  }, []);

  const registerEntity = useRegistration<EntityDef<string, LensPuckConfig>>({
    noun: "entity type",
    name: (def) => `${def.label} (${def.type})`,
    validate: (def) =>
      def.type.trim() && def.label.trim() ? null : "Type name and label required",
    exists: (def) => !!kernel.registry.get(def.type),
    register: (def) => kernel.registry.register(def),
    onSuccess: () => {
      setTypeName("");
      setLabel("");
      setFields([]);
    },
  });

  const register = useCallback(() => {
    const defFields: EntityFieldDef[] = fields.map((f) => ({
      id: f.id,
      type: f.type,
      label: f.label,
      required: f.required,
    }));
    const def: EntityDef<string, LensPuckConfig> = {
      type: typeName.trim(),
      category,
      label: label.trim(),
      pluralLabel: `${label.trim()}s`,
      icon,
      color,
      fields: defFields,
    };
    registerEntity(def);
  }, [typeName, label, category, icon, color, fields, registerEntity]);

  const allDefs = kernel.registry.allDefs();
  const customDefs = allDefs.filter((d) => d.category === "custom");

  return (
    <div
      data-testid="entity-builder-panel"
      style={{
        height: "100%",
        overflow: "auto",
        padding: 16,
        background: "#1e1e1e",
        color: "#ccc",
        fontSize: 12,
      }}
    >
      <h2 style={{ fontSize: 14, margin: 0, marginBottom: 12, color: "#e5e5e5" }}>
        Entity Builder
      </h2>
      <div style={{ color: "#888", fontSize: 11, marginBottom: 16 }}>
        Define a new entity type. Once registered it appears in the component palette
        and can be instantiated like any built-in block.
      </div>

      <div style={{ marginBottom: 16 }}>
        <Field label="Type ID (machine name)">
          <input
            value={typeName}
            onChange={(e) => setTypeName(e.target.value)}
            placeholder="e.g. contact, invoice, recipe"
            data-testid="entity-type-name"
            style={inputStyle}
          />
        </Field>
        <Field label="Display Label">
          <input
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="e.g. Contact"
            data-testid="entity-label"
            style={inputStyle}
          />
        </Field>
        <Field label="Category">
          <select
            value={category}
            onChange={(e) => setCategory(e.target.value)}
            data-testid="entity-category"
            style={inputStyle}
          >
            {CATEGORIES.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Icon">
          <input
            value={icon}
            onChange={(e) => setIcon(e.target.value)}
            data-testid="entity-icon"
            style={inputStyle}
          />
        </Field>
        <Field label="Color">
          <input
            type="color"
            value={color}
            onChange={(e) => setColor(e.target.value)}
            data-testid="entity-color"
            style={{ ...inputStyle, width: 48, padding: 0, height: 24 }}
          />
        </Field>
      </div>

      <div style={{ marginBottom: 16 }}>
        <div style={sectionTitleStyle}>Fields</div>
        {fields.length === 0 && (
          <div style={{ color: "#555", fontSize: 11, fontStyle: "italic" }}>
            No fields yet.
          </div>
        )}
        {fields.map((f) => (
          <div
            key={f.id}
            data-testid={`field-row-${f.id}`}
            style={{
              display: "flex",
              gap: 6,
              alignItems: "center",
              padding: "4px 6px",
              marginBottom: 2,
              background: "#252526",
              border: "1px solid #333",
              borderRadius: 3,
            }}
          >
            <span style={{ flex: 1 }}>
              <strong>{f.label}</strong>{" "}
              <span style={{ color: "#666", fontSize: 10 }}>
                {f.id} · {f.type}
                {f.required ? " · required" : ""}
              </span>
            </span>
            <button onClick={() => removeField(f.id)} style={{ ...iconBtn, color: "#f87171" }}>
              ✕
            </button>
          </div>
        ))}

        <div
          style={{
            marginTop: 8,
            padding: 8,
            background: "#252526",
            border: "1px dashed #555",
            borderRadius: 3,
          }}
        >
          <div style={{ display: "flex", gap: 4, marginBottom: 4 }}>
            <input
              value={newFieldId}
              onChange={(e) => setNewFieldId(e.target.value)}
              placeholder="Field id"
              data-testid="new-field-id"
              style={{ ...inputStyle, flex: 1 }}
            />
            <select
              value={newFieldType}
              onChange={(e) => setNewFieldType(e.target.value as EntityFieldType)}
              data-testid="new-field-type"
              style={{ ...inputStyle, width: 120 }}
            >
              {FIELD_TYPES.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </div>
          <div style={{ display: "flex", gap: 4 }}>
            <input
              value={newFieldLabel}
              onChange={(e) => setNewFieldLabel(e.target.value)}
              placeholder="Label (optional)"
              style={{ ...inputStyle, flex: 1 }}
            />
            <label style={{ display: "flex", alignItems: "center", gap: 4 }}>
              <input
                type="checkbox"
                checked={newFieldRequired}
                onChange={(e) => setNewFieldRequired(e.target.checked)}
              />
              Required
            </label>
            <button onClick={addField} data-testid="add-field-btn" style={primaryBtn}>
              + Add Field
            </button>
          </div>
        </div>
      </div>

      <button
        onClick={register}
        data-testid="register-entity-btn"
        style={{ ...primaryBtn, width: "100%", padding: "6px 12px", fontSize: 12 }}
      >
        Register Entity Type
      </button>

      {customDefs.length > 0 && (
        <div style={{ marginTop: 20 }}>
          <div style={sectionTitleStyle}>Currently Registered Custom Types</div>
          {customDefs.map((d) => (
            <div key={d.type} style={{ fontSize: 11, color: "#ccc", marginBottom: 2 }}>
              {String(d.icon ?? "")} <strong>{d.label}</strong>{" "}
              <span style={{ color: "#666" }}>({d.type})</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: 6 }}>
      <div style={{ fontSize: 11, color: "#999", marginBottom: 3 }}>{label}</div>
      {children}
    </div>
  );
}

const inputStyle: React.CSSProperties = {
  padding: "4px 6px",
  fontSize: 12,
  background: "#1e1e1e",
  border: "1px solid #444",
  borderRadius: 3,
  color: "#ccc",
  outline: "none",
  boxSizing: "border-box",
  width: "100%",
};

const iconBtn: React.CSSProperties = {
  padding: "2px 6px",
  fontSize: 10,
  background: "#333",
  border: "1px solid #444",
  borderRadius: 2,
  color: "#ccc",
  cursor: "pointer",
};

const primaryBtn: React.CSSProperties = {
  padding: "4px 10px",
  fontSize: 11,
  background: "#0e639c",
  border: "1px solid #1177bb",
  borderRadius: 3,
  color: "#fff",
  cursor: "pointer",
};

const sectionTitleStyle: React.CSSProperties = {
  fontSize: 10,
  fontWeight: 600,
  textTransform: "uppercase",
  color: "#666",
  marginBottom: 6,
  letterSpacing: 0.5,
};


// ── Lens registration ──────────────────────────────────────────────────────

export const ENTITY_BUILDER_LENS_ID = lensId("entity-builder");

export const entityBuilderLensManifest: LensManifest = {

  id: ENTITY_BUILDER_LENS_ID,
  name: "Entity Builder",
  icon: "\u{1F9F1}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [
      { id: "switch-entity-builder", name: "Switch to Entity Builder", shortcut: ["shift+e"], section: "Navigation" },
    ],
  },
};

export const entityBuilderLensBundle: LensBundle = defineLensBundle(
  entityBuilderLensManifest,
  EntityBuilderPanel,
);
