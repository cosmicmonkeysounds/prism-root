# Heading

A **Heading** renders an `h1` – `h6` element with text content and optional styling. Headings anchor the page outline and are the primary text size landmark for both users and search engines.

## Fields

- **text** — the heading text.
- **level** — `h1` through `h6`. Controls both the HTML element and the default font size.
- **alignment** — `left`, `center`, `right`.
- **color** — optional token or hex color override.
- **weight** — optional font weight override (`regular`, `medium`, `semibold`, `bold`).

## SEO and accessibility

Use **exactly one** `h1` per page — it should describe the page's primary topic. Every subsequent heading should step down one level at a time (`h1 → h2 → h3`), not skip. Screen readers use heading levels to build an outline for keyboard navigation.

## Editing

Click a heading in the canvas to edit its text inline. Press Enter to commit. Press Escape to cancel.

## Style overrides

Headings inherit color and weight from the page's design tokens by default. Override fields here only override the styling for this one heading — if you want to change all headings of a given level across your site, edit the design tokens instead (`Shift+T`).

## Tips

- Don't use headings to style plain text — use a Text block with a bold weight instead. Screen readers will thank you.
- Keep `h1` text short and descriptive — it's what shows up in search results and browser tabs.
