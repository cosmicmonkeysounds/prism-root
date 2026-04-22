# ADR-007: Slint 1.16 Upgrade — Interpreter Scoped to Builder

**Status:** Accepted (updated 2026-04-22)
**Date:** 2026-04-22

## Context

Prism's UI layer runs on Slint. The workspace originally pinned
`"1.8"` minimum, which Cargo.lock resolved to **1.15.1**. The Slint
interpreter (`slint-interpreter`) serves two roles in Prism:

1. **`prism-builder`** uses it to compile user-authored
   `BuilderDocument` trees into live Slint component fragments at
   runtime (ADR-006). These are isolated, small component trees.
2. **`prism dev shell`** used Slint's `live-preview` feature
   (interpreter mode) for in-process `.slint` hot-reload. This
   parses the *entire* `app.slint` (~2500 lines, 11 ScrollViews,
   25+ repeaters) through the interpreter at runtime.

The interpreter's `ChangeTracker` drops `VRc<ItemTree>` instances
during binding evaluation inside
`dynamic_item_tree::make_binding_eval_closure`. This triggers
`PropertyHandle::remove_binding` "Recursion detected" panics when the
Flickable geometry binding cascade runs. The crash path:

```
update_timers_and_animations
  → ChangeTracker::run_change_handlers
    → Flickable::geometry_without_virtual_keyboard
      → cascading interpreter binding evaluation
        → VRc drop during active CURRENT_BINDING
          → panic at properties.rs:617
```

**Confirmed on both Slint 1.15.1 and 1.16.0.** This is an upstream
interpreter bug. Compiled mode is unaffected — the same bindings are
generated as Rust code with no dynamic VRc lifecycle.

Additionally, the interpreter does not support `ComponentContainer`
or `component-factory` types, which `app.slint` used for the WYSIWYG
preview. These compile fine in compiled mode but cause parse errors
in interpreter mode.

## Decision

1. **Upgrade Slint from 1.15.1 to 1.16.0** — the workspace pins
   move from `"1.8"` to `"1.16"`.
2. **Disable live-preview** for `prism dev` commands — the
   interpreter cannot safely evaluate the full `app.slint` tree.
3. **Keep the interpreter for `prism-builder`** — runtime compilation
   of isolated component fragments does not hit the Flickable/
   ChangeTracker bug because those fragments don't contain ScrollViews
   or the complex binding graph that triggers the cascade.
4. **Remove `ComponentContainer` from `app.slint`** — replaced with
   a styled Rectangle placeholder. The WYSIWYG preview check
   (`compile_slint_preview`) still runs to validate the document
   but no longer sets a `ComponentFactory` on the UI.

## Implementation

- Workspace `Cargo.toml` pins: `slint`, `slint-build`, and
  `slint-interpreter` bumped from `"1.8"` to `"1.16"`.
- `cargo update` resolves all three to 1.16.0.
- `prism-cli/src/commands/dev.rs`: live-preview flags removed from
  `cargo_run_dev_builder`. The `.rs` respawn loop (DevLoop) remains
  active for hot-reload.
- `app.slint`: `component-factory` property and `ComponentContainer`
  element removed. `preview-factory-ready` bool retained for the
  selection overlay layer.
- `prism-shell/src/app.rs`: `preview_component_factory` import and
  `set_preview_factory` call removed. `push_wysiwyg_preview` still
  compile-checks the document but only sets the ready flag.

## Consequences

- `.slint` changes require an incremental rebuild (~3-5s). The `.rs`
  respawn loop already handles this automatically.
- `prism-builder`'s interpreter path benefits from the 1.16 upgrade.
- The persistent VecModel + count property architecture is validated
  as correct in compiled mode.
- Future: if Slint fixes the interpreter VRc bug upstream, live-preview
  can be re-enabled by restoring the flags in `cargo_run_dev_builder`.
