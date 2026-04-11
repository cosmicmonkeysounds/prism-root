# Prism Studio — App Builder Checklist

Goal: Studio builds and modifies Prism apps (SquareSpace / FileMaker Pro).

## Current State (~100%)

All tiers complete. 3513 unit tests pass. Every feature has a registered lens,
a `data-testid` hook, and Playwright E2E coverage in `e2e/`.

---

## Legacy Snapshot (~40%) — kept for reference

**Working:** Object CRUD with undo, entity registry with 8 types + containment,
schema-driven inspector, object explorer tree with drag-drop reorder/reparent,
notifications, CRDT persistence, bus → atom reactivity, tab/lens system,
live-reactive graph visualization, CodeMirror + Puck panels (Puck wired to kernel),
canvas with block toolbar + quick-create, component palette, search, clipboard,
batch ops, activity log, templates, LiveView.

**Not working:** Editor edits global buffer not objects, no export, no data binding,
no styling system, no responsive breakpoints, no design tokens.

---

## Tier 0 — Wire Existing Layer 1 Systems Into Kernel

These systems exist in @prism/core and just need kernel integration.

- [x] **0A. Search** — Wire `createSearchEngine` into kernel, index objects on create/update, expose `kernel.search`
- [x] **0B. Clipboard** — Wire `createTreeClipboard` into kernel, expose `kernel.clipboard` (cut/copy/paste with deep clone + ID remap)
- [x] **0C. Batch Operations** — Wire `createBatchTransaction` into kernel for atomic multi-op + single undo entry
- [x] **0D. Activity Log** — Wire `createActivityStore` + `createActivityTracker` into kernel for audit trail
- [x] **0E. Templates** — Wire `createTemplateRegistry` into kernel, register page builder templates (hero, 3-col, etc.)
- [x] **0F. LiveView** — Wire `createLiveView` into kernel for filtered/sorted/grouped data projections

## Tier 1 — Connect Existing Panels to Kernel

- [x] **1A. Puck ↔ Kernel Bridge** — Bidirectional sync: page object tree → Puck data, Puck edits → kernel CRUD
- [x] **1B. Editor ↔ Selected Object** — `editor-panel.tsx` maps `text-block.content` / `heading.text` of the selected object into per-object `obj_content_{id}` LoroText buffers, falls back to the shared scratch doc otherwise.
- [x] **1C. Graph Panel Live Data** — Graph reacts to all kernel mutations (subscribes to store.onChange)

## Tier 2 — Core Page Builder UX

- [x] **2A. Drag-Drop in Explorer** — Reorder (position) + reparent via drag-drop in object tree with containment validation
- [x] **2B. Component Palette** — Sidebar panel listing available block types from registry, click-to-add + draggable
- [x] **2C. Canvas Preview** — WYSIWYG preview rendering page objects as React components
- [x] **2D. Block Toolbar** — Floating toolbar on selected block: move up/down, duplicate, delete
- [x] **2E. Quick-Create Combobox** — Inline add child of allowed types at bottom of sections/pages

## Tier 3 — Styling & Theming

- [x] **3A. Style Properties** — Per-block CSS: background, padding (X/Y), margin (X/Y), border (width/color/radius), shadow (preset or raw). `STYLE_FIELD_DEFS` in `kernel/block-style.ts` spread into every core block def.
- [x] **3B. Typography Controls** — Font family, size, weight, line-height, letter-spacing, text-align — same `STYLE_FIELD_DEFS` Typography group.
- [x] **3C. Layout Controls** — Flex fields (display/flexDirection/gap/alignItems/justifyContent) exposed via `BlockStyleData`; `columns` widget provides the higher-level 1-6 col grid.
- [x] **3D. Responsive Breakpoints** — Mobile/tablet/desktop overrides stored in block data, rendered through `kernel/block-style.ts`.
- [x] **3E. Design Tokens** — `design-tokens-panel.tsx` + `kernel/design-tokens.ts` expose CSS variables for colors, spacing, fonts; theme picker lens (`Shift+T`).

## Tier 4 — Data & Expressions

- [x] **4A. Expression Fields** — `ComputedFieldDisplay` in `inspector-panel.tsx` evaluates `EntityFieldDef.expression` live via `@prism/core/expression`.
- [x] **4B. Data Binding UI** — `kernel/data-binding.ts` `resolveObjectRefs()` resolves `[obj:pageTitle]` tokens in canvas text nodes (live-reactive).
- [x] **4C. Syntax Completions** — Inspector Expression Bar feeds a `createSyntaxEngine()` completion dropdown with Tab/Enter/Escape keyboard nav.
- [x] **4D. Conditional Visibility** — `kernel/data-binding.ts` `evaluateVisibleWhen()` hides canvas blocks whose `data.visibleWhen` expression is falsy.

## Tier 5 — Templates & Scaffolding

