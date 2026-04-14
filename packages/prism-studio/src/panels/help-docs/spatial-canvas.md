# Spatial Canvas

A **Spatial Canvas** is a free-form absolute-positioning surface. Unlike a Section or Columns block — which flow their children automatically — a Spatial Canvas lets you drag children to any pixel position and freely overlap them.

## When to use it

- **Diagrams** — boxes with connecting arrows.
- **Annotated images** — pin notes on top of a screenshot.
- **Dashboards** — place widgets at fixed screen positions.
- **Pixel-perfect layouts** — when flow layout can't reach what you need.

## Fields

- **width** / **height** — canvas dimensions in pixels. Content larger than these scrolls.
- **grid** — optional background grid (pixels between gridlines).
- **snap** — snap dragged children to grid intersections.
- **background** — canvas fill color or image.

## Editing

- **Drag** any child to reposition it.
- **Drag a corner handle** to resize.
- **Drag a rotation handle** to rotate.
- **Shift-click** multiple children to select a group.
- **Arrow keys** nudge the selection by one pixel; Shift+Arrow nudges by ten.

## Children

Spatial Canvas accepts **any** component as a child. The child's normal fields still apply, and a few extra positioning fields get injected automatically:

- `x`, `y` — top-left position in pixels.
- `width`, `height` — explicit size.
- `rotation` — degrees clockwise.
- `zIndex` — stacking order.

## Tips

- Turn on **snap** when composing diagrams — alignment is dramatically easier.
- Spatial Canvas is not responsive by default. If you need the canvas to scale with the viewport, use the scale override in the Inspector and consider a fluid transform.
- For free-form **node graphs** (boxes + arrows with auto-layout), use the Graph lens (`g`) instead — it's built on xyflow and handles routing.
