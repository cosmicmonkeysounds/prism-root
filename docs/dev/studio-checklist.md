# Prism Studio — App Builder Checklist

Goal: Studio builds and modifies Prism apps (SquareSpace / FileMaker Pro).

## Current State (~40%)

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
- [ ] **1B. Editor ↔ Selected Object** — CodeMirror edits `text-block.data.content` of selected object (not global buffer)
- [x] **1C. Graph Panel Live Data** — Graph reacts to all kernel mutations (subscribes to store.onChange)

## Tier 2 — Core Page Builder UX

- [x] **2A. Drag-Drop in Explorer** — Reorder (position) + reparent via drag-drop in object tree with containment validation
- [x] **2B. Component Palette** — Sidebar panel listing available block types from registry, click-to-add + draggable
- [x] **2C. Canvas Preview** — WYSIWYG preview rendering page objects as React components
- [x] **2D. Block Toolbar** — Floating toolbar on selected block: move up/down, duplicate, delete
- [x] **2E. Quick-Create Combobox** — Inline add child of allowed types at bottom of sections/pages

## Tier 3 — Styling & Theming

- [ ] **3A. Style Properties** — Per-block CSS: background, padding, margin, border, border-radius, shadow
- [ ] **3B. Typography Controls** — Font family, size, weight, color, line-height, letter-spacing
- [ ] **3C. Layout Controls** — Flex/grid settings: direction, gap, align, justify, wrap
- [ ] **3D. Responsive Breakpoints** — Mobile/tablet/desktop overrides stored in block data
- [ ] **3E. Design Tokens** — CSS variables for colors, spacing, fonts; theme picker

## Tier 4 — Data & Expressions

- [ ] **4A. Expression Fields** — Wire `@prism/core/expression` evaluator into inspector for computed fields
- [ ] **4B. Data Binding UI** — Field input that references other objects: `[obj:pageTitle]`
- [ ] **4C. Syntax Completions** — Wire `@prism/core/syntax` for expression autocomplete in inspector
- [ ] **4D. Conditional Visibility** — Show/hide blocks based on expressions (legacy FormSchema.conditional pattern)

## Tier 5 — Templates & Scaffolding

- [ ] **5A. Page Templates** — Predefined page layouts: landing, blog post, dashboard, form
- [ ] **5B. Section Templates** — Hero, feature grid, testimonials, pricing table, CTA, footer
- [ ] **5C. Template Picker UI** — "New from Template" dialog with preview thumbnails
- [ ] **5D. Save as Template** — Right-click object → save subtree as reusable template (createFromObject)

## Tier 6 — Publishing & Export

- [ ] **6A. HTML Export** — Render page object tree to static HTML + inline CSS
- [ ] **6B. JSON Export** — Serialize page structure for sharing/backup
- [ ] **6C. Publish Workflow** — Draft → Review → Published state machine per page
- [ ] **6D. Preview Mode** — Read-only rendered view of the page (toggle edit ↔ preview)

## Tier 7 — Rich Content

- [ ] **7A. Rich Text Blocks** — TipTap/ProseMirror for WYSIWYG text editing in blocks
- [ ] **7B. Markdown Blocks** — Render markdown content with wiki-link support
- [ ] **7C. Code Blocks** — CodeMirror for inline code snippets with syntax highlighting
- [ ] **7D. Media Blocks** — Image upload via VFS, video embed, audio player

## Tier 8 — Advanced Features

- [ ] **8A. Automation Rules** — Wire `@prism/core/automation` for event-driven actions (on publish → notify)
- [ ] **8B. Form Builder** — Wire `@prism/core/forms` FormState + validation for user-facing forms
- [ ] **8C. Plugin System** — Wire `@prism/core/plugin` for third-party block types + views
- [ ] **8D. Multi-Page Navigation** — Site structure editor, menu builder, breadcrumbs
- [ ] **8E. Collaboration** — Wire `@prism/core/presence` for peer cursors + selection awareness

## Tier 9 — FileMaker Pro Features

- [ ] **9A. Custom Entity Types** — UI for defining new entity schemas (not just page blocks)
- [ ] **9B. Relationship Builder** — Visual edge type definition between entity types
- [ ] **9C. View Modes** — Grid/list/kanban/timeline views from `@prism/core/view` ViewRegistry
- [ ] **9D. Computed Fields** — Expression-based derived fields on entities
- [ ] **9E. Import/Export** — CSV/JSON import with field mapping, bulk operations
- [ ] **9F. REST API Generation** — Wire `@prism/core/server` to auto-generate API from registry
- [ ] **9G. Dashboard Builder** — Aggregate views, charts, KPI cards from LiveView data

---

## Priority Order

**Ship a usable page builder (Tiers 0-2):** ~70% of SquareSpace core.
Wire existing systems, connect panels, add drag-drop + palette + preview.

**Ship styling + templates (Tiers 3+5):** ~85% of SquareSpace.
Design tokens, responsive, reusable templates.

**Ship data features (Tiers 4+6):** ~90% of SquareSpace + FileMaker basics.
Expressions, publish workflow, export.

**Ship advanced (Tiers 7-9):** Full FileMaker Pro competitor.
Rich content, automation, custom schemas, API generation.
