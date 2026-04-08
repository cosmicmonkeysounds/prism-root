/**
 * FormSurface — schema-driven form view for the Document Surface.
 *
 * Reads YAML or JSON document text, derives a flat record of values,
 * renders one input per top-level key, and round-trips edits back to text.
 *
 * - YAML or JSON is auto-detected from the source.
 * - When `schema.fields` is provided, those drive label/type/order; otherwise
 *   the form is inferred from the parsed object.
 * - Output preserves the source format on save.
 */

import { useCallback, useMemo, type CSSProperties } from "react";
import type { FieldSchema, FieldType } from "../../layer1/forms/field-schema.js";
import type { FormSchema } from "../../layer1/forms/form-schema.js";
import { detectFormat, parseValues, serializeValues, type SourceFormat } from "../../layer1/facet/facet-parser.js";

// ── Props ───────────────────────────────────────────────────────────────────

export interface FormSurfaceProps {
  /** Current text contents of the document. */
  value: string;
  /** Called with new text when a field is edited. */
  onChange?: ((value: string) => void) | undefined;
  /** Optional explicit schema. If absent, fields are inferred from parsed values. */
  schema?: FormSchema | undefined;
  /** File path — used as a hint for format detection. */
  filePath?: string | undefined;
  /** Disable editing. */
  readOnly?: boolean | undefined;
}

// ── Styles ──────────────────────────────────────────────────────────────────

const styles: Record<string, CSSProperties> = {
  root: {
    padding: 24,
    height: "100%",
    overflow: "auto",
    background: "#0b0b0e",
    color: "#e5e7eb",
    fontFamily: "system-ui, sans-serif",
    fontSize: 13,
  },
  field: {
    display: "flex",
    flexDirection: "column",
    gap: 4,
    marginBottom: 14,
    maxWidth: 600,
  },
  label: { color: "#a1a1aa", fontSize: 12, fontWeight: 500 },
  input: {
    padding: "8px 10px",
    background: "#18181b",
    border: "1px solid #2a2a30",
    borderRadius: 6,
    color: "#e5e7eb",
    fontSize: 13,
    outline: "none",
  },
  textarea: {
    padding: "8px 10px",
    background: "#18181b",
    border: "1px solid #2a2a30",
    borderRadius: 6,
    color: "#e5e7eb",
    fontSize: 13,
    outline: "none",
    minHeight: 80,
    resize: "vertical",
    fontFamily: "inherit",
  },
  empty: { color: "#71717a", fontStyle: "italic", padding: 16 },
  error: {
    color: "#fca5a5",
    background: "rgba(220,38,38,0.08)",
    border: "1px solid rgba(220,38,38,0.3)",
    borderRadius: 6,
    padding: 12,
    marginBottom: 12,
    fontSize: 12,
  },
};

// ── Inference ───────────────────────────────────────────────────────────────

export function inferFieldType(value: unknown): FieldType {
  if (typeof value === "boolean") return "boolean";
  if (typeof value === "number") return "number";
  if (typeof value === "string") {
    if (/^\d{4}-\d{2}-\d{2}T/.test(value)) return "datetime";
    if (/^\d{4}-\d{2}-\d{2}$/.test(value)) return "date";
    if (value.length > 80 || value.includes("\n")) return "textarea";
  }
  return "text";
}

