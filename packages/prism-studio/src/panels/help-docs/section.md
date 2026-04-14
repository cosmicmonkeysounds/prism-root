# Section

A **Section** is a horizontal band — the building block for page layouts. Every block you add to a page lives inside a section, and sections stack vertically from top to bottom.

## When to use it

Use a Section to group related content into a visual band. Typical examples:

- **Hero** — full-width intro with headline, subhead, and call-to-action.
- **Feature row** — three columns of icons + copy.
- **CTA band** — a single horizontal strip with one big button.
- **Testimonials** — a section of quotes or cards.

## Fields

- **background** — color, gradient, or image URL for the section background.
- **fullBleed** — true to span the entire viewport width; false to respect page max-width.
- **padding** — vertical space above and below the section content.
- **alignment** — how the section's direct children are aligned (`start`, `center`, `end`).

## Section vs Columns

Sections stack **vertically**. Columns lay out their children **horizontally**. A typical three-column feature row is a Section containing a Columns block containing three cards.

## Tips

- Don't nest sections directly inside sections — use a Columns block or a Page Shell region instead.
- Full-bleed sections are the right choice for hero backgrounds. Non-full-bleed is right for content bands that should respect the page's max width.
- Use padding, not margin, to space sections from each other — margins collapse unpredictably across browsers.
