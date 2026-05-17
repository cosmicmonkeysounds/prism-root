# Prism cross-cutting systems — the substrate for a great user / dev experience

> Companion to `prui-luau-fusion.md` (the authoring surface),
> `dioxus-inspiration.md` (the reactive substrate), and
> `clay-migration-plan.md` (the post-Slint runtime). Those docs each
> own a vertical slice. **This doc owns the horizontals** — the
> systems that, if missing or half-built, make every vertical feel
> worse than it should.
>
> The fusion plan deliberately deferred a set of "families" rather
> than faking them (Wave I external tooling, the reactive substrate,
> the shared animator, the coroutine scheduler, the hot-reload
> wiring). This document collects those, plus the systemic concerns
> that were never any single wave's job, into one prioritized map so
> the next phase of work is a deliberate sequence, not a pile of
> TODOs.

**Status:** strategy draft (2026-05-16). Authoring-surface Waves A–I
of `prui-luau-fusion.md` are runtime-complete; what remains is
*substrate*, and substrate is cross-cutting by definition.

---

## 1. The thesis

Prism's authoring story is now strong: one source, three
projections, typed seams, bounded render. But authoring is only the
*input*. The experience a user or dev actually feels is dominated by
**what happens after they type**:

- the loop between an edit and seeing it (hot-reload),
- whether a mistake is caught at edit time or 3 screens later
  (diagnostics / types),
- whether the UI updates the changed thing or re-walks the world
  (reactivity),
- whether motion feels designed or mechanical (animation),
- whether async/IO is a first-class citizen or a footgun (suspense),
- whether the author can *see* what the runtime is doing
  (inspector / probes),
- whether a fragment can be trusted with the capability it's given
  (sovereignty).

Every one of these is cross-cutting: it touches PRUI, PRSS, Luau, the
runtime, and the host simultaneously. None of them is "a feature."
They are the substrate. This doc ranks them by **leverage** —
how much the median user/dev experience improves per unit of work —
and sequences them by dependency.

---

## 2. Leverage map

| System | User/Dev impact | Effort | Blocked by | Tier |
|---|---|---|---|---|
| **Reactive substrate completion** | Huge — every interaction feels instant; ends full-tree re-walks | High | nothing (Dioxus phases 0–2 landed) | **1** |
| **Hot-reload patch pipeline** | Huge — sub-second edit→see loop, state preserved | Medium | reactive substrate (state-preserving patch) | **1** |
| **Type / diagnostics toolchain** | Huge — errors at edit time, completion-driven authoring | Medium | external `luau-analyze` binary | **1** |
| **Effect-driven animator** | High — motion stops being linear-only | Medium | reactive substrate (Effect scheduling) | **2** |
| **Async / suspense scheduler** | High — IO-driven UI without footguns | Medium | coroutine pool + BlockInvalidator resume | **2** |
| **Inspector / DevTools surface** | High — "see what the runtime is doing" | Medium | probe event-router wiring | **2** |
| **Capability enforcement at the Luau seam** | High — sovereignty is a core promise | Medium | PrismContext capability matrix | **2** |
| **Codegen breadth + CI gate** | Medium — refactor safety, autocomplete | Low | nothing | **2** |
| **Cross-document macro/dialect registry** | Medium — real component-library reuse | Low | `.prism.json scripts.*` roots | **3** |
| **Error-surface UX (source-mapped, cross-projection)** | Medium — trust + speed of debugging | Medium | spans through the lowering pipeline | **3** |
| **Scaffolding breadth (`prism new` beyond widget)** | Medium — onboarding | Low | nothing | **3** |
| **Render-budget telemetry** | Medium — perf stays a property, not a hope | Low | probe stream | **3** |

Tier 1 = do next, gates the most downstream value. Tier 2 = high
value, mostly unblocked once Tier 1 lands. Tier 3 = real but
independent; can land opportunistically.

---

## 3. Tier 1 — the gating substrate

### 3.1 Reactive substrate completion