- [x] **5A. Page Templates** — Seeded in `App.tsx` `registerSeedTemplates()` and surfaced through the Template Gallery.
- [x] **5B. Section Templates** — `kernel/section-templates.ts` registers hero/feature grid/testimonial/pricing/CTA/footer blueprints.
- [x] **5C. Template Picker UI** — `TemplateGallery` overlay inside `object-explorer.tsx` ("New from Template").
- [x] **5D. Save as Template** — `studio-kernel.templateFromObject()` + Inspector "Save as Template" button round-trips any subtree.

## Tier 6 — Publishing & Export

- [x] **6A. HTML Export** — `kernel/page-export.ts` `exportPageToHtml()` renders deterministic, dependency-free HTML.
- [x] **6B. JSON Export** — `kernel/page-export.ts` `exportPageToJson()` produces a `prism-page/v1` snapshot.
- [x] **6C. Publish Workflow** — `publish-panel.tsx` runs the `draft → review → published` state machine per page.
- [x] **6D. Preview Mode** — `publish-panel.tsx` renders a read-only preview alongside HTML/JSON export actions.

## Tier 7 — Rich Content

- [x] **7A. Rich Text Blocks** — `editor-panel.tsx` exposes a Markdown toolbar (bold/italic/heading/list/link) backed by the pure `computeMarkdownEdit()` helper.
- [x] **7B. Markdown Blocks** — `components/content-renderers.tsx` `MarkdownWidgetRenderer` renders dependency-free markdown with escaping.
- [x] **7C. Code Blocks** — `editor-panel.tsx` maps `code-block` / `luau-block` `source` into CodeMirror; static rendering via `components/code-block-renderer.tsx`.
- [x] **7D. Media Blocks** — `assets-panel.tsx` `handleImportBinary()` imports via VFS and auto-creates `image` blocks; `components/media-renderers.tsx` renders video/audio with an HTTPS allowlist.

## Tier 8 — Advanced Features

- [x] **8A. Automation Rules** — `automation-panel.tsx` drives `@prism/core/automation` (create/toggle/run with history).
- [x] **8B. Form Builder** — `form-builder-panel.tsx` composes form-input entities under a section/page container.
- [x] **8C. Plugin System** — `plugin-panel.tsx` registers/removes plugins through `@prism/core/plugin`.
- [x] **8D. Multi-Page Navigation** — `site-nav-panel.tsx` + `siteNavDef`/`breadcrumbsDef` in `kernel/entities.ts`; `buildSiteNav()` is exported for canvas/widget consumption.
- [x] **8E. Collaboration** — `components/peer-cursors-overlay.tsx` renders `PeerCursorsBar` + `PeerSelectionBadge` from `kernel.presence` inside the canvas.

## Tier 9 — FileMaker Pro Features

- [x] **9A. Custom Entity Types** — `entity-builder-panel.tsx` lets authors register new `EntityDef`s into `kernel.registry` at runtime.
- [x] **9B. Relationship Builder** — `relationship-builder-panel.tsx` registers `EdgeTypeDef`s (behavior/color/source-target type restrictions).
- [x] **9C. View Modes** — Every "view" (list/table/card-grid/kanban/report) is now a composable Puck widget wired into `layout-panel.tsx` and `canvas-panel.tsx` via dedicated renderers in `components/` (list-widget-renderer, table-widget-renderer, card-grid-widget-renderer, report-widget-renderer). Object Explorer is a pure navigation tree; the old view-mode switcher and `record-browser-panel.tsx` were removed.
- [x] **9D. Computed Fields** — `EntityFieldDef.expression` runs through `ComputedFieldDisplay` in `inspector-panel.tsx`.
- [x] **9E. Import/Export** — `import-panel.tsx` handles CSV/JSON import with field mapping; `page-export.ts` covers export.
- [x] **9F. REST API Generation** — Studio itself is client-only; API generation lives in `@prism/core/server` and is exposed by Relay (see `packages/prism-relay/src/routes/autorest-routes.ts`). Studio integration is therefore scoped to Relay.
- [x] **9G. Dashboard Builder** — Chart/Stat widgets (`components/chart-widget-renderer.tsx`, `data-display-renderers.tsx`) compose with LiveView directly in `canvas-panel.tsx` — no dedicated panel required.

---

## Verification

- `pnpm --filter @prism/studio typecheck` — clean
- `pnpm -w run test` — 3513 tests across 174 files (as of 2026-04-08)
- `pnpm --filter @prism/studio test:e2e` — Playwright covers every lens
  (`e2e/new-panels.spec.ts` adds design-tokens, form-builder, site-nav,
  entity-builder, relationship-builder, publish, peer-cursors-bar)

## Priority Order (historical)

**Ship a usable page builder (Tiers 0-2):** ~70% of SquareSpace core.
Wire existing systems, connect panels, add drag-drop + palette + preview.

**Ship styling + templates (Tiers 3+5):** ~85% of SquareSpace.
Design tokens, responsive, reusable templates.

**Ship data features (Tiers 4+6):** ~90% of SquareSpace + FileMaker basics.
Expressions, publish workflow, export.

**Ship advanced (Tiers 7-9):** Full FileMaker Pro competitor.
Rich content, automation, custom schemas, API generation.
