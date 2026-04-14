# Columns

A **Columns** block lays out its children horizontally in a grid. You pick the number of columns and every direct child drops into the next cell.

## Fields

- **count** — number of columns (1–6).
- **gap** — horizontal space between columns, in pixels.
- **alignment** — vertical alignment of content inside each column (`top`, `center`, `bottom`, `stretch`).
- **responsive** — collapse to a single column below a breakpoint.

## When to use it

- **Feature row** — three columns of icon + heading + copy.
- **Pricing table** — three or four columns of plan cards.
- **Gallery** — four columns of images.
- **Two-up content** — heading and paragraph on the left, image on the right.

## Responsive collapse

When `responsive` is enabled, the columns collapse to a single vertical stack below the breakpoint. You can override per-child sizing by setting the child's own `column-span` field.

## Columns vs Section

A Columns block is a **horizontal** layout primitive. A Section is a **vertical** band. Most feature rows are a Section containing a Columns block containing three cards.

## Tips

- Don't use Columns for page-level layout — use Page Shell for that. Columns is for block-level content grouping.
- If you need columns of different widths, use the `column-span` override on each child rather than creating multiple Columns blocks.
- The default gap (24 px) is a good starting point for most feature rows.
