/**
 * Form input renderers — primitive input widgets for the Puck builder.
 *
 * Each renderer is an uncontrolled preview: it keeps its own local state
 * so users can try out the input inside the Canvas / Puck editor without
 * a surrounding <form>. Binding to kernel objects is handled by the
 * schema-driven Form Facet panel — these widgets are visual primitives
 * that live inside Pages.
 */

import { useId, useState } from "react";

// ── Shared style tokens ────────────────────────────────────────────────────

const labelStyle = {
  display: "block",
  fontSize: 12,
  fontWeight: 600,
  color: "#334155",
  marginBottom: 4,
};

const controlStyle = {
  width: "100%",
  padding: "6px 10px",
  fontSize: 13,
  border: "1px solid #cbd5e1",
  borderRadius: 4,
  background: "#ffffff",
  color: "#0f172a",
  outline: "none",
  boxSizing: "border-box" as const,
};

const helpStyle = {
  fontSize: 11,
  color: "#64748b",
  marginTop: 3,
};

const containerStyle = {
  margin: "0 0 8px 0",
};

// ── Text Input ─────────────────────────────────────────────────────────────

export interface TextInputProps {
  label?: string | undefined;
  placeholder?: string | undefined;
  defaultValue?: string | undefined;
  inputType?: "text" | "email" | "url" | "tel" | "password" | undefined;
  required?: boolean | undefined;
  help?: string | undefined;
}

export function TextInputRenderer(props: TextInputProps) {
  const { label, placeholder, defaultValue = "", inputType = "text", required, help } = props;
  const id = useId();
  const [value, setValue] = useState(defaultValue);
  return (
    <div data-testid="text-input" style={containerStyle}>
      {label ? (
        <label htmlFor={id} style={labelStyle}>
          {label}
          {required ? <span style={{ color: "#dc2626" }}> *</span> : null}
        </label>
      ) : null}
      <input
        id={id}
        type={inputType}
        placeholder={placeholder}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        style={controlStyle}
      />
      {help ? <div style={helpStyle}>{help}</div> : null}
    </div>
  );
}

// ── Textarea Input ─────────────────────────────────────────────────────────

export interface TextareaInputProps {
  label?: string | undefined;
  placeholder?: string | undefined;
  defaultValue?: string | undefined;
  rows?: number | undefined;
  required?: boolean | undefined;
  help?: string | undefined;
}

export function TextareaInputRenderer(props: TextareaInputProps) {
  const { label, placeholder, defaultValue = "", rows = 4, required, help } = props;
  const id = useId();
  const [value, setValue] = useState(defaultValue);
  return (
    <div data-testid="textarea-input" style={containerStyle}>
      {label ? (
        <label htmlFor={id} style={labelStyle}>
          {label}
          {required ? <span style={{ color: "#dc2626" }}> *</span> : null}
        </label>
      ) : null}
      <textarea
        id={id}
        placeholder={placeholder}
        rows={rows}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        style={{ ...controlStyle, resize: "vertical", fontFamily: "inherit" }}
      />
      {help ? <div style={helpStyle}>{help}</div> : null}
    </div>
  );
}

// ── Select Input ───────────────────────────────────────────────────────────

export interface SelectInputProps {
  label?: string | undefined;
  /** Comma-separated list or JSON array of values/`value:label` pairs. */
  options?: string | undefined;
  defaultValue?: string | undefined;
  required?: boolean | undefined;
  help?: string | undefined;
}

export interface SelectOption {
  value: string;
  label: string;
}

/** Parse the `options` string into structured options. */
export function parseSelectOptions(raw: string): SelectOption[] {
  const trimmed = raw.trim();
  if (!trimmed) return [];
  if (trimmed.startsWith("[")) {
    try {
      const parsed = JSON.parse(trimmed) as unknown;
      if (Array.isArray(parsed)) {
        return parsed.map((item) => {
          if (typeof item === "string") return { value: item, label: item };
          if (item && typeof item === "object") {
            const rec = item as Record<string, unknown>;
            const v = String(rec.value ?? rec.label ?? "");
            const l = String(rec.label ?? rec.value ?? v);
            return { value: v, label: l };
          }
          return { value: String(item), label: String(item) };
        });
      }
    } catch {
      /* fall through */
    }
  }
  return trimmed
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)
    .map((piece) => {
      const [v, l] = piece.split(":").map((s) => s.trim());
      return { value: v ?? piece, label: l ?? v ?? piece };
    });
}

