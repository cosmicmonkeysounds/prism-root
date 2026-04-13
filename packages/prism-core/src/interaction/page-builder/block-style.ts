/**
 * Shared block-style primitives.
 *
 * Every page-builder block can expose a common set of styling knobs:
 * background, padding/margin, border, radius, shadow, text color,
 * font size/weight/family, alignment. These are shipped as a single
 * EntityFieldDef array so each entity def can spread them in, and as
 * a pure `computeBlockStyle()` helper that canvas/layout renderers
 * apply uniformly on top of their own base CSS.
 *
 * This is Tier 3 of `docs/dev/studio-checklist.md` — style properties,
 * typography controls, and a minimal layout knob set.
 */

import type { EntityFieldDef } from "@prism/core/object-model";

/**
 * Framework-agnostic CSS style bag — values are either strings (`"16px"`,
 * `"solid red"`) or plain numbers (treated as pixels by most CSS properties).
 * Compatible by shape with React's `CSSProperties` so React renderers can
 * pass the result through unchanged.
 */
export type CssStyle = Record<string, string | number>;

// Typed alias — the registry's EntityFieldDef is ungeneric; this matches.
type BlockFieldDef = EntityFieldDef;

// ── Data shape ──────────────────────────────────────────────────────────────

export interface BlockStyleData {
  background?: string | undefined;
  textColor?: string | undefined;
  paddingX?: number | undefined;
  paddingY?: number | undefined;
  marginX?: number | undefined;
  marginY?: number | undefined;
  borderWidth?: number | undefined;
  borderColor?: string | undefined;
  borderRadius?: number | undefined;
  shadow?: "none" | "sm" | "md" | "lg" | string | undefined;
  fontSize?: number | undefined;
  fontWeight?: number | string | undefined;
  fontFamily?: string | undefined;
  textAlign?: "left" | "center" | "right" | "justify" | string | undefined;
  lineHeight?: number | string | undefined;
  letterSpacing?: number | undefined;
  display?: "block" | "flex" | "grid" | "inline-block" | "inline-flex" | string | undefined;
  flexDirection?: "row" | "column" | "row-reverse" | "column-reverse" | string | undefined;
  gap?: number | undefined;
  alignItems?: string | undefined;
  justifyContent?: string | undefined;
  // Positioning — lets authors place a block absolutely on the page canvas
  // (HotGlue-style freeform layout) in addition to the default stacked flow.
  position?: "static" | "relative" | "absolute" | "fixed" | "sticky" | string | undefined;
  top?: number | string | undefined;
  left?: number | string | undefined;
  right?: number | string | undefined;
  bottom?: number | string | undefined;
  zIndex?: number | undefined;
  // Raw CSS declarations. Parsed by `parseCssDeclarations` and merged on top
  // of every other computed property, so this is the final per-block override.
  customCss?: string | undefined;
}

// ── Shadow presets ──────────────────────────────────────────────────────────

export const SHADOW_PRESETS: Record<string, string> = {
  none: "none",
  sm: "0 1px 2px rgba(0,0,0,0.08)",
  md: "0 2px 6px rgba(0,0,0,0.10)",
  lg: "0 8px 24px rgba(0,0,0,0.14)",
};

/** Resolve a shadow token into a CSS value. Passes custom strings through. */
export function resolveShadow(token: string | undefined | null): string | undefined {
  if (!token) return undefined;
  const mapped = SHADOW_PRESETS[token];
  return mapped ?? token;
}

// ── CSS computation ─────────────────────────────────────────────────────────

/** Finite-number guard. */
function num(value: unknown): number | undefined {
  const n = typeof value === "number" ? value : Number(value);
  return Number.isFinite(n) ? n : undefined;
}

