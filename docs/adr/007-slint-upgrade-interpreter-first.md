# ADR-007: Slint Upgrade to 1.16 — Interpreter-First Strategy

**Status:** Accepted
**Date:** 2026-04-22

## Context

Prism's UI layer runs on Slint. The workspace pins `"1.8"` minimum,
which Cargo.lock resolves to **1.15.1**. The Slint interpreter
(`slint-interpreter`) is central to Prism's architecture:

- `prism-builder` uses it to materialise `BuilderDocument` trees into
  live component trees at runtime (ADR-006).
- `prism dev shell` uses Slint's `live-preview` feature (interpreter
  mode) for in-process `.slint` hot-reload during development.

On 1.15.1, the interpreter's `ChangeTracker` drops `VRc<ItemTree>`
instances during binding evaluation inside
`dynamic_item_tree::make_binding_eval_closure`. This triggers
`PropertyHandle::remove_binding` "Recursion detected" panics on any
model update that flows through a `ScrollView`'s `Flickable` geometry
binding. The crash path:

```
update_timers_and_animations
  → ChangeTracker::run_change_handlers
    → Flickable::geometry_without_virtual_keyboard
      → cascading interpreter binding evaluation
        → VRc drop during active CURRENT_BINDING
          → panic at properties.rs:596
```

Compiled mode (without the interpreter) is unaffected because the
same bindings are generated as Rust code with no dynamic VRc lifecycle.

## Decision

**Upgrade Slint from 1.15.1 to 1.16.0** (released 2026-04-16) and
**re-enable the interpreter / live-preview path**.

Rationale:

1. **The interpreter is not optional.** `prism-builder`'s runtime
   walker depends on `slint-interpreter` to instantiate user-authored
   component trees. Disabling the interpreter is not a sustainable
   path — it blocks the builder's core functionality.

2. **Slint's interpreter is battle-tested.** The VRc/ChangeTracker
   panic is a known class of bug in Slint's property system. Upstream
   releases routinely fix these. Upgrading to 1.16.0 is the correct
   first move before investigating whether our own binding patterns
   contribute.

3. **If the panic persists on 1.16.0**, the next step is to audit our
   property system's interaction with the interpreter — specifically
   how persistent `VecModel` updates flow through `ScrollView`
   geometry bindings. The persistent-VecModel + count-property
   infrastructure already landed and is correct in compiled mode; the
   question is whether the interpreter needs a different update
   pattern (e.g., batched updates, deferred model swaps).

## Implementation

- Bump workspace `Cargo.toml` pins: `slint`, `slint-build`, and
  `slint-interpreter` from `"1.8"` to `"1.16"`.
- `cargo update -p slint -p slint-build -p slint-interpreter` to
  resolve 1.16.0.
- Re-enable live-preview flags in `prism-cli/src/commands/dev.rs`:
  `--features prism-shell/live-preview` + `SLINT_LIVE_PREVIEW=1`.
- Update tests expecting no live-preview flags.
- Verify `prism dev shell` no longer panics with interpreter mode.

## Consequences

- `.slint` hot-reload during development works again without rebuilds.
- `prism-builder`'s interpreter path benefits from upstream fixes.
- If 1.16.0 still panics, we have a clear next investigation target
  (our VecModel update patterns under the interpreter) rather than
  a disabled feature.
