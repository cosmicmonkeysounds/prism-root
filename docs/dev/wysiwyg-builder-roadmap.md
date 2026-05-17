# WYSIWYG Fullstack App Builder — Roadmap

> **Scope.** This doc tracks the *product* path: from where the codebase
> is today to the full WYSIWYG fullstack app builder Prism is meant to
> be. It is deliberately distinct from `state-of-prism.md`, which is the
> migration/cleanup tracker (ADR + dev-plan reconciliation). Read that
> one for "is plan X landed"; read this one for "what does done look
> like and what's between here and there."
>
> Status claims are point-in-time (2026-05-16) and synthesised from the
> primary docs + a code sweep. Per house discipline, verify load-bearing
> claims against the code — `cargo check --workspace` is the safety net.

## 0. What "full WYSIWYG fullstack app builder" means for Prism

Five capabilities, each with a concrete acceptance bar:

1. **Visual canvas authoring.** A user drags a block onto a page, sees
   it render exactly as it will ship (femtovg native / WebGL web /
   semantic-HTML SSR — one `RenderCommand` stream), selects it by
   clicking the rendered thing, and edits it via GUI gestures
   (drag/drop/resize, not property-panel overlays — per the
   "inline editing is GUI-only" principle).
2. **One source, three projections.** Every widget is PRUI (tree) +
   PRSS (theme) + Luau (behaviour), authorable as one `.prui` file or
   three siblings, with zero migration cost between the two shapes
   (`prui-luau-fusion.md`). The shell itself is a Builder project built
   from the same primitives users compose with
   (`composable-builder-plan.md` §1).
3. **Fullstack data layer.** Loro CRDT is the source of truth. A user
   opens a folder on disk; files auto-ingest as `GraphObject`s; a
   watcher keeps the object graph live; the Explorer panel browses it;
   blocks bind reactively to that data two-way (`project-vault.md`,
   `data-template-system.md`).
4. **Real dev environment.** Project tree, symbol index,
   jump-to-definition, diagnostics, find-in-files, CRDT inspector,
   presence overlay — the in-shell editor is a real IDE, not a
   textarea (`ide-mode-plan.md`).
5. **Deploy + collaborate.** Rust→WASM web / Rust→native desktop &
   mobile, packaged and self-updating; Loro-backed multiplayer with
   live cursors/selections and federated peer signals.

The acceptance bar for "done": a non-programmer opens Prism, points it
at a folder, drags blocks onto a canvas, binds them to their data by
clicking, scripts an exception in Luau, sees diagnostics inline, and
ships to web + desktop — all without leaving the shell.

## 1. Where the codebase is today (per capability)

| Capability | Status | Evidence |
|---|---|---|
| Render substrate | ✅ Solid | Single Taffy layout backend, retained `Surface`, three render backends off one `RenderCommand` stream. Slint fully exorcised. |
| Reactive spine | ✅ Solid | `Signal<T>` unifies local/IPC/federated/SSR; all 11 `dioxus-inspiration.md` phases landed incl. selective re-walk + reactive `prism.state`. |
| Component model | ✅ Solid | One `ComponentRegistry`; 17 starter blocks + ~48 primitives + core widgets via `CoreWidgetBlock`; dashboard widgets unified in (no parallel `WidgetRegistry`). |
| One-source/three-projection authoring | ✅ Runtime-complete | `prui-luau-fusion.md` Waves A–I; PRUI/PRSS/Luau fusion ships; hot-reload for `.prui` + `.prss` (incl. per-class PRSS invalidation). |
| Visual canvas authoring | 🟡 Partial | Inspector tree is the authoritative selection surface and is fully wired incl. facet inline-template descendants. **Gap:** clicking a *rendered* template descendant on the live canvas to select it (canvas hit-test → composite-id route). |
| Two-way data binding | 🟡 Partial | `bind:*` carries through to `data-bind-*` semantic attrs + skeleton bind installer collects them; **gap:** `data-bind-*` → registered `Effect` install path (A2 partial). |
| Fullstack data layer | 🟡 Partial | Project Vault V1 in flight. **Gap:** V2 (disk watcher + Explorer panel) and V3 (folder hierarchy + thumbnails) — the literal "open a folder, see your files as data" loop. |
| Real dev environment | 🟡 Early | IDE Mode Phase 1 (project tree, open-path) + Phase 4 (Inspector/DevTools, 4 lenses) shipped. **Gap:** Phases 2/3/5/6/7 — symbol index, diagnostics panel, folding/inlays, find-in-files, split/persistence. |
| Deploy | 🟡 Partial | web (wasm-bindgen) + native build paths ship. **Gap:** Phase 6 — mobile + `cargo-packager`/`self_update` packaging. |
| Collaborate | 🟡 Substrate-only | Loro CRDT + `PresenceManager` exist; probe firing + presence ingest stubbed (no host event-router / `PresenceService` consumer yet). |