**What.** `dioxus-inspiration.md` Phases 0–2 + the Phase-8 batch
landed: `Signal`/`Memo`/`Effect`, `Owner`, `DirtyQueue`,
`ReactiveContext`, `Atom` rebuilt on signals, `CrdtSync` →
reactive bridge. **Not** landed: Phase 3 (full render-scope wiring —
only walk dirty subtrees, reuse cached `UiNode`s for the rest),
Phase 4 (reactive props end-to-end), Phase 5 (Luau-side reactive
`prism.state`/`prism.derive` are *identity/eager* stubs today — they
do not actually subscribe), Phase 6 (`#[daemon_fn]` reactive body),
Phase 7 (network signals), Phase 8 (SSR signals).

**Why it matters.** This is the single highest-leverage system. Today
any signal write drives a full `render_tree` re-walk (the queue's
non-emptiness, not its contents, triggers the walk — see
`prism-shell` `RenderScope`). Every other interactive feature
(`prism.state`, computed PRSS, `effect:`, `<suspense>`, animations)
is currently "correct but not incremental." Making the substrate
*selective* makes the entire product feel instant and makes the
authoring primitives the fusion plan shipped (Wave A `prism.state`,
Wave F computed PRSS) deliver their promise instead of re-evaluating
the world.

**What's needed.** Phase 3: per-subtree cache keyed by NodeId, walk
only dirty NodeIds, splice cached subtrees for the rest (the
`BlockInvalidator` scaffold already runs every block in its own
reactive context — this is a *pure addition*). Phase 5: make
`prism.state` return a real reactive table whose field reads
subscribe the surrounding block context and whose writes mark that
block's NodeId dirty; `prism.derive` a real memo. Per-class PRSS
invalidation (fusion F.3) falls out of the same machinery.

**Phase 5 status — ✅ landed 2026-05-16.** `prism.state` /
`prism.derive` in `prism-ui-runtime::luau_scope` are now real
reactive primitives, not identity/eager stubs. `prism.state(t)`
returns a metatable proxy backed by one `Signal<Value>` per entry
(allocated against a per-Lua-state `Owner` in the VM's app-data,
mirroring the proven `prism-daemon::luau_reactive` slot pattern —
shared `lua_value_to_json` lives once in `prism_core::luau_reactive`,
no duplicate). Field reads route through `Signal::get` so they
subscribe whatever `ReactiveContext` is current — i.e. the block's
per-NodeId `BlockInvalidator` ctx during lowering — and a write
(`state.x = …`, run via the new `LuauScopeFrame::exec_action`
statement seam) calls `Signal::set`, marking that block's NodeId
dirty through the existing render-scope dirty queue. `prism.derive`
is a real `Memo`, recomputing only when a signal it read changes.
Shape-faithful: array-shaped `prism.state { {..}, {..} }` (the
common list form many docs rely on) rebuilds a JSON array so
`for t in tasks` keeps iterating; records rebuild objects.
Non-reactive locals still take the cheap snapshot path unchanged.
Pinned by `luau_scope::tests::{prism_state_is_reactive_and_writes_through_proxy,
prism_state_read_subscribes_and_write_marks_dirty,
prism_derive_memoises_and_recomputes_on_state_change,
plain_array_local_still_snapshots_after_phase5,
closure_call_still_works_after_phase5}` + the full 370-test suite.
**Phase 3 status — ✅ landed 2026-05-16.** The shell no longer
re-walks the whole tree on a reactive redraw. `shell.rs` drains the
`RenderScope` `DirtyQueue` and, when the redraw is *purely* reactive
(no event / animator / `FRAME_DIRTY_SENTINEL`), passes the dirty
NodeId set into `render_tree_with` → `LowerScope::with_dirty_nodes`.
`interpret::lower_element` then reuses the cached subtree of any
id'd element whose id is not dirty *and* whose cached subtree holds
no dirty descendant (`subtree_has_dirty`); only dirty NodeIds and
their ancestor paths re-lower, the recursion pruning at the highest
clean boundary. The existing Wave 14.3 `MemoCache` is the cache
substrate (reused, not duplicated — id resolution consolidated into
`resolve_element_id`); full passes populate it per-id so the next
reactive frame can splice. **Strictly non-lossy:** a `touched` set
records every (re)lowered id, and the shell falls back to a full
walk that same frame if any drained dirty id mapped to no element
(unknown/renamed id) — worst case equals the pre-Phase-3 behaviour,
a correct splice never triggers it. Pinned by
`interpret::tests::phase3_dirty_set_splices_clean_subtree_relowers_dirty`
+ the full 371-test prism-ui-runtime suite + prism-shell lib (the 3
`app_registry` failures are pre-existing, from unrelated
uncommitted WIP — verified by stash bisect).

