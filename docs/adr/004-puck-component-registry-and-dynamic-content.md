# ADR-004: Puck component registry and dynamic-content primitives

**Status**: Proposed
**Date**: 2026-04-13

## Context

The Puck visual builder in `@prism/studio` exposes components to page authors
two ways:

1. **Auto-generated from `ObjectRegistry`** — `entityToPuckComponent()` in
   `packages/prism-studio/src/panels/layout-panel.tsx` walks every `EntityDef`
   with category `component` or `section` and produces a `ComponentConfig`
   from its field list. Universal block-style fields are folded in via
   `attachStyleFieldsInPlace()`. This works for anything whose authoring UX
   is "a list of fields → a preview div".

2. **Hand-wired special cases** — roughly fifty `if (def.type === "xxx")`
   blocks inside the same function that bypass the generator and wire
   fields + a bespoke React render by hand: `luau-block`, `facet-view`,
   `spatial-canvas`, `data-portal`, `kanban-widget`, `list-widget`,
   `table-widget`, `card-grid-widget`, `report-widget`, `calendar-widget`,
   `chart-widget`, `map-widget`, all ten dynamic record widgets, tab/popover/
   slide containers, form inputs, columns/divider/spacer, stat/badge/alert/
   progress, markdown/iframe, code-block, media, image, button, card. The
   `LayoutPanel` `useMemo` that builds the Puck `Config` is 2100+ lines.

Every one of the ten dynamic record widgets (tasks, reminders, contacts,
events, notes, goals, habits, bookmarks, timer sessions, captures) is a
near-identical shape: `kernel.store.allObjects()` → filter by record type →
sort → render rows. The differences are cosmetic (row template, date chip
style, priority color) and filter shape (task status, event range, note
tag). Each one owns a 100-line `*-widget-renderer` file plus a 60-line
`if (def.type === "…-widget")` block in `layout-panel.tsx`, plus entity
fields in `entities.ts`. That is ~2000 lines of near-duplicate code.

ADR-003 lands provider-backed records under the same `allObjects()` roof,
which means a single query-driven widget could seamlessly surface local +
synced data with no widget fork.

Two problems need solving together:

1. **Authoring friction**: adding a new Puck component means editing a
   single 3000-line file and possibly duplicating style-field injection
   logic. There is no DI seam, no registration hook, no way for a package
   outside `prism-studio` to contribute a component.

2. **Widget explosion**: every new "view" of a collection (tasks filtered
   by project, events for next week, contacts in a company) spawns either
   a new hand-rolled widget or a hardcoded variant. The kernel's generic
   query pipeline (`@prism/core/view`) exists but has no Puck surface.

## Decision

Introduce two complementary pieces:

### 1. `PuckComponentRegistry` — DI seam for builder components

In `@prism/core/bindings/puck/component-registry.ts`, define:

```ts
export interface PuckComponentProvider<TKernel = unknown> {
  /** Which entity type this provider handles. */
  readonly type: string;
  /** Build a Puck ComponentConfig for the entity def. */
  buildConfig(ctx: ProviderContext<TKernel>): ComponentConfig;
}

export interface ProviderContext<TKernel> {
  readonly def: EntityDef;
  readonly kernel: TKernel;
  readonly fieldFactories: FieldFactories;
  /** Pre-built fields + defaults from the generic entity→component path. */
  readonly baseConfig: ComponentConfig;
}

export class PuckComponentRegistry<TKernel> {
  register(provider: PuckComponentProvider<TKernel>): this;
  buildConfig(opts: {
    defs: ReadonlyArray<EntityDef>;
    kernel: TKernel;
    fieldFactories: FieldFactories;
  }): Config;
}
```

The registry's `buildConfig()` walks every `component`/`section` entity
def. For each, it looks up a registered provider by `def.type`; if one
exists, it calls the provider (passing the auto-generated `baseConfig` so
providers can extend rather than replace). If none exists, it falls back
to the default entity→component generator. Block-style fields are folded
in afterwards, same as today.

This keeps the generic auto-generated path as the default, makes bespoke
widgets first-class extensions, and — crucially — makes components
contributable. A package like `@prism/admin-kit` or a future
`@prism/chart-kit` can export a bundle of providers and a studio app
registers them alongside the built-ins.

