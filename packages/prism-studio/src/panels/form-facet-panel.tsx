/**
 * FormFacet Panel — schema-driven form renderer.
 *
 * Parses YAML/JSON source into editable form fields with auto-detected types.
 * Integrates FacetParser for bidirectional source ↔ form state,
 * SpellEngine for text field checking, and field inference.
 */

import { useState, useCallback, useMemo } from "react";
import { useFacetParser, useSpellCheck, useKernel } from "../kernel/index.js";
import type { FieldSchema } from "@prism/core/forms";

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
  card: {
    background: "#252526",
    border: "1px solid #333",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    marginBottom: "0.5rem",
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
    padding: "0.375rem 0.5rem",
    color: "#e5e5e5",
    fontSize: "0.875rem",
    width: "100%",
    outline: "none",
    boxSizing: "border-box" as const,
  },
  textarea: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.375rem 0.5rem",
    color: "#e5e5e5",
    fontSize: "0.75rem",
    fontFamily: "monospace",
    width: "100%",
    outline: "none",
    boxSizing: "border-box" as const,
    resize: "vertical" as const,
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
  badge: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#1a4731",
    color: "#22c55e",
    marginLeft: "0.375rem",
  },
  badgeWarn: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#4a3b00",
    color: "#f59e0b",
    marginLeft: "0.375rem",
  },
  meta: {
    fontSize: "0.6875rem",
    color: "#666",
  },
  spellError: {
    borderColor: "#f59e0b",
    boxShadow: "0 0 0 1px #f59e0b",
  },
} as const;

// ── Field Renderer ──────────────────────────────────────────────────────────

function FormField({
  field,
  value,
  onChange,
  spellErrors,
}: {
  field: FieldSchema;
  value: unknown;
  onChange: (value: unknown) => void;
  spellErrors: Array<{ word: string; suggestions: string[] }>;
}) {
  const strVal = value === null || value === undefined ? "" : String(value);

  switch (field.type) {
    case "boolean":
      return (
        <div style={styles.fieldRow}>
          <label style={{ ...styles.label, display: "flex", alignItems: "center", gap: "0.5rem" }}>
            <input
              type="checkbox"
              checked={Boolean(value)}
              onChange={(e) => onChange(e.target.checked)}
              data-testid={`field-${field.id}`}
            />
            {field.label ?? field.id}
          </label>
        </div>
      );

    case "number":
      return (
        <div style={styles.fieldRow}>
          <label style={styles.label}>{field.label ?? field.id}</label>
          <input
            type="number"
            style={styles.input}
            value={strVal}
            onChange={(e) => onChange(e.target.value ? Number(e.target.value) : "")}
            placeholder={field.placeholder}
            data-testid={`field-${field.id}`}
          />
        </div>
      );

    case "textarea":
      return (
        <div style={styles.fieldRow}>
          <label style={styles.label}>
            {field.label ?? field.id}
            {spellErrors.length > 0 && (
              <span style={styles.badgeWarn}>{spellErrors.length} spell</span>
            )}
          </label>
          <textarea
            style={{
              ...styles.textarea,
              height: 80,
              ...(spellErrors.length > 0 ? styles.spellError : {}),
            }}
            value={strVal}
            onChange={(e) => onChange(e.target.value)}
            placeholder={field.placeholder}
            data-testid={`field-${field.id}`}
          />
          {spellErrors.length > 0 && (
            <div style={styles.meta}>
              {spellErrors.map((e) => `"${e.word}" → ${e.suggestions.join(", ") || "?"}`).join("; ")}
            </div>
          )}
        </div>
      );

    case "tags":
      return (
        <div style={styles.fieldRow}>
          <label style={styles.label}>{field.label ?? field.id}</label>
          <input
            style={styles.input}
            value={Array.isArray(value) ? (value as string[]).join(", ") : strVal}
            onChange={(e) => onChange(e.target.value.split(",").map((s) => s.trim()).filter(Boolean))}
            placeholder="tag1, tag2, ..."
            data-testid={`field-${field.id}`}
          />
        </div>
      );

    default:
      return (
        <div style={styles.fieldRow}>
          <label style={styles.label}>
            {field.label ?? field.id}
            {spellErrors.length > 0 && (
              <span style={styles.badgeWarn}>{spellErrors.length} spell</span>
            )}
          </label>
          <input
            style={{
              ...styles.input,
              ...(spellErrors.length > 0 ? styles.spellError : {}),
            }}
            value={strVal}
            onChange={(e) => onChange(e.target.value)}
            placeholder={field.placeholder ?? field.type}
            data-testid={`field-${field.id}`}
          />
          {spellErrors.length > 0 && (
            <div style={styles.meta}>
              {spellErrors.map((e) => `"${e.word}" → ${e.suggestions.join(", ") || "?"}`).join("; ")}
            </div>
          )}
        </div>
      );
  }
}