The honest summary: **the engine and authoring substrate are done; the
product surface (canvas WYSIWYG, the data loop, the dev environment,
packaging) is the remaining work.**

## 2. The gap, concretely

### 2.1 Canvas WYSIWYG (the headline gap)
The inspector tree drives selection. The promise "click the thing you
see" holds for top-level nodes but not for facet template descendants
on the *rendered* canvas — only via the inspector row. Closing this
needs a canvas hit-test branch on the composite
`"<facet_node_id>::tpl/<path>"` ids that already exist on the inspector
side. Everything downstream (selection state, property panel, write
routing) is already wired. **Smallest high-value win on the product
axis.**

### 2.2 The fullstack data loop
`project-vault.md` V2/V3 is the difference between "a UI builder" and a
"fullstack app builder." Disk folder → auto-ingest → live object graph
→ Explorer panel → two-way bound blocks. V1 is in flight; V2 (watcher +
Explorer) is the load-bearing slice. Pairs with finishing the `bind:*`
install path (§2.3) — data with no binding is inert.

### 2.3 `bind:*` install path (A2)
Authors can *declare* two-way bindings; the runtime carries them to
`data-bind-*`; the skeleton installer collects them — but the
`data-bind-*` → registered `Effect` wiring is missing, so declared
bindings don't yet observe/write. Small, well-scoped, unblocks the data
loop.

### 2.4 Dev environment depth
Today's editor is Phase 1+4. A "fullstack app builder" needs at minimum:
real project tree (folders/rename/drag), symbol index +
jump-to-definition, and a diagnostics panel. Phase 3 (diagnostics) is
blocked on `luau-analyze` integration (landed as a CI gate; needs the
in-shell surface) and an open femtovg question (no wavy-underline
primitive).

### 2.5 Maintainability tax on velocity
Not a feature gap but it throttles every feature above:
`prism-shell/src/state.rs` is **8558 lines and growing** (was ~6646 six
weeks ago); `prism-builder/src/ui_lower.rs` (1794) and `ui_resolver.rs`
(1465) are the next tier. The `prism-shell/src/app/` decomposition is
the single biggest structural refactor outstanding. This is tracked as
cleanup in `state-of-prism.md` §3 but belongs on the product critical
path because it is now the rate limiter.

## 3. Phased path to the full vision

Ordered for dependency + payoff. Each phase ends test-green + clippy-clean.

**Phase A — Unblock the data loop.**
1. Finish `bind:*` install path (A2): `data-bind-*` → registered
   `Effect`. (§2.3)
2. Project Vault V2: disk watcher + Explorer panel. (§2.2)
3. Canvas hit-test → facet-template composite-id selection. (§2.1)

→ Acceptance: open a folder, see files as data, drag a list block,
bind it to a collection by clicking, edits round-trip.

**Phase B — Make the shell maintainable enough to move fast.**
4. Decompose `prism-shell/src/state.rs` into `prism-shell/src/app/`
   (incremental, behaviour-preserving, test-pinned).
5. Split `ui_lower.rs` / `ui_resolver.rs` along their natural seams
   (container/text/image helpers; tag-dispatch vs. dynamic dispatch).

→ Acceptance: no single shell module > ~1500 lines; tests green
throughout.

**Phase C — Dev environment to fullstack-credible.**
6. IDE Phase 2: real project tree (folders, rename, drag).
7. IDE Phase 3: diagnostics panel (resolve the femtovg
   underline question — gutter markers vs. squiggle primitive).
8. IDE symbol index + jump-to-definition + find-in-files.

→ Acceptance: author a Luau error, see it inline + in a panel, jump to
the symbol, find all refs.

**Phase D — Deploy + collaborate.**
9. Project Vault V3 (folder hierarchy + thumbnails).
10. Phase 6: mobile target + `cargo-packager` + `self_update`.
11. Wire presence: host event-router → `PresenceService` consuming
    probes; live cursors/selections on the canvas.

→ Acceptance: package a signed desktop build that self-updates; two
users co-edit a page with live cursors.

## 4. Cross-references

- Cleanup/migration reconciliation: `docs/dev/state-of-prism.md`
- Substrate roadmap (Tier 1–3): `docs/dev/prism-cross-cutting-systems.md`
- Live UI punch list: `docs/dev/ui-migration-followups.md`
- IDE surface: `docs/dev/ide-mode-plan.md`
- Authoring vision: `docs/dev/prui-luau-fusion.md`,
  `docs/dev/composable-builder-plan.md`
- Data/backend story: `docs/dev/project-vault.md`,
  `docs/dev/data-template-system.md`
- Locked DSL/renderer decision: `docs/adr/008-clay-prism-ui-dsl.md`
