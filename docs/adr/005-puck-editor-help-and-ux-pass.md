# ADR-005: Puck editor help system and UX pass

**Status**: Accepted
**Date**: 2026-04-13

## Context

The Puck visual builder (`LayoutPanel`, `packages/prism-studio/src/panels/layout-panel.tsx`) is the primary authoring surface in Studio. It exposes a component palette, a WYSIWYG canvas, and a field inspector over 50+ entity types — pages, sections, shells, dynamic record widgets, forms, layout primitives, media, and escape hatches like `luau-block` and `spatial-canvas`. Today there is **no user-facing explanation** for any of it:

- Category titles (`Layout`, `Content`, `DataViews`, …) are labels, not descriptions.
- Component palette items show the type name and no hint of what the component does, what fields it accepts, or when to reach for it.
- Field panel labels are verbatim from the entity def — `visibleWhen`, `customCss`, `classNamePuck`, `backgroundColor` — with no guidance on syntax, valid values, or interaction with other fields.
- Container concepts (`page-shell`, `app-shell`, `site-nav`, `data-portal`, `record-list`, `facet-view`) have no place to surface the mental model; the only documentation is in CLAUDE.md files a page author never reads.
- There is no entry point from the editor into existing docs (`docs/adr/`, `packages/*/CLAUDE.md`, `docs/dev/current-plan.md`).

The legacy Helm codebase (`$legacy-inspiration-only/helm/components/src/help/`) solved this with a small, self-contained help system:

- `HelpRegistry` — a global `Map<id, HelpEntry>` with `register` / `registerMany` / `get` / `getAll` / `search`.
- `HelpEntry` — `{ id, title, icon?, summary, docPath?, docAnchor? }`. Summary is plain text for hover; full docs live in separate markdown files.
- `HelpTooltip` — portal-rendered hover popover with 380 ms show delay, singleton (only one visible at a time), dismisses on Escape / scroll / mouseleave. Optional "View full docs" button in the footer.
- `DocSheet` — slide-in 640 px markdown panel with anchor scroll, driven by a caller-provided `fetchDoc(path) => Promise<string>`.
- `DocSearch` — search input + result list over `HelpRegistry.search()`.
- `HelpProvider` + `useHelp()` — React context that decouples `openDoc` from any specific backend.

Packages register entries as import side-effects (`HelpRegistry.registerMany([...])`) so there is zero runtime setup and entries live next to the UI they describe.

Separately, while auditing the Puck editor for this UX pass we found a latent bug in `useResizeHandle` at `packages/prism-studio/src/components/layout-shell-renderers.tsx:81-140`:

```tsx
useEffect(() => {
  if (!dragging) return;
  const handleMove = (e: PointerEvent) => {
    // reads startRef, setValue(clampBar(...))    ← setValue on every move
  };
  const handleUp = () => { /* setDragging(false); onCommit?.(final) */ };
  window.addEventListener("pointermove", handleMove);
  window.addEventListener("pointerup", handleUp);
  window.addEventListener("pointercancel", handleUp);
  return () => { /* remove listeners */ };
}, [dragging, axis, direction, onCommit, value]); // ← value in deps
```

Because `value` (and `onCommit`) live in the dependency array, every `setValue` inside `handleMove` causes React to tear down the effect, remove the window listeners, and re-attach fresh ones. When the user releases the pointer, the release lands in a window where either (a) the old listener has been removed and the new one has not yet attached, or (b) `dragging` has been stale-closured by the cleanup path. Either way, `pointerup` can be missed, `setDragging(false)` never runs, and the resize handle "sticks to the cursor" until the user clicks elsewhere. The symptom is a resize handle that follows mouse movement after release. This affects `PageShellRenderer`'s four bars and `SideBarRenderer`.

Both problems are in scope for the same ADR because they share the same target surface (Puck editor UX).

## Decision

Three phases, delivered together.

### Phase A — Fix `useResizeHandle` commit-on-release

Refactor `useResizeHandle` so the window listener effect depends **only** on `[dragging]`. All mutable state (`axis`, `direction`, `onCommit`, the current `value`) moves to refs that the listeners read at dispatch time. This makes the listener-attach path run exactly twice per drag — once on `pointerdown` (when `dragging` flips true) and once on `pointerup` (when it flips false). No mid-drag tear-down. `pointerup` and `pointercancel` always land on the listener that was attached at drag start.

Keeps `setPointerCapture(pointerId)` on the handle element for robustness against layout shifts during drag, and keeps the `pointercancel` fallback so OS-level gesture interruptions (browser back-swipe, modal focus change) still clear `dragging`.