Remaining in 3.1: per-class PRSS invalidation (fusion F.3), which
now rides the same machinery — a class read inside a block already
subscribes that block's NodeId via Phase 5, so a PRSS literal swap
just needs to mark the dependent NodeIds dirty and Phase 3 splices
the rest.

**Sequencing.** First. It unblocks Tier 2's animator (Effect
scheduling), suspense (resume notification), and the hot-reload
patch (state-preserving requires the reactive graph as the unit of
preservation).

### 3.2 — ✅ PRSS hot-reload landed 2026-05-16

The shell's `hot_reload` watcher gained a `ReloadTarget::Stylesheet
{ app_id }` arm. `run_with_hot_reload` now owns a persistent
`StylesheetWatcher` (the `PrssFingerprintCache` wrapper already in
`render.rs`); a `.prss` save is classified (`NoChange` /
`LiteralOnly` / `Structural` / `ParseError`), and a valid change
reinstalls the host or per-app sheet in place and marks the render
scope dirty — no kill-and-respawn. `build_watch_specs` watches
`ui/app.prss` + each `apps/<id>/shell.prss`. A parse error keeps the
last good sheet (the watcher retains it) so a mid-edit save never
blanks styling. Pinned by
`hot_reload::tests::stylesheet_targets_coalesce_independently_of_skeletons`
+ the existing `StylesheetWatcher` literal/structural tests. The
**Per-class selective invalidation — ✅ landed 2026-05-16.** The
class→NodeId dependency edge the reactive substrate can't infer
(classes resolve from the plain `StyleSheet`, not a `Signal`) is now
an explicit `interpret::ClassUsage` table, populated during lowering
in `apply_container_attributes` for every id'd container that
resolves a PRSS class, threaded via `RenderCaches.class_deps` and
cleared on each full pass. The §3.2 `.prss` consumer reads it: a
`PrssChange::LiteralOnly` patch with `PrssLiteralOwner::Class { name }`
marks *only* `class_deps.nodes_for(name)` dirty (Phase 3 splices the
rest); token-bucket patches + structural changes cascade broadly via
`FRAME_DIRTY_SENTINEL`. Non-lossy fallback: a patched class with no
recorded NodeId (not yet rendered / anonymous) also falls back to
the sentinel so the edit is never dropped. Pinned by
`interpret::tests::class_usage_records_class_to_nodeid_when_collector_installed`.
The `.prui` `PruiDocCache` literal-slot-poke / script-frame-reload
path is the one remaining deeper refinement (the `.prss` half —
coarse *and* per-class selective — is done).

### 3.2 Hot-reload patch pipeline

**What.** The fingerprint *substrate* is now complete:
`prism-ui-build` ships `template_hash`/`prss_hash` and, as of Wave
H.4, `prui_doc::PruiDocCache` — a `.prui` decomposed into
independently-keyed virtual files (`{path}#script:{i}`,
`{path}#style:{i}`) so the narrowest reload wins. The `subsecond`
anchor exists behind the `prism-shell` `hot-reload` feature. **Not**
wired: the dev-loop consumer that reads `PruiDocChange` and applies
the smallest patch (literal slot poke / script-frame reload / PRSS
literal-only swap) instead of a kill-and-respawn; the
`subsecond::register_handler` hookup; state-preserving HMR for
`prism.state` (fusion open question 2 — Svelte-style source-position
keying).

**Why it matters.** The edit→see loop is the most-repeated dev action
in existence. Today a `.prui` save respawns the shell. With the
patch pipeline, a label edit is a slot poke (microseconds, state
preserved), a `<script>` edit reloads one Lua frame, a PRSS value
edit is a literal-only swap. This is the difference between Prism
feeling like a live medium and a compile-run loop.

**What's needed.** A `prism-cli` dev-loop consumer of `PruiDocCache`
+ `PrssFingerprintCache` that maps each `PruiDocChange` arm to a
runtime patch op; the `subsecond` patch-emit cargo invocation; the
state-migration keying for `prism.state` (depends on 3.1's reactive
state being real).