Field factories (`colorField`, `alignField`, `sliderField`, `urlField`,
`classNameField`, `customCssField`, `fontPickerField`, `facetPickerField`,
`mediaUploadField`) move from `packages/prism-studio/src/components/puck-custom-fields.tsx`
to `@prism/core/bindings/puck/fields/` so providers outside studio can
use them. Studio re-exports for back-compat.

The 50 existing hand-wired special cases are **not migrated in this ADR**.
They are a separate, mechanical refactor — each block becomes a provider
file. The registry lands first, RecordList lands as the first provider
using it, and the migration follows as incremental cleanup.

### 2. Dynamic-content primitives

Add three generic components that replace N ad-hoc widgets with parametric
ones:

**`RecordList`** — one widget that queries records by type with a
structured filter/sort/group spec and renders a template per row.

```ts
interface RecordListProps {
  recordType: string;          // e.g. "task", "event", or "*" for all
  filters?: FilterConfig[];    // from @prism/core/view
  sorts?: SortConfig[];        // from @prism/core/view
  groups?: GroupConfig[];      // from @prism/core/view
  limit?: number;
  template: ComponentData[];   // Puck slot — rendered per row
  emptyState?: ComponentData[];
}
```

The filter/sort/group shape already exists as `ViewConfig` in
`@prism/core/view`, consumed by the existing Saved View / Found Set
infrastructure. ADR-004 does **not** introduce a parallel spec shape — it
reuses `FilterConfig` (12 operators: `eq`, `neq`, `contains`, `starts`,
`gt`, `gte`, `lt`, `lte`, `in`, `nin`, `empty`, `notempty`), `SortConfig`,
`GroupConfig`, and `applyViewConfig()`. `RecordList` is the first
UI-facing consumer of that pipeline via Puck props, and saved views
become a natural source of pre-configured `RecordList` instances.

Row templates bind field ids to element props. Inside the template slot,
author drops a `<FieldText field="title">`, `<FieldDate field="dueDate">`,
`<FieldBadge field="priority">` etc. A `RecordContext` (React context)
scoped to each row supplies the current record. These field components
are lightweight adapters, not renderers — they read `useRecord()` and
delegate to existing primitives (text, badge, etc.).

**`FilterBuilder`** — visual composer that produces a `FilterSpec` and
writes it to a sibling `RecordList`'s props. Implemented as a Puck custom
field (`filterField(kernel, { recordType })`) plus a visual panel for
complex cases.

**`Repeater`** — generic "for each item in array prop, render template"
over any array-shaped prop, not just kernel records. Useful for nav items,
testimonials, pricing tiers, column content — anything authored as data
rather than children.

### Migration shape for existing widgets

Once the registry is in place and RecordList is proven, the ten dynamic
record widgets collapse into pre-configured `RecordList` instances seeded
from templates:

- `TasksWidget` → `RecordList recordType="task"` with default filter
  `status != "done"`, sort `priority desc, dueDate asc`, template
  `<TaskRow/>`.
- `EventsWidget` → `RecordList recordType="event"` with filter
  `date between [today, +7d]`, template `<EventRow/>`.
- etc.

Each collapses to ~20 lines of template + default filter, down from
~100 lines of bespoke widget + ~60 lines of layout-panel wiring. The
specialised renderers (`TasksWidgetRenderer`, etc.) stay — they're now
*templates* inside RecordList instances, not top-level widgets.

Similar collapse works for `list-widget`, `table-widget`, `card-grid-widget`,
`report-widget` — they become RecordList with different view modes
(`list` / `table` / `grid` / `report`), which is just the template shape.

## Rationale

### Why a registry instead of a mega-switch

- The 2100-line `useMemo` in `layout-panel.tsx` is the single biggest
  testability and extensibility pain point in the studio package. It
  can't be unit-tested without mounting the whole panel, it can't be
  extended by other packages, and every new widget adds a special case
  someone else has to scroll past.
- Providers are trivially unit-testable — pass a fake kernel, assert the
  returned `ComponentConfig` shape.