Add a regression test in `layout-shell-renderers.test.ts` that mounts a `PageShellRenderer`, dispatches `pointerdown` → `pointermove` → `pointerup` at the window level, and asserts (a) `onCommit` fires with the expected value, (b) the `aria-orientation="vertical"` resize handle's `active` state is false after release.

### Phase B — Port help system to `@prism/core/help`

Port the legacy `HelpRegistry` / `HelpEntry` / `HelpTooltip` / `HelpProvider` / `DocSheet` / `DocSearch` into `packages/prism-core/src/bindings/react-shell/help/`, exposed via a new `"./help"` subpath export (`@prism/core/help`). Files:

- `types.ts` — `HelpEntry` interface. Matches legacy shape (`id`, `title`, `icon?`, `summary`, `docPath?`, `docAnchor?`), except `icon` is now `ReactNode` rather than a `ComponentType<{size,className}>` to avoid pinning an icon library.
- `help-registry.ts` — `HelpRegistry` singleton with `register`, `registerMany`, `get`, `getAll`, `search`, `clear`. Identical surface to legacy; carry over the substring AND search. Registry is a module-scoped `Map<string, HelpEntry>`.
- `help-context.tsx` — `HelpProvider` + `useHelp()`. Context value is `{ openDoc(path, anchor?), searchEntries(query) }`. `searchEntries` delegates to the registry.
- `help-tooltip.tsx` — `HelpTooltip` component. Portal-rendered via `createPortal(..., document.body)`. 380 ms show delay, 120 ms hide delay, singleton-dismiss on show (module-level dismiss callback map). Dismisses on Escape and capture-phase scroll. Optional leading `icon` from `HelpEntry`. Renders a trailing "View full docs" button when the entry has a `docPath`, which calls `useHelp().openDoc()`. Width 276 px, fixed positioning with viewport-edge flipping.
- `doc-sheet.tsx` — `DocSheet` slide-in panel. Fixed-position right-aligned 640 px panel with a close button, backdrop, and scrollable body. Takes a caller-provided `fetchDoc(path) => Promise<string>`. **Uses `parseMarkdown` from `@prism/core/forms`** to tokenize fetched markdown, then renders BlockToken[] → React nodes directly (headings, paragraphs, code, lists, quotes, horizontal rules, task items, wiki-links). Inline tokens flow through `parseInline` from the same module. Anchor scroll on `doc-anchor` attribute or heading `id` slug after content loads.
- `doc-search.tsx` — `DocSearch` input + result list. Calls `HelpRegistry.search()` on every query change. Clicking a result with a `docPath` calls `useHelp().openDoc()`.
- `index.ts` — barrel export.
- `help-registry.test.ts` — unit tests for `register`/`registerMany`/`get`/`search`/`clear`.
- `help-markdown.test.ts` — unit tests for the BlockToken → React rendering helper.

Add to `packages/prism-core/package.json` exports: `"./help": "./src/bindings/react-shell/help/index.ts"`.

Rationale for the choices that diverge from the legacy port:

- **Icons.** Legacy Helm imported from `lucide-react`. No Prism package currently depends on `lucide-react`, and the help module should not add one. Icons become an optional `ReactNode` on `HelpEntry`, and the tooltip/doc-sheet ship with inline SVG chrome (chevron, X, search, bookmark). Individual entries pass whatever icon element they want.
- **Markdown parsing.** Legacy used a `MarkdownViewer` component that shells out to its own parser. Prism has `parseMarkdown` and `parseInline` in `@prism/core/forms`, which ADR-002 established as the *single* markdown tokenizer — `@prism/core/markdown`'s LanguageContribution already reuses them. The help module MUST use the same tokenizer instead of bundling a third copy. This closes the loop on the user's feedback memory ("all external-format parsers must be built on @prism/core/syntax's Scanner, never hand-rolled regex/string indexing") — help markdown now flows through the canonical tokenizer rather than a private regex.
- **SlidingPane.** Legacy used a `SlidingPane` primitive. Prism has no equivalent and building a generic one is out of scope — `DocSheet` implements its own fixed-position right-side panel with backdrop.
- **`cn` utility.** Legacy used `cn` from `@helm/components/lib/cn`. Prism has no equivalent. The help components inline className strings.

### Phase C — Wire help into the Puck editor

Three things to wire:

**1. `HelpProvider` + `DocSheet` mount at the `LayoutPanel` root.**

