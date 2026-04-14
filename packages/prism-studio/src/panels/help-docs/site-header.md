# Site Header

A **Site Header** is a standalone masthead block — the kind you'd put at the top of a marketing page. It is not part of a shell; it is a regular block you drop into any region.

## Fields

- **logoText** — wordmark text. Leave blank if you want a pure-image logo.
- **logoImage** — optional image URL rendered before the wordmark.
- **navLinks** — top-level nav entries, one per line as `label | href`.
- **sticky** — pin the header to the top of the viewport on scroll.
- **variant** — `light` / `dark` / `transparent`.

## Site Header vs App Shell header

Site Header is a **content block** — it renders text and nav links and nothing more. It has no state, no active-route highlighting, no cross-page awareness.

App Shell's `header` region is **app chrome** — it stays mounted as users navigate between pages and typically contains global controls (search, user menu, notifications).

Use Site Header when you're building a single marketing page. Use App Shell's header when you're building a multi-page site that needs persistent chrome.

## Responsive behaviour

Site Header collapses to a hamburger menu below 768 px. The breakpoint is configurable via the `mobileBreakpoint` field.