export function SelectInputRenderer(props: SelectInputProps) {
  const { label, options = "", defaultValue = "", required, help } = props;
  const id = useId();
  const opts = parseSelectOptions(options);
  const [value, setValue] = useState(defaultValue || opts[0]?.value || "");
  return (
    <div data-testid="select-input" style={containerStyle}>
      {label ? (
        <label htmlFor={id} style={labelStyle}>
          {label}
          {required ? <span style={{ color: "#dc2626" }}> *</span> : null}
        </label>
      ) : null}
      <select
        id={id}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        style={controlStyle}
      >
        {opts.length === 0 ? (
          <option value="">(no options)</option>
        ) : (
          opts.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))
        )}
      </select>
      {help ? <div style={helpStyle}>{help}</div> : null}
    </div>
  );
}

// ── Checkbox Input ─────────────────────────────────────────────────────────

export interface CheckboxInputProps {
  label?: string | undefined;
  defaultChecked?: boolean | undefined;
  help?: string | undefined;
}

export function CheckboxInputRenderer(props: CheckboxInputProps) {
  const { label, defaultChecked = false, help } = props;
  const id = useId();
  const [checked, setChecked] = useState(defaultChecked);
  return (
    <div data-testid="checkbox-input" style={containerStyle}>
      <label
        htmlFor={id}
        style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13, color: "#334155" }}
      >
        <input
          id={id}
          type="checkbox"
          checked={checked}
          onChange={(e) => setChecked(e.target.checked)}
          style={{ width: 16, height: 16 }}
        />
        <span>{label ?? "Checkbox"}</span>
      </label>
      {help ? <div style={helpStyle}>{help}</div> : null}
    </div>
  );
}

// ── Number Input ───────────────────────────────────────────────────────────

export interface NumberInputProps {
  label?: string | undefined;
  defaultValue?: number | undefined;
  min?: number | undefined;
  max?: number | undefined;
  step?: number | undefined;
  required?: boolean | undefined;
  help?: string | undefined;
}

export function NumberInputRenderer(props: NumberInputProps) {
  const { label, defaultValue, min, max, step, required, help } = props;
  const id = useId();
  const [value, setValue] = useState<string>(
    defaultValue !== undefined && defaultValue !== null ? String(defaultValue) : "",
  );
  return (
    <div data-testid="number-input" style={containerStyle}>
      {label ? (
        <label htmlFor={id} style={labelStyle}>
          {label}
          {required ? <span style={{ color: "#dc2626" }}> *</span> : null}
        </label>
      ) : null}
      <input
        id={id}
        type="number"
        value={value}
        min={min}
        max={max}
        step={step}
        onChange={(e) => setValue(e.target.value)}
        style={controlStyle}
      />
      {help ? <div style={helpStyle}>{help}</div> : null}
    </div>
  );
}

// ── Date Input ─────────────────────────────────────────────────────────────

export interface DateInputProps {
  label?: string | undefined;
  defaultValue?: string | undefined;
  dateKind?: "date" | "datetime-local" | "time" | undefined;
  required?: boolean | undefined;
  help?: string | undefined;
}

export function DateInputRenderer(props: DateInputProps) {
  const { label, defaultValue = "", dateKind = "date", required, help } = props;
  const id = useId();
  const [value, setValue] = useState(defaultValue);
  return (
    <div data-testid="date-input" style={containerStyle}>
      {label ? (
        <label htmlFor={id} style={labelStyle}>
          {label}
          {required ? <span style={{ color: "#dc2626" }}> *</span> : null}
        </label>
      ) : null}
      <input
        id={id}
        type={dateKind}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        style={controlStyle}
      />
      {help ? <div style={helpStyle}>{help}</div> : null}
    </div>
  );
}