`LayoutPanel` wraps its render tree in `<HelpProvider onOpenDoc={handleOpenDoc}>`. A local `useState` tracks the currently-open `{docPath, anchor?}` and renders `<DocSheet>` alongside `<Puck>`. The `fetchDoc` implementation reads from a bundled markdown map baked at build time by Vite's `?raw` imports so the sheet works offline (Studio is a Vite SPA + Tauri desktop, not a server app). A small resolver maps logical doc IDs (`puck.categories.layout`, `puck.components.record-list`) to raw markdown strings.

**2. `puck-help-entries.ts` authored in `packages/prism-studio/src/panels/`.**

A single file with flagship-seeded entries for:

- **Categories** (8 entries): `puck.categories.layout`, `.content`, `.media`, `.dataviews`, `.forms`, `.navigation`, `.display`, `.dynamic`.
- **Components** (~50 entries, one per entity type registered in layout-panel.tsx). For the initial land, twelve entries get full summaries (page-shell, app-shell, site-header, site-footer, section, columns, heading, image, facet-view, luau-block, record-list, spatial-canvas); the remaining components get stub summaries with a `TODO(docs):` prefix so it is obvious which need writing. This captures the schema and gets the UI wired without blocking on content writing.
- **Style field groups** (6 entries): `puck.style.colors`, `.spacing`, `.typography`, `.layout`, `.responsive`, `.escape-hatches`.
- **Editor regions** (4 entries): `puck.region.sidebar`, `.canvas`, `.field-panel`, `.publish`.

The file calls `HelpRegistry.registerMany([...])` at module top level; `layout-panel.tsx` imports it once for the side effect.

**3. `HelpTooltip` wrapped around UI surfaces.**

To avoid forking Puck or reimplementing its sidebar, Phase C introduces a single visible entry point plus two surgical hooks:

- **`?` help button in the layout-panel toolbar.** Renders a round button with a "?" SVG. Clicking it opens a floating `DocSearch` panel anchored below the button. This is the primary discovery affordance and works regardless of whether Puck's sidebar exposes per-item slots.
- **Category and component help via `overrides`.** Puck exposes `overrides` on `<Puck>` for customising `componentItem`, `fieldLabel`, etc. `LayoutPanel` registers override implementations that look up a `helpId` derived from the Puck component key (PascalCase → kebab-case entity type → `puck.components.<type>`) and wrap the existing rendering in a `HelpTooltip`. Field labels do the same lookup against `puck.fields.<name>` with a fallback through the four style field groups.
- **Inline `helpId` prop on wrapped Puck ComponentConfigs.** When the `PuckComponentProvider` (ADR-004) registers a provider, it may return a `helpId: string` alongside the Puck `ComponentConfig`. The provider's `buildConfig` returns `{ config, helpId }` and the registry stores both. When the Puck `componentItem` override renders, it reads the stored `helpId` and wraps in `HelpTooltip`.

A thirteenth set of help entries describes the help system itself (meta-docs), so the `?` button's own search results surface "How to use this editor" at the top.

### Content strategy

Twelve flagship components receive complete 2–3 sentence summaries and linked full docs at land time. The remaining component, category, style-field-group, and region entries get stub summaries structured like:

```
"TODO(docs): describe the <record-list> component — what it queries, how filter expression works, when to use it vs list-widget."
```

The structure is intentional: ADR-005 is both a technical framework AND a content placeholder index. Subsequent PRs fill summaries and write full docs without any more code changes. Full docs are markdown files under `packages/prism-studio/docs/help/` bundled via `?raw` imports.

### Out of scope

