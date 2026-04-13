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
import {
  FONT_OPTIONS,
  findFontOption,
  googleFontsHref,
  type FontCategory,
  type FontOption,
} from "@prism/core/page-builder";
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

// ── Font picker with live Google Fonts preview ─────────────────────────────

/**
 * Injects a Google Fonts `<link>` into `document.head` once per href. Safe to
 * call repeatedly — tracked by the module-level `loadedFontHrefs` set — and a
 * no-op when there is no `document` (vitest env) or when the font is not
 * Google-hosted. Each option is loaded individually so the picker can render
 * every dropdown row in its own face without a monster URL.
 */
const loadedFontHrefs = new Set<string>();
function ensureGoogleFontLoaded(value: string): void {
  if (typeof document === "undefined") return;
  const opt = findFontOption(value);
  if (!opt?.google) return;
  const href = googleFontsHref([value]);
  if (!href || loadedFontHrefs.has(href)) return;
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = href;
  link.setAttribute("data-prism-font", opt.label);
  document.head.appendChild(link);
  loadedFontHrefs.add(href);
}

const CATEGORY_LABELS: Record<FontCategory, string> = {
  sans: "Sans-serif",
  serif: "Serif",
  mono: "Monospace",
  display: "Display",
};

const CATEGORY_ORDER: readonly FontCategory[] = ["sans", "serif", "display", "mono"];

function groupFontsByCategory(
  opts: ReadonlyArray<FontOption>,
): ReadonlyArray<{ category: FontCategory; options: FontOption[] }> {
  const buckets = new Map<FontCategory, FontOption[]>();
  for (const o of opts) {
    const list = buckets.get(o.category) ?? [];
    list.push(o);
    buckets.set(o.category, list);
  }
  return CATEGORY_ORDER.filter((c) => buckets.has(c)).map((category) => ({
    category,
    options: buckets.get(category) ?? [],
  }));
}

/**
 * Native `<select>` grouped by font category. Each `<option>` is rendered in
 * its own `font-family`, so the dropdown *is* the preview. Google-hosted
 * faces are eagerly fetched on first render so the previews have something
 * to render against.
 */
export function fontPickerField(opts: { label?: string; offline?: boolean } = {}): Field<string> {
  const available = opts.offline ? FONT_OPTIONS.filter((o) => !o.google) : FONT_OPTIONS;
  if (!opts.offline) {
    for (const o of available) if (o.google) ensureGoogleFontLoaded(o.value);
  }
  const groups = groupFontsByCategory(available);
  return {
    type: "custom",
    ...(opts.label !== undefined ? { label: opts.label } : {}),
    render: ({ value, onChange, readOnly }): ReactElement => {
      const current = typeof value === "string" ? value : "";
      const resolved = findFontOption(current);
      const previewFamily = resolved?.value ?? current ?? "inherit";
      return withLabel(
        opts.label,
        readOnly,
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <select
            value={resolved?.value ?? ""}
            disabled={readOnly}
            onChange={(e) => onChange(e.target.value)}
            style={{ ...baseInput, fontFamily: previewFamily }}
            aria-label={opts.label ?? "Font family"}
          >
            <option value="" style={{ fontFamily: "system-ui, sans-serif" }}>
              — Default —
            </option>
            {groups.map((g) => (
              <optgroup key={g.category} label={CATEGORY_LABELS[g.category]}>
                {g.options.map((o) => (
                  <option key={o.value} value={o.value} style={{ fontFamily: o.value }}>
                    {o.label}
                  </option>
                ))}
              </optgroup>
            ))}
          </select>
          <div
            style={{
              padding: "8px 10px",
              border: "1px dashed #cbd5e1",
              borderRadius: 4,
              background: "#f8fafc",
              fontFamily: previewFamily,
              fontSize: 16,
              color: "#0f172a",
              minHeight: 28,
            }}
            aria-hidden
          >
            The quick brown fox jumps over the lazy dog.
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
