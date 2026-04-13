/**
 * Font registry + Google Fonts helpers.
 *
 * A curated list of CSS font stacks — system fallbacks plus a small set of
 * popular Google Fonts — used as the source for the Puck builder's font
 * picker and for `@import` / `<link>` injection on page export. Framework-
 * agnostic (no React, no DOM). Studio's `fontPickerField` plus the HTML
 * exporter both consume `FONT_OPTIONS` so the builder preview and the
 * exported page stay in sync.
 */

export type FontCategory = "sans" | "serif" | "mono" | "display";

export interface FontOption {
  /** CSS `font-family` value written into block style bags. */
  value: string;
  /** Display name shown in the picker dropdown. */
  label: string;
  category: FontCategory;
  /** True when the font is hosted by Google Fonts and needs a link tag. */
  google?: boolean;
  /** Weights loaded from Google Fonts. Defaults to [400]. */
  weights?: readonly number[];
}

/**
 * The canonical picker list. System stacks are first so offline / privacy-
 * sensitive setups get the defaults without hitting the network. Ordering
 * inside each group is by popularity (rough, not bindingly exact).
 */
export const FONT_OPTIONS: readonly FontOption[] = [
  // ── System stacks ────────────────────────────────────────────────────────
  {
    value: "system-ui, -apple-system, Segoe UI, Roboto, sans-serif",
    label: "System Sans",
    category: "sans",
  },
  {
    value: "ui-serif, Georgia, Cambria, Times New Roman, serif",
    label: "System Serif",
    category: "serif",
  },
  {
    value: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
    label: "System Mono",
    category: "mono",
  },

  // ── Google sans ──────────────────────────────────────────────────────────
  { value: "Inter, sans-serif", label: "Inter", category: "sans", google: true, weights: [400, 500, 600, 700] },
  { value: "Roboto, sans-serif", label: "Roboto", category: "sans", google: true, weights: [400, 500, 700] },
  { value: "Poppins, sans-serif", label: "Poppins", category: "sans", google: true, weights: [400, 500, 600, 700] },
  { value: "Nunito, sans-serif", label: "Nunito", category: "sans", google: true, weights: [400, 600, 700] },
  { value: "Open Sans, sans-serif", label: "Open Sans", category: "sans", google: true, weights: [400, 600, 700] },
  { value: "Lato, sans-serif", label: "Lato", category: "sans", google: true, weights: [400, 700] },
  { value: "Work Sans, sans-serif", label: "Work Sans", category: "sans", google: true, weights: [400, 600, 700] },

  // ── Google serif / display ───────────────────────────────────────────────
  { value: "Lora, serif", label: "Lora", category: "serif", google: true, weights: [400, 700] },
  { value: "Merriweather, serif", label: "Merriweather", category: "serif", google: true, weights: [400, 700] },
  { value: "Playfair Display, serif", label: "Playfair Display", category: "serif", google: true, weights: [400, 700] },
  { value: "DM Serif Display, serif", label: "DM Serif Display", category: "display", google: true, weights: [400] },

  // ── Google mono ──────────────────────────────────────────────────────────
  { value: "JetBrains Mono, monospace", label: "JetBrains Mono", category: "mono", google: true, weights: [400, 500, 700] },
  { value: "Fira Code, monospace", label: "Fira Code", category: "mono", google: true, weights: [400, 500, 700] },
  { value: "Space Mono, monospace", label: "Space Mono", category: "mono", google: true, weights: [400, 700] },
];

/** Strip quotes + whitespace from the first token of a font-family string. */
function firstFamily(value: string): string {
  const raw = value.split(",")[0] ?? "";
  return raw.trim().replace(/^['"]|['"]$/g, "");
}

/**
 * Resolve an arbitrary `font-family` string to a known `FontOption`. Matches
 * by exact value first, then by leading family name so stacks with different
 * fallbacks still find their canonical entry.
 */
export function findFontOption(value: string | undefined | null): FontOption | undefined {
  if (!value) return undefined;
  const exact = FONT_OPTIONS.find((f) => f.value === value);
  if (exact) return exact;
  const family = firstFamily(value);
  if (!family) return undefined;
  return FONT_OPTIONS.find((f) => f.label === family || firstFamily(f.value) === family);
}

/** True when the given font value resolves to a Google-hosted option. */
export function isGoogleFontValue(value: string | undefined | null): boolean {
  return findFontOption(value)?.google === true;
}

/**
 * Build a Google Fonts v2 CSS URL for every Google-hosted font in `values`.
 * Returns `undefined` when none of the inputs are Google fonts (so callers
 * can conditionally inject a `<link>` tag). Each family is requested with
 * its declared weights and `display=swap` for instant fallback rendering.
 */
export function googleFontsHref(values: ReadonlyArray<string>): string | undefined {
  const seen = new Set<string>();
  const opts: FontOption[] = [];
  for (const v of values) {
    const o = findFontOption(v);
    if (!o || !o.google) continue;
    if (seen.has(o.label)) continue;
    seen.add(o.label);
    opts.push(o);
  }
  if (opts.length === 0) return undefined;
  const families = opts
    .map((o) => {
      const family = o.label.replace(/ /g, "+");
      const weights = (o.weights ?? [400]).slice().sort((a, b) => a - b).join(";");
      return `family=${family}:wght@${weights}`;
    })
    .join("&");
  return `https://fonts.googleapis.com/css2?${families}&display=swap`;
}

/**
 * Deeply walk an exported node tree and collect every `fontFamily` value
 * referenced by any block's `data` bag. Used by the HTML exporter to decide
 * which Google Fonts `<link>` tag to inject, and by tests to assert font
 * coverage on a sample page.
 */
export function collectFontFamilies(node: {
  data?: Record<string, unknown> | undefined;
  children?: ReadonlyArray<unknown>;
}): string[] {
  const found = new Set<string>();
  const walk = (n: { data?: Record<string, unknown> | undefined; children?: ReadonlyArray<unknown> }) => {
    const ff = n.data?.["fontFamily"];
    if (typeof ff === "string" && ff.trim() !== "") found.add(ff);
    for (const c of n.children ?? []) {
      if (c && typeof c === "object") walk(c as { data?: Record<string, unknown>; children?: ReadonlyArray<unknown> });
    }
  };
  walk(node);
  return Array.from(found);
}