/** Build a framework-agnostic CSS style bag from a BlockStyleData bag. */
export function computeBlockStyle(data: BlockStyleData | undefined | null): CssStyle {
  if (!data) return {};
  const css: CssStyle = {};

  if (data.background) css.background = data.background;
  if (data.textColor) css.color = data.textColor;

  const px = num(data.paddingX);
  const py = num(data.paddingY);
  if (px !== undefined || py !== undefined) {
    css.padding = `${py ?? 0}px ${px ?? 0}px`;
  }

  const mx = num(data.marginX);
  const my = num(data.marginY);
  if (mx !== undefined || my !== undefined) {
    css.margin = `${my ?? 0}px ${mx ?? 0}px`;
  }

  const bw = num(data.borderWidth);
  if (bw !== undefined && bw > 0) {
    css.border = `${bw}px solid ${data.borderColor ?? "#e2e8f0"}`;
  }

  const br = num(data.borderRadius);
  if (br !== undefined) css.borderRadius = br;

  const shadow = resolveShadow(data.shadow);
  if (shadow !== undefined) css.boxShadow = shadow;

  const fs = num(data.fontSize);
  if (fs !== undefined) css.fontSize = fs;
  if (data.fontWeight !== undefined && data.fontWeight !== "") {
    const wn = typeof data.fontWeight === "number" ? data.fontWeight : Number(data.fontWeight);
    css.fontWeight = Number.isFinite(wn) ? wn : (data.fontWeight as string);
  }
  if (data.fontFamily) css.fontFamily = data.fontFamily;
  if (data.textAlign) css.textAlign = data.textAlign;

  const lh = typeof data.lineHeight === "number" ? data.lineHeight : Number(data.lineHeight);
  if (Number.isFinite(lh)) css.lineHeight = lh;

  const ls = num(data.letterSpacing);
  if (ls !== undefined) css.letterSpacing = ls;

  if (data.display) css.display = data.display;
  if (data.flexDirection) css.flexDirection = data.flexDirection;

  const gap = num(data.gap);
  if (gap !== undefined) css.gap = gap;

  if (data.alignItems) css.alignItems = data.alignItems;
  if (data.justifyContent) css.justifyContent = data.justifyContent;

  // Positioning
  if (data.position) css.position = data.position;
  for (const k of ["top", "left", "right", "bottom"] as const) {
    const v = data[k];
    if (v === undefined || v === "") continue;
    const n = typeof v === "number" ? v : Number(v);
    css[k] = Number.isFinite(n) ? (n as number) : (v as string);
  }
  const zi = num(data.zIndex);
  if (zi !== undefined) css.zIndex = zi;

  // Raw CSS last — wins over everything above so it's a real escape hatch.
  if (data.customCss) {
    const parsed = parseCssDeclarations(data.customCss);
    for (const [k, v] of Object.entries(parsed)) css[k] = v;
  }

  return css;
}

/**
 * Parse a string of semicolon-separated CSS declarations (`color: red;
 * padding: 4px 8px;`) into a `CssStyle` bag. Keys are converted from
 * kebab-case to camelCase so React's `style` prop accepts them directly.
 * Plain numeric values (no unit) become numbers — everything else stays a
 * string. Invalid lines are silently skipped.
 */
export function parseCssDeclarations(src: string): CssStyle {
  const out: CssStyle = {};
  for (const decl of src.split(";")) {
    const idx = decl.indexOf(":");
    if (idx < 0) continue;
    const rawKey = decl.slice(0, idx).trim();
    const rawVal = decl.slice(idx + 1).trim();
    if (!rawKey || !rawVal) continue;
    const key = rawKey.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());
    out[key] = /^-?\d+(?:\.\d+)?$/.test(rawVal) ? Number(rawVal) : rawVal;
  }
  return out;
}

/** Merge two CSS style bags. Right-hand side wins. */
export function mergeCss(
  base: CssStyle | undefined,
  overlay: CssStyle | undefined,
): CssStyle {
  if (!base) return overlay ?? {};
  if (!overlay) return base;
  return { ...base, ...overlay };
}

// ── Field defs (to spread into EntityDef.fields) ────────────────────────────

/**
 * The canonical "Style" group of fields. Spread into any EntityDef.fields
 * array to expose per-block styling knobs in the inspector.
 */
