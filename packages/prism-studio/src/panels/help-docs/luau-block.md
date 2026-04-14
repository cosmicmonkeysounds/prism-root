# Luau Block

A **Luau Block** embeds a Luau script directly in a page. The script runs in Prism's sandboxed browser Luau runtime (via `luau-web`) and can call `ui.*` functions to render its own UI inline.

## When to use it

- **Custom UI** — render something the built-in blocks don't cover (a dynamic chart, a custom form, a live data panel).
- **Scripted interactions** — respond to kernel events, mutate records, call relays.
- **Computed content** — derive text or styling from record data at render time.

## The `ui.*` API

Luau Blocks render their output via `ui.*` calls:

- `ui.text(str)` — plain text.
- `ui.heading(level, str)` — h1–h6.
- `ui.button(label, onClick)` — a clickable button.
- `ui.input(value, onChange)` — a text input.
- `ui.row(children)` / `ui.column(children)` — layout containers.
- `ui.card(children)` — a bordered card.

See the Luau Facet panel (`Shift+U`) for the full reference and sample scripts.

## Sandboxing

Every Luau Block runs inside Prism's capability sandbox. The script can read from `kernel.store`, call registered actions, and emit events — but it **cannot** make arbitrary network calls, access the file system, or touch globals that weren't explicitly exposed. Capabilities are configured via the Trust panel.

## Debugging

- Syntax errors show inline in the builder.
- Runtime errors are logged to the browser console and surface as red banners in the rendered block.
- Use `print()` for quick debugging — output goes to the console.

## Tips

- Start simple — `ui.text("hello")` first, then add state.
- Keep per-block scripts short. Long scripts belong in a FacetDefinition's Sequencer or a standalone automation rule.
- Use the Luau Facet lens (`Shift+U`) to iterate on scripts — it has a live preview and sample snippets.