export function titleCase(id: string): string {
  return id
    .replace(/[_-]+/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

export function deriveFields(values: Record<string, unknown>): FieldSchema[] {
  return Object.keys(values).map((id) => ({
    id,
    label: titleCase(id),
    type: inferFieldType(values[id]),
  }));
}

// ── Coercion ────────────────────────────────────────────────────────────────

export function coerce(field: FieldSchema, raw: string): unknown {
  switch (field.type) {
    case "number":
    case "currency":
    case "rating":
    case "slider":
    case "duration": {
      if (raw === "") return null;
      const n = Number(raw);
      return Number.isNaN(n) ? raw : n;
    }
    case "boolean":
      return raw === "true";
    default:
      return raw;
  }
}

// ── Component ───────────────────────────────────────────────────────────────

export function FormSurface({ value, onChange, schema, filePath: _filePath, readOnly = false }: FormSurfaceProps) {
  const format: SourceFormat = useMemo(() => detectFormat(value), [value]);

  const { values, error } = useMemo<{ values: Record<string, unknown>; error: string | null }>(() => {
    if (!value.trim()) return { values: {}, error: null };
    try {
      return { values: parseValues(value, format), error: null };
    } catch (err) {
      return {
        values: {},
        error: err instanceof Error ? err.message : "Failed to parse document",
      };
    }
  }, [value, format]);

  const fields = useMemo<FieldSchema[]>(() => {
    if (schema?.fields && schema.fields.length > 0) return schema.fields;
    return deriveFields(values);
  }, [schema, values]);

  const handleFieldChange = useCallback(
    (field: FieldSchema, raw: string) => {
      if (!onChange || readOnly) return;
      const next = { ...values, [field.id]: coerce(field, raw) };
      onChange(serializeValues(next, format, value));
    },
    [values, format, value, onChange, readOnly],
  );

  if (error) {
    return (
      <div style={styles.root} data-testid="form-surface">
        <div style={styles.error}>Parse error: {error}</div>
      </div>
    );
  }

  if (fields.length === 0) {
    return (
      <div style={styles.root} data-testid="form-surface">
        <div style={styles.empty}>No fields to display. Add YAML/JSON keys to the document.</div>
      </div>
    );
  }

  return (
    <div style={styles.root} data-testid="form-surface">
      {fields.map((field) => {
        const current = values[field.id];
        return (
          <div key={field.id} style={styles.field}>
            <label style={styles.label}>{field.label}</label>
            {renderInput(field, current, (raw) => handleFieldChange(field, raw), readOnly)}
          </div>
        );
      })}
    </div>
  );
}

function renderInput(
  field: FieldSchema,
  current: unknown,
  onChange: (raw: string) => void,
  readOnly: boolean,
) {
  const stringValue = current == null ? "" : String(current);

  if (field.type === "boolean") {
    return (
      <input
        type="checkbox"
        checked={current === true}
        disabled={readOnly}
        onChange={(e) => onChange(e.target.checked ? "true" : "false")}
        data-testid={`form-field-${field.id}`}
      />
    );
  }

  if (field.type === "textarea" || field.type === "rich-text") {
    return (
      <textarea
        style={styles.textarea}
        value={stringValue}
        readOnly={readOnly}
        onChange={(e) => onChange(e.target.value)}
        data-testid={`form-field-${field.id}`}
      />
    );
  }

  if (field.type === "number" || field.type === "currency" || field.type === "duration" || field.type === "rating" || field.type === "slider") {
    return (
      <input
        type="number"
        style={styles.input}
        value={stringValue}
        readOnly={readOnly}
        onChange={(e) => onChange(e.target.value)}
        data-testid={`form-field-${field.id}`}
      />
    );
  }

  if (field.type === "date") {
    return (
      <input
        type="date"
        style={styles.input}
        value={stringValue}
        readOnly={readOnly}
        onChange={(e) => onChange(e.target.value)}
        data-testid={`form-field-${field.id}`}
      />
    );
  }

  if (field.type === "datetime") {
    return (
      <input
        type="datetime-local"
        style={styles.input}
        value={stringValue.slice(0, 16)}
        readOnly={readOnly}
        onChange={(e) => onChange(e.target.value)}
        data-testid={`form-field-${field.id}`}
      />
    );
  }

  return (
    <input
      type="text"
      style={styles.input}
      value={stringValue}
      readOnly={readOnly}
      onChange={(e) => onChange(e.target.value)}
      data-testid={`form-field-${field.id}`}
    />
  );
}
