/**
 * @prism/core/page-builder — pure helpers for building and exporting
 * page-builder trees composed of GraphObject nodes.
 *
 * - `block-style` — BlockStyleData shape, STYLE_FIELD_DEFS, computeBlockStyle,
 *   extractBlockStyle, mergeCss, resolveShadow, responsive overrides.
 * - `page-export` — deterministic JSON + dependency-free HTML export from
 *   a page → section → block tree.
 *
 * Framework-agnostic: no React imports. The computed style objects are
 * `Record<string, string | number>` (structurally compatible with React's
 * `CSSProperties`).
 */

export type {
  BlockStyleData,
  ResponsiveBlockStyle,
  Viewport,
  CssStyle,
} from "./block-style.js";
export {
  SHADOW_PRESETS,
  STYLE_FIELD_DEFS,
  BREAKPOINTS,
  resolveShadow,
  computeBlockStyle,
  mergeCss,
  mediaRule,
  computeMobileOverride,
  computeTabletOverride,
  extractBlockStyle,
  extractResponsive,
} from "./block-style.js";

export type {
  ExportedNode,
  ExportedPage,
  HtmlExportOptions,
} from "./page-export.js";
export {
  exportPageToJson,
  exportPageToHtml,
  toExportedNode,
  renderNodeHtml,
  escapeHtml,
  escapeAttr,
  cssToInline,
} from "./page-export.js";

export type { FontCategory, FontOption } from "./fonts.js";
export {
  FONT_OPTIONS,
  findFontOption,
  isGoogleFontValue,
  googleFontsHref,
  collectFontFamilies,
} from "./fonts.js";