export const STYLE_FIELD_DEFS: ReadonlyArray<BlockFieldDef> = [
  {
    id: "className",
    type: "string",
    label: "Tailwind Classes",
    ui: {
      group: "Tailwind",
      placeholder: "e.g. bg-blue-500 text-white rounded-lg p-6 shadow-lg",
      multiline: true,
    },
  },
  { id: "background", type: "color", label: "Background", ui: { group: "Style" } },
  { id: "textColor", type: "color", label: "Text Color", ui: { group: "Style" } },
  { id: "paddingX", type: "int", label: "Padding X (px)", ui: { group: "Style" } },
  { id: "paddingY", type: "int", label: "Padding Y (px)", ui: { group: "Style" } },
  { id: "marginX", type: "int", label: "Margin X (px)", ui: { group: "Style" } },
  { id: "marginY", type: "int", label: "Margin Y (px)", ui: { group: "Style" } },
  { id: "borderWidth", type: "int", label: "Border Width (px)", ui: { group: "Style" } },
  { id: "borderColor", type: "color", label: "Border Color", ui: { group: "Style" } },
  { id: "borderRadius", type: "int", label: "Border Radius (px)", ui: { group: "Style" } },
  {
    id: "shadow",
    type: "enum",
    label: "Shadow",
    default: "none",
    enumOptions: [
      { value: "none", label: "None" },
      { value: "sm", label: "Small" },
      { value: "md", label: "Medium" },
      { value: "lg", label: "Large" },
    ],
    ui: { group: "Style" },
  },
  { id: "fontFamily", type: "string", label: "Font Family", ui: { group: "Typography" } },
  { id: "fontSize", type: "int", label: "Font Size (px)", ui: { group: "Typography" } },
  { id: "fontWeight", type: "int", label: "Font Weight", ui: { group: "Typography" } },
  { id: "lineHeight", type: "float", label: "Line Height", ui: { group: "Typography" } },
  { id: "letterSpacing", type: "int", label: "Letter Spacing (px)", ui: { group: "Typography" } },
  {
    id: "textAlign",
    type: "enum",
    label: "Text Align",
    default: "left",
    enumOptions: [
      { value: "left", label: "Left" },
      { value: "center", label: "Center" },
      { value: "right", label: "Right" },
      { value: "justify", label: "Justify" },
    ],
    ui: { group: "Typography" },
  },
  {
    id: "position",
    type: "enum",
    label: "Position",
    default: "",
    enumOptions: [
      { value: "", label: "Default (flow)" },
      { value: "relative", label: "Relative" },
      { value: "absolute", label: "Absolute (freeform)" },
      { value: "fixed", label: "Fixed" },
      { value: "sticky", label: "Sticky" },
    ],
    ui: { group: "Position" },
  },
  { id: "top", type: "int", label: "Top (px)", ui: { group: "Position" } },
  { id: "left", type: "int", label: "Left (px)", ui: { group: "Position" } },
  { id: "right", type: "int", label: "Right (px)", ui: { group: "Position" } },
  { id: "bottom", type: "int", label: "Bottom (px)", ui: { group: "Position" } },
  { id: "zIndex", type: "int", label: "Z Index", ui: { group: "Position" } },
  {
    id: "customCss",
    type: "string",
    label: "Custom CSS",
    ui: {
      group: "Style",
      placeholder: "color: red;\npadding: 12px 16px;",
      multiline: true,
    },
  },
  { id: "visibleWhen", type: "string", label: "Visible When (expression)", ui: { group: "Behavior", placeholder: "e.g. 1 == 1" } },
  { id: "hiddenMobile", type: "bool", label: "Hide on Mobile", default: false, ui: { group: "Responsive" } },
  { id: "hiddenTablet", type: "bool", label: "Hide on Tablet", default: false, ui: { group: "Responsive" } },
  { id: "mobilePaddingX", type: "int", label: "Mobile Padding X", ui: { group: "Responsive" } },
  { id: "mobilePaddingY", type: "int", label: "Mobile Padding Y", ui: { group: "Responsive" } },
  { id: "mobileFontSize", type: "int", label: "Mobile Font Size", ui: { group: "Responsive" } },
  { id: "mobileTextAlign", type: "enum", label: "Mobile Text Align", enumOptions: [
    { value: "left", label: "Left" },
    { value: "center", label: "Center" },
    { value: "right", label: "Right" },
  ], ui: { group: "Responsive" } },
];

// ── Responsive overrides ────────────────────────────────────────────────────

/** Common viewport breakpoints (px). */
export const BREAKPOINTS = {
  mobile: 640,
  tablet: 1024,
} as const;

export type Viewport = "mobile" | "tablet" | "desktop";

