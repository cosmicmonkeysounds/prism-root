# FileMaker Pro Parity — Gap Analysis

Status tracking for closing the gap between Prism Studio and FileMaker Pro's
layout/data capabilities. Updated as features land.

## Architecture

Puck is the **top-level page compositor**. Data-aware components nest inside it:

| Component | Purpose | Status |
|-----------|---------|--------|
| `facet-view` | Renders a FacetDefinition as form/list/table/report/card | Done |
| `spatial-canvas` | Free-form (x,y) absolute positioning of fields | Done |
| `data-portal` | Related records via edge relationships (inline table) | Done |
| `lua-block` | Custom UI via Lua scripting | Done |

Libraries: `react-moveable` + `react-selecto` + `@scena/react-guides` (Scena ecosystem).

---

## Gap Tracker

### P0 — Layout Foundation

| Gap | FileMaker | Prism | Status |
|-----|-----------|-------|--------|
| Free-form positioning | Absolute (x,y) for every object | `SpatialCanvasRenderer` with Moveable | **Done** |
| Part bands | Resizable horizontal bands (header/body/footer/summary) | `computePartBands()` + visual band rendering | **Done** |
| Snap-to-grid | Grid snap for all positioning | `snapToGrid()` + Moveable snapGrid | **Done** |
| Resize handles | 8-direction resize on every object | Moveable `resizable` | **Done** |
| Multi-select | Shift-click + marquee select | Selecto + shift-click | **Done** |
| Alignment guides | Snap-to-element blue/red lines | Moveable `snappable` (configured) | **Done** |
| Alignment commands | Left/right/top/bottom/center-h/center-v | `alignSlots()` pure function | **Done** |
| Distribute commands | Even spacing horizontal/vertical | `distributeSlots()` pure function | **Done** |
| Z-ordering | Layer objects front/back | `zIndex` on all slot types + `sortByZIndex()` | **Done** |
| Drawing objects | Lines, rectangles, ellipses | `DrawingSlot` type + SVG rendering | **Done** |
| Text labels | Static/merge-field text objects | `TextSlot` type with styling props | **Done** |
| Schema types | Spatial properties on all slots | `x/y/w/h/zIndex` on FieldSlot, PortalSlot, TextSlot, DrawingSlot | **Done** |
| Builder API | Programmatic layout construction | `.addText()`, `.addDrawing()`, `.layoutMode()`, `.canvasSize()` | **Done** |
| Canvas editor | Dedicated layout editing lens | `SpatialCanvasPanel` (lens #23, Shift+X) | **Done** |

### P1 — Data Views & Navigation

| Gap | FileMaker | Prism | Status |
|-----|-----------|-------|--------|
| Multiple layouts per table | Many layouts, switch freely | FacetDefinition per objectType, FacetViewRenderer | **Done** |
| Layout picker | UI to select which layout is active | `facet-view` component with `viewMode` prop | **Partial** — needs dropdown UI |
| Portal rendering | Related records inline | `DataPortalRenderer` + `data-portal` Puck component | **Done** |
| Conditional formatting | Color/style fields based on value | `conditionalFormats` on FieldSlot (schema done) | **Schema done** — needs runtime eval |
| Tab controls | Tabbed container on layout | Not started | Todo |
| Slide panels | Swipeable panel container | Not started | Todo |
| Popovers | Click-to-open floating panels | Not started | Todo |

### P2 — Calculations & Automation

| Gap | FileMaker | Prism | Status |
|-----|-----------|-------|--------|
| Calculation fields | Auto-compute from other fields | ExpressionEngine exists | Todo — needs per-field binding |
| Auto-enter options | Serial number, timestamps, calculated | `createdAt`/`updatedAt` built in | **Partial** — no user-defined |
| Merge fields | `<<FieldName>>` in text objects | `{{fieldName}}` syntax in TextSlot | **Schema done** — needs runtime |
| Script triggers | Per-layout automation hooks | `onRecordLoad/Commit/LayoutEnter/Exit` | **Done** |
| Visual script builder | Drag conditions/actions | Sequencer panel | **Done** |

### P3 — Theming & Print

| Gap | FileMaker | Prism | Status |
|-----|-----------|-------|--------|
| Themes / styles | Built-in themes, custom styles | Not started | Todo |
| Print layout | Page margins, breaks, print header/footer | Not started | Todo |
| PDF export | Export as PDF | Not started | Todo |
| Sliding fields | Collapse when empty | Not started | Todo |
| Field label formatting | Font/size/color per field label | `TextSlot` has styling; FieldSlot needs it | Todo |
| Button bar | Segmented button control | Not started | Todo |
| Web viewer | Embedded browser | Not started | Todo |
| Container field | Inline files/images/PDFs | VfsManager exists | **Partial** |

### P4 — Security & Multi-User

| Gap | FileMaker | Prism | Status |
|-----|-----------|-------|--------|
| Layout privilege sets | Per-layout/field access control | Identity + PeerTrustGraph exist | Todo — needs layout binding |
| CRDT persistence | FacetDefinitions survive restart | Currently in-memory Map | Todo |

---

## Test Coverage

| Area | Tests | Framework |
|------|-------|-----------|
| FacetSchema types + builder | 90 | Vitest |
| Spatial layout pure functions | 45 | Vitest |
| Studio kernel integration | 97 | Vitest |
| Layout builder E2E | 49 | Playwright |
| Spatial canvas E2E | 14 | Playwright |
| **Total new tests** | **30** | — |
| **Total test count** | **2828** | All passing |

---

## Files Added/Modified

### New Files
- `packages/prism-core/src/layer1/facet/spatial-layout.ts` — pure geometry helpers
- `packages/prism-core/src/layer1/facet/spatial-layout.test.ts` — 45 tests
- `packages/prism-studio/src/components/spatial-canvas-renderer.tsx` — free-form canvas
- `packages/prism-studio/src/components/facet-view-renderer.tsx` — FacetDefinition view renderer
- `packages/prism-studio/src/components/data-portal-renderer.tsx` — related records renderer
- `packages/prism-studio/src/panels/spatial-canvas-panel.tsx` — editor lens
- `packages/prism-studio/e2e/spatial-canvas.spec.ts` — 14 Playwright E2E tests

### Modified Files
- `packages/prism-core/src/layer1/facet/facet-schema.ts` — spatial types, TextSlot, DrawingSlot, ConditionalFormat
- `packages/prism-core/src/layer1/facet/facet-schema.test.ts` — extended with spatial + new slot tests
- `packages/prism-core/src/layer1/facet/index.ts` — new exports
- `packages/prism-studio/src/kernel/entities.ts` — 3 new entity types
- `packages/prism-studio/src/panels/layout-panel.tsx` — Puck renderers for new entities
- `packages/prism-studio/src/panels/facet-designer-panel.tsx` — handle text/drawing slots
- `packages/prism-studio/src/lenses/index.tsx` — spatial-canvas lens registration
- `packages/prism-studio/package.json` — react-moveable, react-selecto, @scena/react-guides
