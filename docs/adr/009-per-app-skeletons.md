# ADR-009: Per-App Skeletons

**Status:** Accepted
**Date:** 2026-05-13

## Context

Today `packages/prism-shell/ui/app.prism-ui` is **the** Prism Studio
skeleton. One file, parsed once at boot via `include_str!`, drives
every Prism app. Its root element hard-codes
`app-name="Studio"`; its body is one `<shell.dock-workspace/>` element
that recursively walks whatever `DockState` happens to be active.

That worked for the launchpad-Studio-and-back-again loop because all
four built-in apps (Lattice, Flux, Musica, Studio) share Studio's
chrome — a workflow page bar at the bottom, a dock workspace in the
middle, overlays painted on top. ADR-005 wired the dock to render any
combination of registered panels, so Studio could host every app's
panels without skeleton-side knowledge.

Two pressures push back on the single-skeleton model:

1. **Some apps want fundamentally different chrome.** Musica's
   timeline-first DAW shape pulls toward a transport bar + zoomable
   ruler + mixer pane, not a workflow page bar. Flux's reactive
   dataflow editor pulls toward a full-canvas node graph. Forcing
   them into Studio's dock workspace will work for years but doesn't
   capture the per-app identity these apps deserve.

2. **The DSL self-bootstrap plan
   (`docs/dev/dsl-self-bootstrap.md`)** flagged per-app skeletons as
   an explicit follow-on. The doc deliberately deferred them: the
   four loops (manifest, dock catalog, service activation,
   `AppRegistrar` trait) had to land first, because per-app
   skeletons need the manifest to point at the skeleton file. With
   those four loops landed and 3603 tests passing, the seam is now
   ready to widen.

Two architectural questions arise:

- **Full skeleton replacement or slot composition?** Either each app
  ships a complete `.prism-ui` file that replaces today's
  `app.prism-ui` outright, or there's a shared host skeleton that
  exposes a `<slot>` and apps only declare what fills it.

- **Boot mount or active-app swap?** Either the shell mounts one
  skeleton at boot and never swaps it, or the active-app cursor
  drives a re-mount each time the user switches apps from the
  launchpad.

### Prior art surveyed

- **VS Code** — one shell, extensions add panels through a common
  contribution model. Equivalent to the current Prism approach (one
  skeleton, registered components). Doesn't ship per-app chrome.
- **JetBrains IDEs** — each IDE variant (IntelliJ, WebStorm, Rider)
  ships with the same shell crate plus per-product configuration.
  Equivalent to the slot-composition path: shared chrome, per-product
  slot fill.
- **DaVinci Resolve** — workflow pages (already adopted via ADR-005)
  let one shell host wildly different UIs per page. Equivalent to
  slot composition at the dock layer.
- **macOS Music vs. Final Cut Pro** — separate apps, separate
  binaries, full skeleton replacement. Costs: every app must
  re-implement the menu bar, command palette, settings. Not viable
  for Prism's "one shell, many apps" identity.

## Decision

**Slot composition.** A single host skeleton owns the universal
chrome; each app declares an app skeleton that fills a named slot.

### Two-layer skeleton model

```
┌───────────────────────────────────────────────────────────────┐
│ host skeleton — packages/prism-shell/ui/app.prism-ui          │
│                                                               │
│   <shell.app-window>                                          │
│     <slot name="app-content"/>     ← app skeleton mounts here │
│   </shell.app-window>                                         │
│                                                               │
│   <shell.workflow-page-bar/>      ← universal, app-agnostic   │
│   <shell.command-palette/>                                    │
│   <shell.toast-stack/>                                        │
│   <shell.help-tooltip/>                                       │
│   <shell.context-menu/>                                       │
│   <shell.menu-dropdown/>                                      │
│   <shell.component-picker/>                                   │
│   <shell.connection-picker/>                                  │
└───────────────────────────────────────────────────────────────┘
                              ↑
                              │
                              │ slot fill
                              │
┌───────────────────────────────────────────────────────────────┐
│ app skeleton — apps/<id>/shell.prism-ui                       │
│                                                               │
│   <shell.dock-workspace id="dock"/>      ← Studio / Lattice   │
│                                                               │
│ OR                                                            │
│                                                               │
│   <musica.transport id="transport"/>     ← Musica             │
│   <musica.timeline id="timeline"/>                            │
│   <musica.mixer id="mixer"/>                                  │
│                                                               │
│ OR                                                            │
│                                                               │
│   <flux.canvas id="canvas"/>             ← Flux               │
└───────────────────────────────────────────────────────────────┘
```

Apps that don't declare a skeleton fall through to a **default app
skeleton** that's exactly `<shell.dock-workspace/>` — preserving every
existing app's current behaviour with no manifest edit.

### Two skeletons + zero ambiguity

The slot mechanism already exists in `prism-ui-runtime` via the
`<slot name="X"/>` element + the `LowerCtx::with_host_children_by_slot`
seam (Wave 13.1). We're not inventing anything new — we're using the
same mechanism the shell already uses to compose `<shell.app-window>`
with its body. The host skeleton just declares the slot; the app
skeleton's elements become the slot's host children.

### Skeleton swap on active-app change

