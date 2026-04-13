# page-builder

Pure helpers for page-builder trees composed of `GraphObject` nodes. Provides the shared block-style schema (`BlockStyleData` + `STYLE_FIELD_DEFS`), a style compiler (`computeBlockStyle`), responsive overrides, a curated font registry with Google Fonts link generation, and deterministic page export to `prism-page/v1` JSON + dependency-free HTML. Framework-agnostic — style bags are `Record<string, string | number>` (structurally compatible with React's `CSSProperties`).

## Import

```ts
import {
  STYLE_FIELD_DEFS,
  computeBlockStyle,
  extractBlockStyle,
  mergeCss,
  resolveShadow,
  BREAKPOINTS,
  computeMobileOverride,
  computeTabletOverride,
  FONT_OPTIONS,
  googleFontsHref,
  exportPageToJson,
  exportPageToHtml,
} from "@prism/core/page-builder";
```

## Key exports

- `STYLE_FIELD_DEFS` — `EntityFieldDef[]` spread into block entity defs for per-block styling.
- `computeBlockStyle(data)` / `extractBlockStyle(obj)` / `mergeCss(...bags)` — build, read, and combine `CssStyle` bags.
- `resolveShadow(token)` — map `sm`/`md`/`lg` (or custom) to a CSS box-shadow; `SHADOW_PRESETS` exposed.
- `BREAKPOINTS`, `mediaRule`, `computeMobileOverride`, `computeTabletOverride`, `extractResponsive` — responsive overrides.
- `FONT_OPTIONS`, `findFontOption`, `isGoogleFontValue`, `googleFontsHref`, `collectFontFamilies` — curated font registry + Google Fonts v2 link builder, used by both the picker UI and the HTML exporter.
- `exportPageToJson(page, allObjects)` / `exportPageToHtml(page, allObjects, options?)` — deterministic page export.
- `toExportedNode`, `renderNodeHtml`, `escapeHtml`, `escapeAttr`, `cssToInline` — lower-level export primitives.
- Types: `BlockStyleData`, `ResponsiveBlockStyle`, `Viewport`, `CssStyle`, `ExportedNode`, `ExportedPage`, `HtmlExportOptions`, `FontCategory`, `FontOption`.

## Usage

```ts
import {
  computeBlockStyle,
  exportPageToHtml,
} from "@prism/core/page-builder";

const style = computeBlockStyle({
  background: "#fff",
  paddingX: 24,
  paddingY: 16,
  borderRadius: 8,
  shadow: "md",
  fontFamily: "Inter, sans-serif",
});

const html = exportPageToHtml(pageObject, allObjects, {
  title: "Landing",
});
```
