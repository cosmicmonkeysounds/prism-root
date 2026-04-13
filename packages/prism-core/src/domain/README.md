# domain/

Higher-level domain models that compose the foundational primitives (object-model, crdt-stores, batch, etc.) into cohesive application concerns. Sits above `interaction/` and below `bindings/` in the dependency DAG — still React-free.

## Categories

- [`flux/`](./flux/README.md) — `@prism/core/flux`. Productivity / CRM / finance / inventory entity schemas, edge types, automation presets, and CSV/JSON import/export.
- [`graph-analysis/`](./graph-analysis/README.md) — `@prism/core/graph-analysis`. Dependency graph primitives (topological sort, cycle detection, blocking chain, impact analysis) and a CPM critical-path planning engine.
- [`timeline/`](./timeline/README.md) — `@prism/core/timeline`. Non-linear-editor timeline engine with transport, tracks, clips, automation lanes, markers, and a PPQN tempo map modeled on OpenDAW. Bridged to audio via `@prism/core/audio`.

## Rules

- No React, DOM, or WebGL imports — this layer is pure data-model code.
- May import from `foundation/`, `language/`, `identity/`, `kernel/`, `network/`, and `interaction/`.
- Anything visual, editor-bound, or GL-bound belongs in `bindings/`.
