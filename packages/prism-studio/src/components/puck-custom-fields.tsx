/**
 * Specialized Puck field renderers.
 *
 * Puck ships a small set of primitive field types (text/textarea/number/
 * select/radio). Those are fine for data entry but too coarse for visual
 * styling: you can't pick a color without typing hex, can't align without
 * tapping a dropdown, can't adjust spacing without a bare number input.
 *
 * Each factory here returns a Puck `CustomField` — the framework renders
 * whatever React tree we return and wires `value`/`onChange` for live,
 * per-keystroke updates (same path Puck uses for its own fields, so the
 * canvas preview updates in real time without any extra debouncing).
 *
 * Each render is wrapped in Puck's own `FieldLabel` so labels look and
 * behave identically to Puck's built-in fields.
 */

import { FieldLabel, type Field } from "@measured/puck";
import type { CSSProperties, ReactElement, ReactNode } from "react";

// ── Shared look ─────────────────────────────────────────────────────────────

const baseInput: CSSProperties = {
  width: "100%",
  padding: "6px 8px",
  fontSize: 13,
  border: "1px solid #cbd5e1",
  borderRadius: 4,
  background: "#ffffff",
  color: "#0f172a",
  boxSizing: "border-box",
};

const mutedLabel: CSSProperties = {
  fontSize: 11,
  color: "#64748b",
  fontVariantNumeric: "tabular-nums",
};

/** Wrap a custom field body in Puck's FieldLabel so it matches built-ins. */
function withLabel(
  label: string | undefined,
  readOnly: boolean | undefined,
  body: ReactNode,
): ReactElement {
  return (
    <FieldLabel label={label ?? ""} el="div" readOnly={readOnly ?? false}>
      {body}
    </FieldLabel>
  );
}

// ── Color picker ────────────────────────────────────────────────────────────

/**
 * Swatch + hex input. Both controls are bound to the same value so edits
 * propagate bidirectionally: clicking the swatch opens the native picker,
 * typing in the text field accepts any CSS color string (hex, rgb, named).
 */
export function colorField(opts: { label?: string } = {}): Field<string> {
  return {
    type: "custom",
    ...(opts.label !== undefined ? { label: opts.label } : {}),
    render: ({ value, onChange, readOnly }): ReactElement => {
      const current = typeof value === "string" ? value : "";
      const hexForPicker = isHexColor(current) ? current : "#ffffff";
      return withLabel(
        opts.label,
        readOnly,
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <input
            type="color"
            value={hexForPicker}
            disabled={readOnly}
            onChange={(e) => onChange(e.target.value)}
            style={{
              width: 36,
              height: 28,
              padding: 0,
              border: "1px solid #cbd5e1",
              borderRadius: 4,
              background: "transparent",
              cursor: readOnly ? "default" : "pointer",
            }}
            aria-label={opts.label ? `${opts.label} swatch` : "Color swatch"}
          />
          <input
            type="text"
            value={current}
            disabled={readOnly}
            placeholder="#000000"
            onChange={(e) => onChange(e.target.value)}
            style={{ ...baseInput, flex: 1 }}
            aria-label={opts.label ?? "Color value"}
          />
          {current ? (
            <button
              type="button"
              onClick={() => onChange("")}
              disabled={readOnly}
              title="Clear"
              style={{
                border: "1px solid #cbd5e1",
                background: "#f8fafc",
                borderRadius: 4,
                padding: "4px 8px",
                fontSize: 12,
                cursor: readOnly ? "default" : "pointer",
                color: "#64748b",
              }}
            >
              ×
            </button>
          ) : null}
        </div>,
      );
    },
  };
}

function isHexColor(s: string): boolean {
  return /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(s);
}

// ── Alignment segmented control ─────────────────────────────────────────────

/**
 * Horizontal button group — one press per value. Used for align / textAlign
 * enum fields that benefit from glanceable icons over a dropdown.
 */
export function alignField(opts: {
  label?: string;
  options: ReadonlyArray<{ value: string; label: string }>;
}): Field<string> {
  return {
    type: "custom",
    ...(opts.label !== undefined ? { label: opts.label } : {}),
    render: ({ value, onChange, readOnly }): ReactElement => {
      const current = typeof value === "string" ? value : "";
      return withLabel(
        opts.label,
        readOnly,
        <div
          role="radiogroup"
          aria-label={opts.label ?? "Alignment"}
          style={{
            display: "inline-flex",
            borderRadius: 6,
            border: "1px solid #cbd5e1",
            overflow: "hidden",
            background: "#f8fafc",
          }}
        >
          {opts.options.map((o, i) => {
            const active = current === o.value;
            return (
              <button
                key={o.value}
                type="button"
                role="radio"
                aria-checked={active}
                disabled={readOnly}
                onClick={() => onChange(o.value)}
                title={o.label}
                style={{
                  padding: "6px 10px",
                  fontSize: 12,
                  background: active ? "#6366f1" : "transparent",
                  color: active ? "#ffffff" : "#334155",
                  border: "none",
                  borderLeft: i === 0 ? "none" : "1px solid #cbd5e1",
                  cursor: readOnly ? "default" : "pointer",
                  fontWeight: active ? 600 : 400,
                  minWidth: 44,
                }}
              >
                {alignIcon(o.value) ?? o.label}
              </button>
            );
          })}
        </div>,
      );
    },
  };
}

function alignIcon(v: string): string | undefined {
  switch (v) {
    case "left":
      return "⇤";
    case "center":
      return "↔";
    case "right":
      return "⇥";
    case "justify":
      return "☰";
    default:
      return undefined;
  }
}

// ── Slider + numeric readout ────────────────────────────────────────────────

