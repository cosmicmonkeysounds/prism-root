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
| Conditional formatting | Color/style fields based on value | `evaluateConditionalFormats()` in facet-runtime.ts | **Done** |
| Tab controls | Tabbed container on layout | Not started | Todo |
| Slide panels | Swipeable panel container | Not started | Todo |
| Popovers | Click-to-open floating panels | Not started | Todo |
| Container field | Inline files/images/PDFs via VFS | `ContainerSlot` type + `addContainer()` builder | **Done** |

### P2 — Calculations & Automation

| Gap | FileMaker | Prism | Status |
|-----|-----------|-------|--------|
| Calculation fields | Auto-compute from other fields | ExpressionEngine exists | Todo — needs per-field binding |
| Auto-enter options | Serial number, timestamps, calculated | `createdAt`/`updatedAt` built in | **Partial** — no user-defined |
| Merge fields | `<<FieldName>>` in text objects | `interpolateMergeFields()` + `renderTextSlot()` | **Done** |
| Script triggers | Per-layout automation hooks | `onRecordLoad/Commit/LayoutEnter/Exit` | **Done** |
| Visual script builder | Drag conditions/actions | VisualScriptPanel (31 steps → Lua) | **Done** |

### P3 — Theming & Print

| Gap | FileMaker | Prism | Status |
|-----|-----------|-------|--------|
| Themes / styles | Built-in themes, custom styles | Not started | Todo |
| Print layout | Page margins, breaks, print header/footer | `PrintConfig` on FacetDefinition | **Done** |
| PDF export | Export as PDF | Not started | Todo — PrintConfig enables this |
| Sliding fields | Collapse when empty | Not started | Todo |
| Field label formatting | Font/size/color per field label | `TextSlot` has styling; FieldSlot needs it | Todo |
| Button bar | Segmented button control | Not started | Todo |
| Web viewer | Embedded browser | Not started | Todo |

### P4 — Security & Multi-User

| Gap | FileMaker | Prism | Status |
|-----|-----------|-------|--------|
| Layout privilege sets | Per-layout/field access control | `PrivilegeSet` in manifest schema | **Done** |
| Field-level security | Per-field read/write/hidden | `FieldPermission` in PrivilegeSet | **Done** |
| Row-level security | Record-level access filtering | `recordFilter` expression in PrivilegeSet | **Done** |
| CRDT persistence | FacetDefinitions survive restart | `FacetStore` with serialize/load | **Done** |

### P5 — Data Model & Views (NEW)

| Gap | FileMaker | Prism | Status |
|-----|-----------|-------|--------|
| Saved views / Found Sets | Named, saveable filtered subsets | `SavedView` type + persistence schema | **Done** |
| Value lists | Static + dynamic dropdown options | `ValueList` registry with relationship sourcing | **Done** |
| Starter templates | Ready-to-use workspace templates | Template system exists | Todo — needs gallery UI |
| Schema designer | Visual entity/relationship editor | Graph panel (view-only) | Todo — needs write mode |
| Script debugger | Step-through with breakpoints | Sequencer + Lua runtime | Todo — needs DAP |

---

## FileMaker → Prism Concept Mapping

| FileMaker Concept | Prism Equivalent | Notes |
|-------------------|------------------|-------|
| Layout | `FacetDefinition` | Visual projection of an entity type |
| Layout Part | `LayoutPart` | header/body/footer/summary bands |
| Field (on layout) | `FieldSlot` | Positioned field reference |
| Portal | `PortalSlot` + `DataPortalRenderer` | Related records inline |
| Container Field | `ContainerSlot` | VFS blob with MIME rendering |
| Value List | `ValueList` | Static or relationship-sourced |
| Found Set | `SavedView` | Named ViewConfig persisted to Loro |
| Privilege Set | `PrivilegeSet` | DID role → permission mapping |
| Script | Lua 5.4 + Automation Engine | Visual builder + code editor |
| Script Workspace | VisualScriptPanel + Sequencer | 31 step types, Lua codegen, palette + live preview |
| Relationship Graph | Graph Panel + `@xyflow/react` | View-only (editor TODO) |
| Theme | Not yet | Todo |
| Print Layout | `PrintConfig` on `FacetDefinition` | Page dims, margins, breaks |
| Table | Collection | Loro CRDT array |
| Record | `GraphObject` | Core data unit |
| Field (schema) | `EntityFieldDef` | Registry-driven field defs |

