# prism-builder

The Clay-native page builder that replaces Puck. Phase-3 target of the
Clay migration. Owns the component-type registry, the document tree
schema, and the forward-only Puck JSON reader that keeps existing
user content loadable forever.

## Build & Test
- `cargo build -p prism-builder`
- `cargo test -p prism-builder` — unit + insta snapshot tests.

## Public surface
From `src/lib.rs`:
- `Component`, `ComponentId`, `RenderContext` — the trait every
  renderable block implements plus its id + per-frame context.
- `BuilderDocument`, `Node`, `NodeId` — the serializable document
  tree written to disk by Studio.
- `ComponentRegistry`, `RegistryError` — the DI entry point.

## Architecture
Four modules, all in `src/`:

- `component.rs` — the `Component` trait every renderable block
  implements. Blocks are `Arc<dyn Component>`.
- `document.rs` — `BuilderDocument` + `Node` + `NodeId`. The
  serializable tree that Studio saves / loads.
- `registry.rs` — `ComponentRegistry`, backed by an `IndexMap`
  keyed by `ComponentId`. Register once at boot (`register(Arc<dyn
  Component>)`), look up by id at render time (`get(&str)`). This is
  the **single DI surface** for adding new block types — no side
  registries, no per-component singletons.
- `puck_json.rs` — **permanent** one-way reader for legacy
  Puck `{ type, props, children }` documents. We read Puck JSON
  forever so existing user content boots; new content is always
  written in the `BuilderDocument` schema. This is not deprecation
  and there is no phase-out planned — treat it as a load-only
  compatibility layer.

## Adding a new block type
Contributors adding new drag-droppable blocks must go through the
registry. Do not hand-wire a `Node` or stash a block factory in a
module-level `static`.

1. Define a struct that implements `Component`.
2. Decide the component's `ComponentId` (stable string, versioned if
   the props shape can change).
3. Call `ComponentRegistry::register(Arc::new(MyBlock { .. }))` from
   the panel that owns the registration.
4. Let `BuilderDocument` reference it by `ComponentId` — the tree
   walker resolves `ComponentRegistry::get(id)` at render time.

Field factories (the reusable building blocks for property panels)
will land in `registry` alongside `register` once Phase 3 brings the
properties panel over from the old TS tree. Add shared factories
there, not inline in each component.

## Scaffolding status
Types and traits are defined; method bodies are stubs until Phase 3
lands the rest of the builder UI. When expanding a stub, keep the
insta snapshots up to date and add integration tests that walk a
`BuilderDocument` + registry end-to-end.

## Dependencies
- `prism-core` — types + registry utilities reused across the
  workspace.
- No hard dep on `prism-shell`: `prism-shell` depends on
  `prism-builder`, not the other way around. Keep the graph that
  direction.
