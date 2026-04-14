# Site Footer

A **Site Footer** is the bottom counterpart to Site Header — a standalone block containing copyright, legal links, social icons, and optional columns of navigation.

## Fields

- **copyright** — plain text shown at the bottom (e.g. `© 2026 Acme Inc.`).
- **columns** — each column is a heading + a list of `label | href` links.
- **social** — icon links, one per line as `platform | href` (e.g. `twitter | https://twitter.com/acme`).
- **variant** — `light` / `dark` / `bordered`.

## Layout

By default, columns stack vertically on narrow screens and wrap into a grid on wider screens. The footer always pins the copyright row to the very bottom regardless of column count.

## Common patterns

- **Minimal** — copyright only, no columns.
- **Marketing** — 3–4 columns of nav links + social row + copyright.
- **Legal** — single row of `Privacy | Terms | Cookies` + copyright.

## Accessibility

Every link gets an `aria-label` derived from its text. Social icons render their `platform` name as the accessible label so screen readers announce "Twitter link" rather than "link".