---

## Test Coverage

| Area | Tests | Framework |
|------|-------|-----------|
| FacetSchema types + builder | 103 | Vitest |
| Spatial layout pure functions | 45 | Vitest |
| SavedView schema | 28 | Vitest |
| ValueList schema | 25 | Vitest |
| ContainerSlot schema | 10+ | Vitest |
| PrivilegeSet schema | 21 | Vitest |
| PrintConfig schema | 10+ | Vitest |
| Facet runtime (conditional fmt, merge fields) | 24 | Vitest |
| Script steps (31 kinds, Lua codegen) | 37 | Vitest |
| FacetStore (CRDT persistence) | 14 | Vitest |
| PrivilegeEnforcer (row/field security) | 16 | Vitest |
| Studio kernel integration | 97 | Vitest |
| Layout builder E2E | 49 | Playwright |
| Spatial canvas E2E | 14 | Playwright |

---

## Files Added/Modified

### New Files (Phase 3 — Runtime + Studio UI)
- `packages/prism-core/src/layer1/manifest/privilege-enforcer.ts` — PrivilegeSet runtime enforcement
- `packages/prism-core/src/layer1/manifest/privilege-enforcer.test.ts` — 16 tests
- `packages/prism-core/src/layer1/facet/facet-runtime.ts` — Conditional formatting, merge fields, value list resolver
- `packages/prism-core/src/layer1/facet/facet-runtime.test.ts` — 24 tests
- `packages/prism-core/src/layer1/facet/script-steps.ts` — 31 visual script step types + Lua codegen
- `packages/prism-core/src/layer1/facet/script-steps.test.ts` — 37 tests
- `packages/prism-core/src/layer1/facet/facet-store.ts` — Persistent FacetDefinition + Script + ValueList registry
- `packages/prism-core/src/layer1/facet/facet-store.test.ts` — 14 tests
- `packages/prism-studio/src/components/container-field-renderer.tsx` — MIME-aware file preview component
- `packages/prism-studio/src/panels/visual-script-panel.tsx` — Visual script editor (lens #24, Shift+S)
- `packages/prism-studio/src/panels/saved-view-panel.tsx` — Saved view manager (lens #25, Shift+V)
- `packages/prism-studio/src/panels/value-list-panel.tsx` — Value list editor (lens #26, Shift+L)
- `packages/prism-studio/src/panels/privilege-set-panel.tsx` — Privilege set manager (lens #27, Shift+P)

### New Files (Phase 2 — Core Schemas)
- `packages/prism-core/src/layer1/view/saved-view.ts` — SavedView types + CRUD
- `packages/prism-core/src/layer1/view/saved-view.test.ts` — SavedView tests
- `packages/prism-core/src/layer1/facet/value-list.ts` — ValueList types + resolution
- `packages/prism-core/src/layer1/facet/value-list.test.ts` — ValueList tests
- `packages/prism-core/src/layer1/manifest/privilege-set.ts` — PrivilegeSet types
- `packages/prism-core/src/layer1/manifest/privilege-set.test.ts` — PrivilegeSet tests

### New Files (Phase 1 — Layout)
- `packages/prism-core/src/layer1/facet/spatial-layout.ts` — pure geometry helpers
- `packages/prism-core/src/layer1/facet/spatial-layout.test.ts` — 45 tests
- `packages/prism-studio/src/components/spatial-canvas-renderer.tsx` — free-form canvas
- `packages/prism-studio/src/components/facet-view-renderer.tsx` — FacetDefinition view renderer
- `packages/prism-studio/src/components/data-portal-renderer.tsx` — related records renderer
- `packages/prism-studio/src/panels/spatial-canvas-panel.tsx` — editor lens
- `packages/prism-studio/e2e/spatial-canvas.spec.ts` — 14 Playwright E2E tests

### Modified Files
- `packages/prism-core/src/layer1/facet/facet-schema.ts` — ContainerSlot, PrintConfig
- `packages/prism-core/src/layer1/facet/facet-schema.test.ts` — ContainerSlot + PrintConfig tests
- `packages/prism-core/src/layer1/facet/index.ts` — new exports
- `packages/prism-core/src/layer1/view/index.ts` — SavedView exports
- `packages/prism-core/src/layer1/manifest/manifest-types.ts` — PrivilegeSet on PrismManifest
