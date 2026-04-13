# bindings/

External library adapters — the ONLY layer in `@prism/core` allowed to import React, the DOM, or WebGL/WebGPU. Everything below (foundation, language, identity, kernel, network, interaction, domain) stays framework-agnostic; bindings is where those primitives are projected into real renderers, editors, and engines.

## Adapters

- [`audio/`](./audio/README.md) — `@prism/core/audio`. OpenDAW audio engine bridge. Drives the Layer 1 timeline (`@prism/core/timeline`) through a real DSP/mixer/exporter with React hooks.
- [`codemirror/`](./codemirror/README.md) — `@prism/core/codemirror`. CodeMirror 6 extensions and a React hook. Bidirectional sync between CM6 editors and Loro `LoroText` nodes. Prism's sole editor — no Monaco.
- [`kbar/`](./kbar/README.md) — `@prism/core/kbar`. KBar command palette with focus-depth routing (global / app / plugin / cursor).
- [`puck/`](./puck/README.md) — `@prism/core/puck`. Puck layout/page-builder bridge backed by Loro. Prism's page-builder surface — "views" (kanban, chart, map, report, etc.) are Puck widgets, NOT new `ViewMode` entries.
- [`react-shell/`](./react-shell/README.md) — `@prism/core/shell`. React shell components: `ShellLayout`, `ActivityBar`, `TabBar`, `LensProvider`, plus document/form/CSV/report surfaces and a print renderer.
- [`viewport3d/`](./viewport3d/README.md) — `@prism/core/viewport3d`. Loro-backed 3D scene state, OpenCASCADE.js CAD import, TSL shader compiler (WebGL/WebGPU), and gizmo controllers.
- [`xyflow/`](./xyflow/README.md) — `@prism/core/graph` (canonical alias) and `@prism/core/xyflow`. Spatial node graph built on `@xyflow/react`, custom nodes/edges, and elkjs auto-layout.

## Rules

- React / DOM / WebGL imports are confined to this folder. A sibling in `domain/` or anywhere below must not reach for `react`, `@xyflow/react`, `@measured/puck`, `@opendaw/*`, `@codemirror/*`, `kbar`, or three/WebGPU APIs.
- Each binding is free to pull from any layer below. They sit at the top of the DAG.