- Other packages (`@prism/admin-kit`, `@prism/chart-kit`, user plugins)
  can ship Puck components. Today they can't.
- `@prism/admin-kit` already has a `createAdminPuckConfig()` factory
  (`packages/prism-admin-kit/src/puck-config.tsx`). The registry is the
  same idea, generalised.

### Why parametric RecordList instead of adding more widgets

- Adding a twelfth dynamic widget (say, "projects filtered by tag and
  sorted by priority") is currently a new file, a new entity type, a new
  special case in layout-panel, and a new filter helper. With RecordList
  it's a Puck template saved as a section template.
- The 10 existing filter helpers (`filterTasks`, `filterEvents`, etc.)
  are all variations on the same query over `data.status`, `data.date`,
  `data.tags`. `@prism/core/view` already has `applyFilters` with 12
  operators. The only missing piece is a serialisable spec shape the
  author can compose visually.
- ADR-003's provider records flow through `allObjects()` alongside local
  records. A generic RecordList picks up both for free. A hand-rolled
  widget needs explicit provider awareness.
- Authors — not developers — can now create new "views" without shipping
  code. This unlocks the FileMaker-style Found Set flow the legacy
  project had.

### Why reuse existing `ViewConfig` instead of a new spec shape

- `FilterConfig`/`SortConfig`/`GroupConfig`/`ViewConfig` already exist in
  `@prism/core/view` with a tested `applyViewConfig()` pipeline.
- The existing Found Set / Saved View infrastructure (`createSavedView`,
  `createSavedViewRegistry`) persists `ViewConfig` — a parallel spec
  shape would force two converters.
- Saved views become a natural source of pre-configured `RecordList`
  instances: pick a saved view in the Puck field, `RecordList` inherits
  its `ViewConfig`.

### Why field factories move to core

- Today `packages/prism-studio/src/components/puck-custom-fields.tsx`
  is studio-only. A provider defined in core or admin-kit cannot reach it.
- Field factories are pure and React-based, which makes
  `@prism/core/bindings/puck/` (a React-allowed binding) the right home.
- Studio re-exports keep all existing imports working.

## Consequences

### What changes

- New module: `@prism/core/bindings/puck/component-registry.ts`
  (interface + class + unit tests).
- New module: `@prism/core/bindings/puck/fields/` (moved from studio,
  studio re-exports).
- No new view module: `RecordList` consumes the existing
  `FilterConfig`/`SortConfig`/`GroupConfig`/`ViewConfig` types and
  `applyViewConfig()` pipeline from `@prism/core/view`.
- New entity: `record-list` in `packages/prism-studio/src/kernel/entities.ts`.
- New renderer: `packages/prism-studio/src/components/record-list-renderer.tsx`.
- New provider: `packages/prism-studio/src/panels/providers/record-list-provider.tsx`.
- `layout-panel.tsx` instantiates a `PuckComponentRegistry`, registers
  `RecordListProvider`, and feeds registered providers first before
  falling through to the existing special-case waterfall. Existing code
  stays; the registry is additive.
- Docs: update `packages/prism-studio/CLAUDE.md` component section,
  `packages/prism-core/CLAUDE.md` bindings section.

### What does not change (yet)

- The 50 existing special-case `if (def.type === …)` blocks stay. They
  can be migrated one-by-one in follow-up PRs. Each migration converts
  one block into a provider file; no behaviour changes.
- The ten dynamic-widget renderers stay as React components — they become
  row templates inside RecordList instances rather than top-level widgets.
- The generic `entityToPuckComponent()` path stays as the registry's
  default fallback; anything without a registered provider still works.

### Follow-ups

- Migrate hand-wired widgets into providers (one PR per category: form
  inputs, layout primitives, data displays, content, media, record
  widgets, containers, facet surfaces). Mechanical, no behaviour change.
- Build `FilterBuilder` custom field once `RecordList` is landed and
  tested.
- Build `Repeater` once a concrete use case appears (site nav, pricing
  tiers).
- Wire `createSavedViewRegistry` to `RecordList` props so saved views
  become first-class Puck templates.
- When ADR-003 ships, verify `RecordList` surfaces provider-synced
  records with no code fork.