**Sequencing.** Right after / alongside 3.1. The fingerprint half is
done; the patch-application half wants the reactive graph as the
preservation unit.

### 3.3 Type / diagnostics toolchain

**Status (2026-05-16).** ✅ `prism lint --types` landed: it locates
`luau-analyze` (`LUAU_ANALYZE` override → `PATH`), runs it in strict
mode over every `.luau` source in the workspace, and returns its exit
code as a genuine CI gate. When the binary is absent it prints an
actionable install hint and *skips* (exit 0) — honestly inert, never
a fake pass, per §7. Remaining: the `PrismUiSyntaxProvider` PRUI
`{expr}`-slot → synthetic-Luau rewrite (so slot typos are caught too)
and `--!strict` `load_*` defaulting; both ride this same CLI seam and
are the documented next step.

**What.** Fusion Wave I deliberately did **not** fake this: the
value bridges (`prism.scope`, `{lua=…}` evaluator, signal
registrations) are landed and ready to be type-checked, but the
PRUI-slot rewrite-and-typecheck, signal-payload narrowing, and the
Inspector type annotations still need wiring on top of the now-landed
`prism lint --types` external-toolchain seam.

**Why it matters.** "Diagnostics-first authoring" (Roc/Grain feel) is
a stated design goal (fusion §6). Without it, the typed seams are a
latent capability nobody experiences — a typo in `state.fliter` is a
runtime `nil`, not a red squiggle. This is the difference between the
type system being a *promise* and a *product*.

**What's needed.** Bundle/locate `luau-analyze`; the
`PrismUiSyntaxProvider` rewrite pass (PRUI `{expr}` slots → synthetic
Luau, diagnostics mapped back to PRUI ranges — fusion §6.2 describes
the mechanic); `prism lint --types` CLI subcommand running it over
every `.luau` + `<script>` + rewritten slot; `--!strict` defaulting
(fusion I.5 — a one-line `load_*` flag, gated on this).

**Sequencing.** Independent of 3.1/3.2; can run in parallel. It is
Tier 1 because its leverage is comparable and it is *not* blocked by
the reactive work.

---

## 4. Tier 2 — high value, unblocked once Tier 1 lands

### 4.1 Effect-driven animator — ✅ Luau easing landed 2026-05-16

The shared `prism_ui_runtime::animator::Animator` substrate (observe
/ apply / tick) landed in Wave 14.3. **§4.1 closes the remaining
gap: custom Luau easing.** `transition:easing={\fn(t) … end}` is
*sampled at lowering time* (where the per-document Lua frame is live)
into a comma-joined LUT and round-tripped through the
`data-transition-easing` semantic attr; `animator::parse_easing`
decodes it into `Easing::Lut`, which the per-frame `sample` path
interpolates linearly. Net: the animator stays **Lua-free per
frame** (zero `call_closure` on the frame clock) and never holds a
Lua handle past the lowering pass — a closure that outlives the VM
can't dangle. A named keyword (`ease-in` / `ease-out` /
`ease-in-out` / `ease` / `linear`) round-trips verbatim; an absent
attr is byte-identical to the prior linear behaviour. One
`data-transition-easing` governs every animated prop on the
container (entry / mid-life delta / out), mirroring CSS's
per-element `transition-timing-function`. Springs/elastic/custom are
now one Luau function. Pinned by
`animator::tests::{parse_easing_recognises_keywords_and_lut,
lut_easing_interpolates_between_samples,
observe_applies_custom_easing_from_attr}` +
`interpret::tests::{transition_easing_keyword_round_trips,
transition_easing_closure_samples_to_lut}`.