// ── Source Editor Section ────────────────────────────────────────────────────

const DEFAULT_SOURCE = `title: My Page
slug: my-page
published: true
views: 42
tags: draft, featured
description: A sample page for the facet form.`;

// ── Main Panel ──────────────────────────────────────────────────────────────

export function FormFacetPanel() {
  const kernel = useKernel();
  const { detectFormat, parseValues, serializeValues, inferFields } = useFacetParser();
  const { check } = useSpellCheck();

  const [source, setSource] = useState(DEFAULT_SOURCE);
  const [showSource, setShowSource] = useState(false);

  // Parse source → values → fields
  const format = useMemo(() => detectFormat(source), [source, detectFormat]);
  const values = useMemo(() => parseValues(source, format), [source, format, parseValues]);
  const fields = useMemo(() => inferFields(values), [values, inferFields]);

  // Spell-check text/textarea fields
  const spellResults = useMemo(() => {
    const results = new Map<string, Array<{ word: string; suggestions: string[] }>>();
    for (const field of fields) {
      if (field.type === "text" || field.type === "textarea") {
        const val = values[field.id];
        if (typeof val === "string" && val.length > 0) {
          const diagnostics = check(val);
          if (diagnostics.length > 0) {
            results.set(field.id, diagnostics.map((d) => ({ word: d.word, suggestions: d.suggestions })));
          }
        }
      }
    }
    return results;
  }, [fields, values, check]);

  const handleFieldChange = useCallback(
    (fieldId: string, newValue: unknown) => {
      const updated = { ...values, [fieldId]: newValue };
      const newSource = serializeValues(updated, format, source);
      setSource(newSource);
    },
    [values, format, source, serializeValues],
  );

  const handleSourceChange = useCallback(
    (newSource: string) => {
      setSource(newSource);
    },
    [],
  );

  const handleLoadSample = useCallback(() => {
    const sample = `{
  "name": "Alice",
  "email": "alice@example.com",
  "age": 30,
  "active": true,
  "bio": "A teh quick brown fox.",
  "website": "https://example.com"
}`;
    setSource(sample);
    kernel.notifications.add({ title: "Loaded JSON sample", kind: "info" });
  }, [kernel]);

  return (
    <div style={styles.container} data-testid="form-facet-panel">
      <div style={styles.header as React.CSSProperties}>
        <span>Form Facet</span>
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <span style={styles.badge}>{format.toUpperCase()}</span>
          <span style={styles.meta}>{fields.length} fields</span>
        </div>
      </div>

      {/* Controls */}
      <div style={{ display: "flex", gap: 6, marginBottom: "0.75rem" }}>
        <button
          style={showSource ? styles.btnPrimary : styles.btn}
          onClick={() => setShowSource(!showSource)}
          data-testid="toggle-source-btn"
        >
          {showSource ? "Hide Source" : "Show Source"}
        </button>
        <button style={styles.btn} onClick={handleLoadSample} data-testid="load-sample-btn">
          Load JSON Sample
        </button>
      </div>

      {/* Source editor (toggle) */}
      {showSource && (
        <div style={{ ...styles.card, marginBottom: "0.75rem" }}>
          <div style={styles.sectionTitle}>Source ({format})</div>
          <textarea
            style={{ ...styles.textarea, height: 120 }}
            value={source}
            onChange={(e) => handleSourceChange(e.target.value)}
            data-testid="source-editor"
          />
        </div>
      )}

      {/* Form fields */}
      <div style={styles.card} data-testid="form-fields">
        <div style={styles.sectionTitle}>Fields</div>
        {fields.length === 0 ? (
          <div style={{ color: "#555", fontStyle: "italic", padding: "0.5rem 0" }}>
            No fields detected. Enter YAML or JSON source above.
          </div>
        ) : (
          fields.map((field) => (
            <FormField
              key={field.id}
              field={field}
              value={values[field.id]}
              onChange={(v) => handleFieldChange(field.id, v)}
              spellErrors={spellResults.get(field.id) ?? []}
            />
          ))
        )}
      </div>

      {/* Spell check summary */}
      {spellResults.size > 0 && (
        <div style={styles.card} data-testid="spell-summary">
          <div style={styles.sectionTitle}>Spelling Issues</div>
          {[...spellResults.entries()].map(([fieldId, errors]) => (
            <div key={fieldId} style={{ marginBottom: 4 }}>
              <span style={{ color: "#aaa", fontSize: "0.75rem" }}>{fieldId}: </span>
              {errors.map((e, i) => (
                <span key={i} style={styles.badgeWarn}>{e.word}</span>
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
