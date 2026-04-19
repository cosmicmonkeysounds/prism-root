# ADR-003: Unified Builder Layout Model

**Status**: Accepted
**Date**: 2026-04-19

## Context

Prism Builder currently treats layout as a side-effect of component
rendering: a `Container` emits `VerticalLayout`, `Columns` emits
`HorizontalLayout`, and Slint's implicit flow determines where things
land. There is no spatial data on `Node`, no coordinate system, no
transforms, and no way to position elements freely on a page.

This blocks three capabilities the builder needs:

1. **Print-style page layout** — editorial grids with margins, gutters,
   bleeds, and named grid areas (newspaper columns, poster layouts,
   magazine spreads). These are structural properties of the *page*,
   not components you drag onto one.

2. **Unity-style transforms** — every element should carry position,
   rotation, scale, and a pivot point relative to its layout-computed
   origin. Modifiers (snap-to-grid, aspect-ratio lock, clamp-to-bounds)
   compose on top. This is what makes a builder feel like a design tool
   rather than a form editor.

3. **Coordinate space queries** — converting between local, parent, page,
   and screen coordinates. Required for hit-testing, drag-and-drop,
   snap guides, and multi-select bounding boxes.

## Decision

We introduce a three-layer layout model into `prism-core` and
`prism-builder`, backed by `glam` (SIMD-accelerated 2D math) and
`taffy` (CSS Grid/Flexbox/Block layout engine).

### Layer 1: Page Grid

Every page in a `BuilderDocument` gains a `PageLayout` — a structural
property, not a component. It defines the page's physical dimensions,
margins, bleed area, and a CSS Grid template (rows, columns, gaps).

Taffy computes the grid cells. Components placed into grid areas get
their bounds from Taffy, not from Slint's implicit flow.

```rust
pub struct PageLayout {
    pub size: PageSize,
    pub orientation: Orientation,
    pub margins: Edges<f32>,
    pub bleed: f32,
    pub columns: Vec<TrackSize>,
    pub rows: Vec<TrackSize>,
    pub column_gap: f32,
    pub row_gap: f32,
}
```

### Layer 2: Transform

Every `Node` gains a `Transform2D` that applies *on top of* the
layout-computed position. A node in a CSS Grid cell gets its base
(x, y, w, h) from Taffy; `Transform2D.position` offsets it,
`rotation` spins it around `pivot`, `scale` resizes it.

```rust
pub struct Transform2D {
    pub position: Vec2,
    pub rotation: f32,
    pub scale: Vec2,
    pub pivot: Vec2,
    pub anchor: Anchor,
    pub z_index: ZIndex,
    pub modifiers: Vec<TransformModifier>,
}
```

A propagation pass walks the tree after layout, composing local
`Affine2` matrices into global ones. `ComputedTransform` caches both
for O(1) coordinate conversion queries.

### Layer 3: Layout Mode

Each node declares how it participates in its parent's layout:

- **`Flow`** — Taffy computes position and size (flex/grid/block
  semantics). Carries CSS-like properties: display, size, min/max,
  padding, margin, flex-grow/shrink/basis, grid placement.
- **`Free`** — removed from flow. `Transform2D.position` is the sole
  source of truth. Used for overlays, callouts, free-form design.

## Crate Stack

| Layer | Crate | Role |
|---|---|---|
| Math | `glam` | `Vec2`, `Affine2`, `Mat3` — SIMD on native + WASM |
| Layout | `taffy` | CSS Grid + Flexbox + Block + Absolute positioning |
| Spatial index | `rstar` | R*-tree for hit-testing and snap guides (future) |
| Text measurement | `parley` | Accurate text sizing for Taffy's MeasureFunc (future) |

`glam` and `taffy` land now. `rstar` and `parley` are deferred to the
interaction and text-measurement passes respectively.

**Not adopted:**
- `morphorm` — Taffy is strictly more capable (CSS Grid, Block layout)
- `nalgebra` — overkill for 2D; glam is faster and simpler
- `euclid` — good typed spaces but no SIMD; glamour can be layered on
  glam later if compile-time coordinate safety is needed
- `lyon` — Slint owns rendering; path tessellation is unnecessary

## Module Placement

| Module | Package | Contents |
|---|---|---|
| `foundation::geometry` | `prism-core` | `Point2`, `Size2`, `Rect`, `Edges<T>` |
| `foundation::spatial` | `prism-core` | `Transform2D`, `ComputedTransform`, `Anchor`, `ZIndex`, `TransformModifier` |
| `layout` | `prism-builder` | `PageLayout`, `PageSize`, `Orientation`, `TrackSize`, `LayoutMode`, `FlowProps`, layout computation pass |

Geometry and spatial types live in `prism-core` because they are
general-purpose foundations used by the shell (hit-testing, selection
rectangles), the builder (layout computation), and potentially the
relay (absolute-positioned HTML output).

## Integration Architecture

```
BuilderDocument
  +-- pages: Vec<Page>
       +-- layout: PageLayout
       +-- root: Node
            +-- layout_mode: LayoutMode (Flow | Free)
            +-- transform: Transform2D
            +-- component: ComponentId
            +-- children: [Node]

            +-----------------+
            |   Taffy Pass    |  Build Taffy tree from LayoutMode::Flow
            +---------+-------+  nodes. Solve. Extract (x,y,w,h).
                      |
            +---------v-------+
            |   Transform     |  Compose local Affine2 per node
            |   Propagation   |  (layout rect + Transform2D -> matrix).
            +---------+-------+  Cache global Affine2 on each node.
                      |
         +------------+------------+
         |            |            |
   +-----v----+ +----v-----+ +----v-----+
   |  Slint   | |  HTML    | |  R-tree  |
   |  Render  | |  SSR     | |  Index   |
   +----------+ +----------+ +----------+
   explicit     relay         hit-test,
   x/y/w/h                   snap, DnD
   + rotate
```

**Slint rendering**: the builder canvas emits explicit `x`, `y`,
`width`, `height` + `rotation` on each element instead of layout
primitives. Slint still renders — it just receives pre-computed
positions. The Studio chrome (toolbars, panels, inspector) continues
to use Slint's native layout.

**HTML SSR**: the relay can use Taffy-computed positions to emit
`style="position: absolute; ..."` or fall back to semantic flow
layout for accessibility.

## Drag-and-Drop (future)

With an R-tree built from computed global bounds:

1. **Pick** — `rtree.locate_at_point(cursor)` to find the hit node
2. **Drag** — update `Transform2D.position`, re-propagate, rebuild index
3. **Snap** — query R-tree for nearby edges, compute alignment guides
4. **Drop** — if into a Flow container, convert to grid/flex insertion;
   if onto canvas, stay Free

Transform modifiers fire during drag: `SnapToGrid` quantizes position,
`ConstrainAspectRatio` locks resize handles, `ClampToBounds` prevents
dragging outside the page.

## Consequences

- `Node` grows from a flat `(id, component, props, children)` bag to a
  spatially-aware element with layout mode and transform. This is a
  breaking change to the document schema — existing documents need a
  migration (defaulting to `LayoutMode::Flow` with identity transform).
- The Slint render walker must change from emitting layout primitives
  to emitting explicit coordinates on the canvas.
- Two new workspace dependencies (`glam`, `taffy`) enter the tree.
  Both are widely used, actively maintained, and compile to WASM.
- The layout computation pass is a new responsibility of
  `prism-builder` — it runs before rendering and produces a
  `ComputedLayout` that both render paths consume.