Remaining (separate from easing): the `at:<time>` multi-stop
*timeline* interpolation (G.3's keyframe-sequence case) is still the
data-round-trips-only path; the easing backplane it would use is now
in place.

### 4.2 Async / suspense scheduler — ✅ landed 2026-05-16

`<suspense>`/`<fallback>` swapped correctly at lowering time (fusion
D.3); **§4.2 lands the coroutine scheduler + reactive resume.**
`prism.objects:query_async(producer [, boundary])` allocates a
backing `Signal<JsonValue>` seeded with `{ tag = "Pending" }` (the
exact marker `subtree_has_pending` trips the `<fallback>` on),
registers a `coroutine` wrapping `producer` into a per-document
`SuspenseScheduler` (a `Vec<SuspenseTask>` in the Lua app-data,
alongside the Phase-5 `Owner` — a query can't outlive its
document), and returns *immediately* with a reactive proxy. The
binding harvester binds it like a memo (`__prism_async` id → signal)
so a read resolves live through `Signal::get`, **subscribing the
awaiting block's `BlockInvalidator` context**.

`LuauScopeFrame::drain_suspense` resumes every in-flight coroutine
**exactly once per render tick** (per-task fairness — no boundary's
slow query starves another): a *return* resolves the query by
writing the value through the backing signal, which marks the
subscribed `<suspense>` boundary's NodeId dirty — **the resume
notification rides §3.1's reactive dirty queue with zero extra
wiring**, so the next frame re-lowers and `subtree_has_pending`
picks the primary subtree; a `coroutine.yield()` keeps the task for
the next tick (multi-tick await — the shape a relay/daemon IO
wrapper takes); an error resolves to `{ tag = "Error" }` so a
boundary never suspends forever. The shell drains the retained
per-document frame each render tick next to the animator pass and
folds "still pending" into the same frame-redraw bit a running
transition uses. Bounded by construction: ≤ one resume per task per
tick, no nested scheduling (design principle 1). Pinned by
`luau_scope::tests::{query_async_starts_pending_then_resolves_on_drain,
query_async_multi_tick_yield_stays_pending,
query_async_resolve_marks_subscribed_reader_dirty}` +
`interpret::tests::suspense_shows_fallback_for_unresolved_query_async`.

Impact: `prism.objects:query_async` is first-class — no manual
coroutine wrapper at the call site. Wiring a concrete federated /
relay / daemon IO *source* into a `query_async` producer (the
coroutine `yield`s until the host feeds the result back) is the
documented host follow-up — the runtime substrate it rides is now
complete (open question 3 resolved).

### 4.3 Inspector / DevTools surface

Probes (`probe:` namespace, `prism.probes:on`) register and fire in
the Luau frame (fusion G.2) but the host event-router does not yet
fire probes off a `data-probe-*` hit, and there is no panel. Needed:
the event-router wiring (same family as suspense resume) + an
Inspector panel that streams the probe bus, shows each binding's
declared type next to its value (rides 3.3), and supports
filter/replay/snapshot. Impact: delivers the "one debugging surface"
promise; printf-debugging stops leaking `data-foo` into production.

### 4.4 Capability enforcement at the Luau seam — ✅ first cut landed 2026-05-16

`LuauScopeFrame::from_modules_with_requires_and_policy` threads a
`prism_core::identity::trust::SandboxPolicy` (the matrix already in
`prism-core`) through the per-document seam. The reachable,
capability-bearing surface today is **authoring registration**:
`prism.macro` / `prism.dialect` mutate the document's component
vocabulary, so a fragment whose `PrismContext` lacks `crdt:write`
(facet-resolver / read-only role) is denied with a bounded sandbox
error — a bundled/imported module can no longer silently extend the
document with the host's full authority. The doc's role ladder maps
onto the existing capability vocabulary with no new enum
(`ScopeCaps::from_policy`). `policy = None` keeps full trust for
host-internal fragments, so every current call site is unchanged.
Remaining (rides Phase 5): once `prism.state` writes / `signal:set`
exist at this seam, gate them on the same `CrdtWrite` bit.

Design principle 4 of the fusion plan: a Luau fragment inherits the
surrounding document's `PrismContext` capability set (facet-resolver
= read-only, signal-handler = read/write, admin = everything). Today
only the Wave-A baseline ships: `lua.sandbox(true)` freezes stdlib +
`prism`/`tokens`. The capability *matrix* (per-fragment scoping tied
to `PrismContext`, the `identity::trust` sandbox policy already in
`prism-core`) is not wired through `LuauScopeFrame`. Impact:
sovereignty is a core Prism promise; a bundled/imported module
running with the document's full capability is the gap.

### 4.5 Codegen breadth + CI gate — ✅ landed 2026-05-16