- First-run coachmarks, spotlight tours, guided onboarding.
- Help content for non-Puck surfaces (object explorer, inspector, other lenses). The help module is exported from `@prism/core/help` so any lens can adopt it, but this ADR only wires the Puck editor.
- Moving field factories to `@prism/core` (that was ADR-004's deferred follow-up).
- A user-facing help editor for non-developers to contribute entries.
- Full translations — summaries ship in English only.

## Rationale

**Why port instead of building fresh.** The legacy design is small (~500 LOC excluding tests), battle-tested in a production workspace, and shaped around the exact authoring pattern we want (side-effect registration per package, opt-in tooltip wrapping, pluggable `fetchDoc`). Rolling our own tooltip + registry + doc sheet would cost more and miss the observed UX details (380 ms show delay, singleton dismiss, anchor scroll, capture-phase scroll dismiss).

**Why `@prism/core/help` instead of `@prism/studio`.** Three other lenses (Admin, Facet Designer, Spatial Canvas) and the `@prism/admin-kit` package will want the same tooltip surface. Putting the module in `@prism/core/bindings/react-shell/help/` keeps Studio as a consumer and allows any React-mounted surface in the monorepo to register entries and wrap UI elements. The `bindings/react-shell/` subtree is where `@prism/core/shell` already lives; help is additive to that category.

**Why reuse `parseMarkdown` from forms.** ADR-002 Phase 3 established `@prism/core/forms`'s `parseMarkdown`/`parseInline` as the canonical markdown tokenizer, reused by `@prism/core/markdown`'s `LanguageContribution`. The Studio `renderMarkdown` in `content-renderers.tsx` predates that consolidation and is a local duplicate. The help module picks up the canonical tokenizer to respect the user's feedback memory ("all external-format parsers must be built on @prism/core/syntax's Scanner, never hand-rolled regex/string indexing") and eliminates drift between help markdown and every other markdown surface in Prism. A small `BlockToken[] → ReactNode` renderer in the help module is the only new code.

**Why bundled markdown instead of HTTP fetch.** Studio is a Vite SPA that runs as a Tauri desktop app and a Capacitor mobile wrapper. There is no guaranteed HTTP server at runtime. Vite's `?raw` import emits markdown as plain strings at build time, which works offline, ships to mobile, and makes the doc map statically analysable (a missing doc fails at build, not at hover).

**Why keep the `useResizeHandle` fix in the same ADR.** The resize bug was found while auditing the Puck editor for this UX pass. It is strictly scoped to the same surface, ships with the same PR, and rolling it into ADR-005 keeps the fix discoverable next to the rest of the Puck editor improvements. A standalone ADR would be noise.

**Why provider `helpId` instead of data-attributes.** Puck's internals walk `ComponentConfig.fields` to render the inspector. A `helpId` stored alongside the provider config is a clean, typed extension that does not require mutating Puck rendering — it surfaces through our own `overrides`. Data attributes on DOM elements would force the tooltip to observe via `MutationObserver` and couple to Puck's internal markup, which is fragile.

## Consequences

### What changes

- **New module.** `packages/prism-core/src/bindings/react-shell/help/` with types, registry, context, tooltip, doc-sheet, search, tests.
- **New export.** `@prism/core/help` subpath in `packages/prism-core/package.json`.
- **Fix.** `useResizeHandle` in `packages/prism-studio/src/components/layout-shell-renderers.tsx` moves mutable state to refs and depends only on `[dragging]`. Regression test added.
- **New file.** `packages/prism-studio/src/panels/puck-help-entries.ts` with flagship-seeded entries and TODO stubs. Imported for side effect from `layout-panel.tsx`.
- **Wired.** `LayoutPanel` mounts `HelpProvider` + `DocSheet` + a `?` toolbar button with inline `DocSearch`. Puck `overrides` wrap `componentItem` and `fieldLabel` in `HelpTooltip` where a matching entry exists.
- **New dir.** `packages/prism-studio/docs/help/` with the twelve flagship markdown files, imported via `?raw`.
- **Docs.** `packages/prism-core/CLAUDE.md` gains a bullet under `bindings/react-shell/` for the help module. `packages/prism-studio/CLAUDE.md` gains a bullet under `panels/` for the help wiring.

### What does not change

- `PuckComponentRegistry` (ADR-004) API stays the same. Providers may optionally attach a `helpId` to the returned config but are not required to.
- The 50 existing hand-wired special cases in `layout-panel.tsx` stay. Each picks up help via its derived `helpId` (PascalCase → kebab-case → `puck.components.<type>`) whether or not it has been migrated to a provider yet.
- `@prism/studio`'s `renderMarkdown` in `content-renderers.tsx` stays — it's still the markdown-widget's Puck preview renderer. A future PR may collapse it onto `parseMarkdown` from forms, but that is out of scope here.
- Lucide-react is not added as a dependency. Icons in the help UI are inline SVGs.

### Follow-ups

- Fill in the remaining ~40 TODO stub summaries and write the full docs for the remaining components.
- Adopt the help system in other lenses (Admin, Facet Designer, Spatial Canvas, Inspector).
- Consider a "help content" command palette entry that opens `DocSearch` from anywhere in Studio, not just the layout panel.
- Replace `content-renderers.tsx`'s private `renderMarkdown` with a call into the help module's BlockToken renderer (or promote the renderer to `@prism/core/markdown` if it's useful there).
- If any non-Vite consumer wants to use `DocSheet`, wire an HTTP `fetchDoc` alongside the bundled one.
