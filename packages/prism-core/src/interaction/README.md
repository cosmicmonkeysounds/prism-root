# interaction

UI-facing state and primitives for Prism clients. Every module here is React-agnostic — generic over component types so higher layers (Studio, Flux, Lattice, Musica) can specialize with their own renderer type. Loro CRDT remains the source of truth; these modules project and derive from it.

## Categories

- [activity](./activity/README.md) — `@prism/core/activity` — append-only per-object audit trail with auto-diffing from GraphObject snapshots.
- [atom](./atom/README.md) — `@prism/core/atom` — PrismBus event bus, reactive Zustand atom stores, object/edge cache, bus-to-atom bridges.
- [design-tokens](./design-tokens/README.md) — `@prism/core/design-tokens` — framework-agnostic CSS variable registry (colors/spacing/fonts).
- [input](./input/README.md) — `@prism/core/input` — KeyboardModel, InputScope, InputRouter — shortcut parsing and scoped routing.
- [layout](./layout/README.md) — `@prism/core/layout` — SelectionModel, PageModel, PageRegistry, LensSlot, LensManager.
- [lens](./lens/README.md) — `@prism/core/lens` — lens registry, manifests, shell-store tabs/panels, LensBundle install pattern.
- [notification](./notification/README.md) — `@prism/core/notification` — NotificationStore with eviction policy and debounced NotificationQueue.
- [page-builder](./page-builder/README.md) — `@prism/core/page-builder` — block style primitives, responsive overrides, font registry, and deterministic page export (JSON + HTML).
- [search](./search/README.md) — `@prism/core/search` — TF-IDF SearchIndex and cross-collection SearchEngine with filters/facets/pagination.
- [view](./view/README.md) — `@prism/core/view` — derived view pipeline (filters/sorts/groups), ViewRegistry capabilities, LiveView materialization, SavedView registry. Visual view types (kanban/chart/map/etc.) are composed as Puck widgets at higher layers, not new ViewMode entries.

## Layering

`interaction/` sits above `kernel/` and `network/` and below `domain/` + `bindings/` in the prism-core DAG. It must not import from React or DOM; those live in `bindings/`. See the top-level `packages/prism-core/CLAUDE.md` for the full dependency graph.