`prism codegen luau-types` now emits a `Prism` namespace stub plus
`PrismReactive` / `PrismProbes` / `ReactiveSignal` / `ReactiveMemo`
in `prism-ui-runtime::luau_types` (it owns `install_prism_helpers`,
so the stub lives next to the surface it documents — no duplication).
The drift gate is behavioral, not textual: `luau_types::PRISM_GLOBAL_MEMBERS`
is the single source of truth, and `luau_scope::tests::prism_global_matches_stub`
boots a real `LuauScopeFrame`, enumerates the live `prism` table with
`pairs`, and asserts it equals the stub set — a runtime surface added
without a stub (or a stale stub) fails CI. Per the doc's own
anti-faking principle, `prism.on_signal` / `prism.tokens` were *not*
stubbed: they aren't exposed on `prism` at runtime (`tokens` is a
top-level global), and a stub for a non-existent member trains authors
to distrust completion. `prism.scope` / `prism.reactive` are stubbed
as optional because they're host-/daemon-seeded conditionally.

---

## 5. Tier 3 — real but independent

- **Cross-document macro / dialect registry** (fusion open Q6):
  workspace-global macros/dialects via `.prism.json scripts.*`
  roots, instead of per-document `<script>` registration. The H.1
  remainder (per-app `prism://` root registration from the manifest)
  is the same wiring. The E.3 bundled-dialect injection seam
  (`LowerScope::with_builtin_scripts`) is the runtime half; this is
  the workspace-config half.
- **Error-surface UX.** Recoverable parse errors exist per-projection
  but are not yet source-mapped *across* projections (a type error in
  a `{expr}` slot should point at the PRUI range, a PRSS `{lua=…}`
  failure at the `.prss` line). Needs spans threaded through the
  lowering pipeline and a unified diagnostic sink.
- **Scaffolding breadth.** `prism new widget` landed (Wave H.5).
  `prism new app` / `prism new dialect` / `prism new page` on the
  same generator are the obvious follow-ups; onboarding leverage.
- **Render-budget telemetry.** The render walk is bounded by
  contract; a probe-fed budget meter (nodes walked, Lua calls,
  layout passes per frame) keeps it a *measured* property, not a
  hoped-for one. Rides the probe stream (4.3).
- **`require` deep-graph maturation.** Tier-3 `require` landed
  (H.7); the documented nuance — two different literal spellings of
  the same file evaluate twice — and a module fingerprint-cache for
  hot-reload are the maturation items.

---

## 6. Recommended sequence

```
Phase α  (parallel, no cross-deps):
  ├─ 3.1 Reactive substrate (Dioxus Phases 3,4,5)   ← biggest lever
  └─ 3.3 Type/diagnostics toolchain (luau-analyze)   ← independent

Phase β  (gated on α.3.1):
  ├─ 3.2 Hot-reload patch pipeline
  ├─ 4.1 Effect-driven animator
  └─ 4.2 Async/suspense scheduler

Phase γ  (gated on α.3.3 / probe wiring):
  ├─ 4.3 Inspector / DevTools
  ├─ 4.4 Capability enforcement
  └─ 4.5 Codegen breadth + CI gate

Phase δ  (opportunistic, independent):
  └─ Tier 3 items as they unblock authors
```

The ordering principle: **3.1 first because almost everything
incremental rides it; 3.3 in parallel because it is unblocked and
equally high-leverage; everything else falls out of those two plus
the probe event-router wiring.**

---

## 7. Explicitly out of scope here

- Anything with a single owning vertical doc keeps its home:
  per-construct authoring semantics live in `prui-luau-fusion.md`;
  the reactive primitive design lives in `dioxus-inspiration.md`;
  the post-Slint runtime shape lives in `clay-migration-plan.md`.
  This doc *sequences and connects* them; it does not restate them.
- New authoring surface. The fusion plan's twelve mechanisms are
  runtime-complete; the work ahead is substrate, not more syntax.
- Faking external tooling. The type toolchain (3.3) requires a real
  `luau-analyze`; a stubbed typechecker is worse than none (it
  trains authors to distrust diagnostics). It ships when it is real.

---

## 8. One-line summary

Authoring is done; **the experience now lives in the substrate** —
reactivity, hot-reload, and diagnostics first; animation, async,
inspector, and capability second; everything else opportunistically.
Build 3.1 and 3.3 next and most of the rest becomes a downhill roll.