When the user clicks a launchpad tile, `WorkspaceSlot::set_active_app`
fires today as a no-op-with-effect (toggles a string field). With
per-app skeletons, it additionally triggers a re-fill: the next
render walks the host skeleton and looks up the active app's slot
contribution in `ShellInner.app_skeletons`. No re-parse — every app
skeleton is parsed once at boot and cached.

### Loader changes

`AppLoader` (already in `prism_shell::app_loader`) gains one step:

```rust
pub struct LoadedApp {
    pub manifest: AppManifest,
    pub base_dir: PathBuf,
    pub skeleton: Option<Skeleton>,   // NEW
}
```

The discover step:
1. Reads `manifest.toml` (today's behaviour).
2. If `manifest.entry.skeleton` is set, joins it to `base_dir`,
   reads the file, parses through `prism_ui_runtime::interpret`.
   Parse failures are logged + the skeleton drops to `None` (the app
   falls back to default).
3. Returns the hydrated `LoadedApp`.

### ShellInner changes

```rust
pub struct ShellInner {
    // ... existing fields ...
    /// Per-app skeleton lookup, keyed by `manifest.id`. Built at
    /// `Shell::new` from every `LoadedApp` whose manifest pointed at
    /// a parseable skeleton file. Apps not in this map fall back to
    /// `default_app_skeleton()`.
    pub app_skeletons: std::collections::HashMap<String, Skeleton>,
}
```

The host skeleton stays at `ui/app.prism-ui` and is the only one
parsed via `include_str!` (no I/O on the hot path).

### Render-side changes

`render_tree` already builds a `LowerScope` with a
`host_children_by_slot` map. It currently only fills slot data that
the shell composition blocks (e.g. `shell.app-window`) provide
through the existing block-level seam. The slot-fill loop gains one
line: before walking the host skeleton, look up the active app's
skeleton in `ShellInner.app_skeletons`, lower its top-level children
to `UiNode`s, and stash them in the slot map keyed by
`"app-content"`.

This is the *only* render-side change. The slot mechanism handles
the rest. No new pipeline, no new resolver, no new fan-out.

### Migration

Phase 0 (this ADR's implementation):

1. Add `<slot name="app-content"/>` to `app.prism-ui` between
   `<shell.app-window>` and `</shell.app-window>`. The previous
   `<shell.dock-workspace/>` body moves *out* of the host skeleton.
2. Add `default_app_skeleton()` returning a `Skeleton` parsed from a
   one-line source: `<shell.dock-workspace id="dock"/>`. This is the
   slot fill used for any app without its own skeleton.
3. Add `AppLoader` skeleton discovery + the `LoadedApp.skeleton`
   field.
4. Add `ShellInner.app_skeletons` + the slot-fill render hook.
5. Author `apps/lattice/shell.prism-ui` containing the same
   single-element body so we can prove per-app skeletons land
   correctly without changing observable behaviour.

Phase 1 (follow-on):

- Author distinct skeletons for Musica + Flux that prove the
  framework supports app-specific chrome.
- Wire `set_active_app` to mark the frame dirty so swaps are
  immediate.

## Rationale

- **Slot composition** keeps the chrome under shell ownership.
  Menu bar, command palette, status bar are universal; apps don't
  re-author them and can't accidentally break them. Apps decide
  only what lives inside the workspace area.
- **Default app skeleton** means today's four apps all keep working
  without manifest edits — only apps that *want* custom chrome
  declare a skeleton.
- **Reuses existing seams.** The slot mechanism is already wired
  through Wave 13.1; the loader is already wired through Loop 1.
  The total new code is one map field, one parse step, one
  slot-fill call.
- **No new IPC, no new file format.** App skeletons are
  `.prism-ui` files just like the host skeleton. The same parser,
  the same lowering, the same TagResolver.

## Consequences

- `ui/app.prism-ui` loses its `<shell.dock-workspace/>` child; that
  element migrates to the default app skeleton (or to per-app
  `shell.prism-ui` files).
- `LoadedApp` grows a `skeleton: Option<Skeleton>` field. The
  field is `Option` so apps that declare no skeleton (or whose
  skeleton fails to parse) don't break — they fall back.
- `ShellInner` gains `app_skeletons: HashMap<String, Skeleton>`.
- Render time: one additional lookup + one slot-fill assignment
  per frame. Both are O(1) and don't touch I/O.
- Boot time: one disk read + parse per app whose manifest declares
  a skeleton. Cached.
- Tests: a new integration test in
  `tests/dsl_self_bootstrap.rs` covering manifest-declared
  skeletons replacing the slot fill.

## Out of scope (intentional)

- **Sub-slots inside the app skeleton.** Apps that need multiple
  slots (e.g. Musica's transport vs. timeline vs. mixer) can
  declare their own `<slot>`s — but the host skeleton only
  exposes `app-content`. Hierarchical slot composition is the app's
  internal concern.
- **Hot-swap during render.** Switching the active app re-walks the
  tree on the next frame, not mid-frame. A mid-frame swap would
  require dirty-queue surgery the dirty-queue isn't sized for.
- **Per-route skeletons.** Today the active-app cursor is the only
  thing that drives a skeleton swap. URL-driven routing (each app's
  internal navigation) lives below the skeleton level — apps handle
  their own routing through state, not by swapping the skeleton.
- **Skeleton inheritance.** Apps can't extend each other's
  skeletons. Each is standalone (or falls through to default).
  Inheritance is a future ADR if a real need emerges.