/**
 * Range slider bound to a numeric value, with a small number readout and
 * unit suffix. Good for padding/margin/font-size/radius etc.
 */
export function sliderField(opts: {
  label?: string;
  min: number;
  max: number;
  step?: number;
  unit?: string;
}): Field<number> {
  const step = opts.step ?? 1;
  return {
    type: "custom",
    ...(opts.label !== undefined ? { label: opts.label } : {}),
    render: ({ value, onChange, readOnly }): ReactElement => {
      const current = typeof value === "number" && Number.isFinite(value) ? value : 0;
      return withLabel(
        opts.label,
        readOnly,
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <input
            type="range"
            min={opts.min}
            max={opts.max}
            step={step}
            value={current}
            disabled={readOnly}
            onChange={(e) => onChange(Number(e.target.value))}
            style={{ flex: 1, accentColor: "#6366f1" }}
            aria-label={opts.label ?? "Slider"}
          />
          <input
            type="number"
            min={opts.min}
            max={opts.max}
            step={step}
            value={current}
            disabled={readOnly}
            onChange={(e) => {
              const n = Number(e.target.value);
              if (Number.isFinite(n)) onChange(n);
            }}
            style={{ ...baseInput, width: 60, textAlign: "right" }}
          />
          {opts.unit ? <span style={mutedLabel}>{opts.unit}</span> : null}
        </div>,
      );
    },
  };
}

// ── URL field with open-in-new-tab ──────────────────────────────────────────

/**
 * Text input plus a small "open" button that opens the current URL in a new
 * tab. Button disables itself when the value isn't an http(s) URL.
 */
export function urlField(opts: { label?: string } = {}): Field<string> {
  return {
    type: "custom",
    ...(opts.label !== undefined ? { label: opts.label } : {}),
    render: ({ value, onChange, readOnly }): ReactElement => {
      const current = typeof value === "string" ? value : "";
      const canOpen = /^https?:\/\//i.test(current);
      return withLabel(
        opts.label,
        readOnly,
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <input
            type="text"
            value={current}
            disabled={readOnly}
            placeholder="https://…"
            onChange={(e) => onChange(e.target.value)}
            style={{ ...baseInput, flex: 1 }}
            aria-label={opts.label ?? "URL"}
          />
          <a
            href={canOpen ? current : undefined}
            target="_blank"
            rel="noopener noreferrer"
            aria-disabled={!canOpen}
            title={canOpen ? "Open in new tab" : "Enter an http(s) URL"}
            style={{
              display: "inline-flex",
              alignItems: "center",
              padding: "4px 8px",
              border: "1px solid #cbd5e1",
              borderRadius: 4,
              fontSize: 12,
              textDecoration: "none",
              color: canOpen ? "#6366f1" : "#94a3b8",
              background: "#f8fafc",
              pointerEvents: canOpen ? "auto" : "none",
            }}
          >
            ↗
          </a>
        </div>,
      );
    },
  };
}

// ── Tailwind className input ────────────────────────────────────────────────

/**
 * Textarea tuned for pasting Tailwind utility classes. Shows a small help
 * line under the input reminding users what's expected. Uses monospace
 * font so long class lists are easier to parse by eye.
 */
export function classNameField(opts: { label?: string; placeholder?: string } = {}): Field<string> {
  return {
    type: "custom",
    ...(opts.label !== undefined ? { label: opts.label } : {}),
    render: ({ value, onChange, readOnly }): ReactElement => {
      const current = typeof value === "string" ? value : "";
      return withLabel(
        opts.label,
        readOnly,
        <div>
          <textarea
            value={current}
            disabled={readOnly}
            placeholder={opts.placeholder ?? "e.g. bg-blue-500 text-white rounded-lg p-6"}
            onChange={(e) => onChange(e.target.value)}
            spellCheck={false}
            rows={2}
            style={{
              ...baseInput,
              fontFamily: "ui-monospace, Menlo, Consolas, monospace",
              fontSize: 12,
              resize: "vertical",
              minHeight: 40,
            }}
            aria-label={opts.label ?? "Tailwind classes"}
          />
          <div style={{ ...mutedLabel, marginTop: 4 }}>
            Any Tailwind utility classes. Applied to the component root.
          </div>
        </div>,
      );
    },
  };
}

// ── Raw CSS declarations ───────────────────────────────────────────────────

/**
 * Textarea accepting plain CSS declarations (`key: value;` pairs). Parsed by
 * `computeBlockStyle` and merged on top of every other style source, so this
 * is the final per-component override — useful when a look isn't reachable
 * via the schema fields or a Tailwind class.
 */
export function customCssField(opts: { label?: string; placeholder?: string } = {}): Field<string> {
  return {
    type: "custom",
    ...(opts.label !== undefined ? { label: opts.label } : {}),
    render: ({ value, onChange, readOnly }): ReactElement => {
      const current = typeof value === "string" ? value : "";
      return withLabel(
        opts.label,
        readOnly,
        <div>
          <textarea
            value={current}
            disabled={readOnly}
            placeholder={opts.placeholder ?? "color: red;\npadding: 12px 16px;"}
            onChange={(e) => onChange(e.target.value)}
            spellCheck={false}
            rows={3}
            style={{
              ...baseInput,
              fontFamily: "ui-monospace, Menlo, Consolas, monospace",
              fontSize: 12,
              resize: "vertical",
              minHeight: 60,
            }}
            aria-label={opts.label ?? "Custom CSS"}
          />
          <div style={{ ...mutedLabel, marginTop: 4 }}>
            Semicolon-separated CSS declarations. Overrides all other styles.
          </div>
        </div>,
      );
    },
  };
}