export interface ResponsiveBlockStyle {
  hiddenMobile?: boolean | undefined;
  hiddenTablet?: boolean | undefined;
  mobilePaddingX?: number | undefined;
  mobilePaddingY?: number | undefined;
  mobileFontSize?: number | undefined;
  mobileTextAlign?: string | undefined;
}

/** Build a `@media` CSS rule string for a given breakpoint + CSS body. */
export function mediaRule(viewport: Exclude<Viewport, "desktop">, selector: string, css: CssStyle): string {
  const body = Object.entries(css)
    .map(([k, v]) => {
      if (v === undefined || v === null || v === "") return "";
      const key = k.replace(/[A-Z]/g, (m) => `-${m.toLowerCase()}`);
      const value = typeof v === "number" && !key.endsWith("line-height") ? `${v}px` : String(v);
      return `${key}: ${value};`;
    })
    .filter(Boolean)
    .join(" ");
  if (!body) return "";
  const max = BREAKPOINTS[viewport];
  return `@media (max-width: ${max - 1}px) { ${selector} { ${body} } }`;
}

/** Extract just the mobile-specific override CSS from a data bag. */
export function computeMobileOverride(data: ResponsiveBlockStyle | undefined | null): CssStyle {
  if (!data) return {};
  const css: CssStyle = {};
  if (data.hiddenMobile) {
    css.display = "none";
    return css;
  }
  const px = typeof data.mobilePaddingX === "number" ? data.mobilePaddingX : undefined;
  const py = typeof data.mobilePaddingY === "number" ? data.mobilePaddingY : undefined;
  if (px !== undefined || py !== undefined) {
    css.padding = `${py ?? 0}px ${px ?? 0}px`;
  }
  if (typeof data.mobileFontSize === "number") css.fontSize = data.mobileFontSize;
  if (data.mobileTextAlign) css.textAlign = data.mobileTextAlign;
  return css;
}

/** Same as `computeMobileOverride` but for the tablet breakpoint (hide-only for now). */
export function computeTabletOverride(data: ResponsiveBlockStyle | undefined | null): CssStyle {
  if (!data) return {};
  if (data.hiddenTablet) return { display: "none" };
  return {};
}

/** Extract responsive overrides from a data bag. */
export function extractResponsive(data: unknown): ResponsiveBlockStyle {
  if (!data || typeof data !== "object") return {};
  const d = data as Record<string, unknown>;
  const out: ResponsiveBlockStyle = {};
  if (d["hiddenMobile"] !== undefined) out.hiddenMobile = !!d["hiddenMobile"];
  if (d["hiddenTablet"] !== undefined) out.hiddenTablet = !!d["hiddenTablet"];
  if (typeof d["mobilePaddingX"] === "number") out.mobilePaddingX = d["mobilePaddingX"];
  if (typeof d["mobilePaddingY"] === "number") out.mobilePaddingY = d["mobilePaddingY"];
  if (typeof d["mobileFontSize"] === "number") out.mobileFontSize = d["mobileFontSize"];
  if (d["mobileTextAlign"]) out.mobileTextAlign = d["mobileTextAlign"] as string;
  return out;
}

/**
 * Extract a BlockStyleData bag from any `GraphObject.data` payload.
 * Unknown keys are ignored; missing keys are undefined.
 */
export function extractBlockStyle(data: unknown): BlockStyleData {
  if (!data || typeof data !== "object") return {};
  const d = data as Record<string, unknown>;
  const result: BlockStyleData = {};
  const keys: (keyof BlockStyleData)[] = [
    "background",
    "textColor",
    "paddingX",
    "paddingY",
    "marginX",
    "marginY",
    "borderWidth",
    "borderColor",
    "borderRadius",
    "shadow",
    "fontSize",
    "fontWeight",
    "fontFamily",
    "textAlign",
    "lineHeight",
    "letterSpacing",
    "display",
    "flexDirection",
    "gap",
    "alignItems",
    "justifyContent",
    "position",
    "top",
    "left",
    "right",
    "bottom",
    "zIndex",
    "customCss",
  ];
  for (const k of keys) {
    if (d[k as string] !== undefined && d[k as string] !== "") {
      (result as Record<string, unknown>)[k as string] = d[k as string];
    }
  }
  return result;
}
