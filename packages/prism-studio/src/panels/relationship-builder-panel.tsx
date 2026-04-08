/**
 * Relationship Builder panel (Tier 9B).
 *
 * Visually compose custom edge types: pick a relation name, pretty label,
 * behavior class, color, and optional source/target type restrictions.
 * The resulting `EdgeTypeDef` is handed to the kernel registry so new
 * relationship types are immediately available in the graph panel and
 * any data-portal widgets.
 */

import { useCallback, useState } from "react";
import type { EdgeTypeDef, EdgeBehavior } from "@prism/core/object-model";
import { useKernel } from "../kernel/index.js";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
const BEHAVIORS: EdgeBehavior[] = [
  "weak",
  "strong",
  "dependency",
  "membership",
  "assignment",
  "stream",
];

export function RelationshipBuilderPanel() {
  const kernel = useKernel();

  const [relation, setRelation] = useState("");
  const [label, setLabel] = useState("");
  const [behavior, setBehavior] = useState<EdgeBehavior>("weak");
  const [color, setColor] = useState("#94a3b8");
  const [sourceTypes, setSourceTypes] = useState("");
  const [targetTypes, setTargetTypes] = useState("");
  const [description, setDescription] = useState("");

  const register = useCallback(() => {
    if (!relation.trim() || !label.trim()) {
      kernel.notifications.add({
        title: "Relation and label required",
        kind: "warning",
      });
      return;
    }
    if (kernel.registry.getEdgeType(relation.trim())) {
      kernel.notifications.add({
        title: `Relation "${relation}" already exists`,
        kind: "warning",
      });
      return;
    }
    const def: EdgeTypeDef = {
      relation: relation.trim(),
      label: label.trim(),
      behavior,
      color,
    };
    if (description.trim()) def.description = description.trim();
    const srcList = sourceTypes.trim()
      ? sourceTypes.split(",").map((s) => s.trim()).filter(Boolean)
      : null;
    const tgtList = targetTypes.trim()
      ? targetTypes.split(",").map((s) => s.trim()).filter(Boolean)
      : null;
    if (srcList && srcList.length > 0) def.sourceTypes = srcList;
    if (tgtList && tgtList.length > 0) def.targetTypes = tgtList;
    kernel.registry.registerEdge(def);
    kernel.notifications.add({
      title: `Registered edge type "${label}"`,
      kind: "success",
    });
    setRelation("");
    setLabel("");
    setDescription("");
    setSourceTypes("");
    setTargetTypes("");
  }, [relation, label, behavior, color, description, sourceTypes, targetTypes, kernel]);

  const allEdgeDefs = kernel.registry.allEdgeDefs();

  return (
    <div
      data-testid="relationship-builder-panel"
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
        Relationships
      </h2>
      <div style={{ color: "#888", fontSize: 11, marginBottom: 16 }}>
        Define a new edge type so objects can reference each other with custom
        semantics (depends-on, parent-of, cited-by, etc.).
      </div>

      <div style={{ marginBottom: 16 }}>
        <Field label="Relation ID">
          <input
            value={relation}
            onChange={(e) => setRelation(e.target.value)}
            placeholder="e.g. depends-on, cites, owns"
            data-testid="relation-id"
            style={inputStyle}
          />
        </Field>
        <Field label="Display Label">
          <input
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="e.g. Depends On"
            data-testid="relation-label"
            style={inputStyle}
          />
        </Field>
        <Field label="Behavior">
          <select
            value={behavior}
            onChange={(e) => setBehavior(e.target.value as EdgeBehavior)}
            data-testid="relation-behavior"
            style={inputStyle}
          >
            {BEHAVIORS.map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Color">
          <input
            type="color"
            value={color}
            onChange={(e) => setColor(e.target.value)}
            style={{ ...inputStyle, width: 48, padding: 0, height: 24 }}
          />
        </Field>
        <Field label="Allowed Source Types (comma separated; blank = any)">
          <input
            value={sourceTypes}
            onChange={(e) => setSourceTypes(e.target.value)}
            placeholder="e.g. page, section"
            data-testid="relation-source-types"
            style={inputStyle}
          />
        </Field>
        <Field label="Allowed Target Types (comma separated; blank = any)">
          <input
            value={targetTypes}
            onChange={(e) => setTargetTypes(e.target.value)}
            placeholder="e.g. contact"
            data-testid="relation-target-types"
            style={inputStyle}
          />
        </Field>
        <Field label="Description">
          <input
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            style={inputStyle}
          />
        </Field>
      </div>

      <button
        onClick={register}
        data-testid="register-relation-btn"
        style={{
          width: "100%",
          padding: "6px 12px",
          fontSize: 12,
          background: "#0e639c",
          border: "1px solid #1177bb",
          borderRadius: 3,
          color: "#fff",
          cursor: "pointer",
        }}
      >
        Register Relation
      </button>

      <div style={{ marginTop: 20 }}>
        <div
          style={{
            fontSize: 10,
            fontWeight: 600,
            textTransform: "uppercase",
            color: "#666",
            marginBottom: 6,
          }}
        >
          Currently Registered Relations
        </div>
        {allEdgeDefs.length === 0 ? (
          <div style={{ color: "#555", fontStyle: "italic" }}>None</div>
        ) : (
          allEdgeDefs.map((d) => (
            <div
              key={d.relation}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "3px 6px",
                background: "#252526",
                border: "1px solid #333",
                borderRadius: 2,
                marginBottom: 2,
              }}
            >
              <span
                style={{
                  display: "inline-block",
                  width: 10,
                  height: 10,
                  borderRadius: 2,
                  background: d.color ?? "#888",
                }}
              />
              <strong>{d.label}</strong>
              <span style={{ color: "#666", fontSize: 10 }}>{d.relation}</span>
              <span style={{ color: "#777", fontSize: 10, marginLeft: "auto" }}>
                {d.behavior ?? "weak"}
              </span>
            </div>
          ))
        )}
      </div>
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


// ── Lens registration ──────────────────────────────────────────────────────

export const RELATIONSHIP_BUILDER_LENS_ID = lensId("relationship-builder");

export const relationshipBuilderLensManifest: LensManifest = {

  id: RELATIONSHIP_BUILDER_LENS_ID,
  name: "Relationships",
  icon: "\u{1F517}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [
      { id: "switch-relationships", name: "Switch to Relationships", shortcut: ["shift+r"], section: "Navigation" },
    ],
  },
};

export const relationshipBuilderLensBundle: LensBundle = defineLensBundle(
  relationshipBuilderLensManifest,
  RelationshipBuilderPanel,
);
