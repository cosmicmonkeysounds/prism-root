# Dioxus-Inspired Reactive Overhaul

**Status:** Phases 0–10 landed (see status block below).
**Date:** 2026-05-11
**Owner:** JJM
**Supersedes:** the earlier "watch list" version of this doc

## Status snapshot (2026-05-11)

| Phase | What | Status |
|---|---|---|
| 0 | Workspace scaffold + `prism-core::reactive` re-exports | ✅ done |
| 1 | `Signal` / `Memo` / `Effect` / `ReactiveContext` / `Owner` | ✅ done |
| 2 | `kernel::atom` + `crdt_sync` on top of `reactive::Signal` | ✅ done |
| 3a | `render_tree` runs inside frame-level `ReactiveContext` | ✅ done — `RenderScope::run_in_render_pass` |
| 3b | Per-block `lower_ui` reactive contexts | ✅ done — `BlockInvalidator` + per-NodeId cache |
| 4a | `ActionKind::Bind` → `Effect` install on document load | ✅ done — `DocumentBindings::install_for` |
| 4b | `Node::props` → `ReactiveProps` migration | ✅ done — `LowerCtx::with_bindings` + `ctx.prop_*` reactive accessors + `NodeMutator` single-seam write path |
| 5 | Luau `Signal::read`/`write` UserData surface | ✅ done — `prism-core::luau_reactive` |
| 6 | `#[daemon_fn]` + `RemoteSignal<T>` + IpcInvoker | ✅ done — `prism-daemon::IpcInvoker` over postcard-on-interprocess |
| 7 | `FederatedSignal` / `PeerSignal` / `RelaySignal` + `LocalHub` | ✅ done (trait seam + production-shape fan-out hub; per-transport wire integration is host-side) |
| 8 | `SsrCache` wired into prism-relay routes | ✅ done — `SsrWorker` single-threaded worker + `portal_detail` cache hit/miss path |
| 9 | `subsecond` hot-reload anchor in shell | ✅ done — `--features hot-reload` wraps render walk; patch pipeline integration deferred |
| 10 | `.prism-ui` template hash fast-path | ✅ done — `FingerprintCache::observe` returns `TemplateChange::{NoChange,LiteralOnly,Structural,…}` |

### Phase 4b shape (as landed)

Disk format unchanged — `Node::props: serde_json::Value` is still the
serializable form. The reactive surface comes from three cooperating
DI-shaped pieces:

- **`LowerCtx::with_bindings(&DocumentBindings)`** (builder side) —
  one slot, propagates through `lower()` / `lower_as()` like
  `with_block_invalidator` does. When wired, every
  `ctx.prop_str(node, k)` / `ctx.prop_bool` / `ctx.prop` /
  `ctx.prop_signal` read routes through
  `bindings.props_for(node.id, &node.props).signal(k)`, subscribing
  the current per-block reactive context. The free
  `prop_str` / `prop_string` / `prop_bool` helpers are gone — the
  three call shapes consolidated onto `LowerCtx` so blocks compose
  uniformly across the reactive and headless paths.
- **`NodeMutator` builder** (`prism-builder/src/mutator.rs`) — the
  single seam for every `Node::props` mutation across prefab
  materialisation, facet scalar resolution, facet variant rule
  evaluation, and the shell's `AppState::set_node_prop`.
  `NodeMutator::with_bindings(b).write(&mut node, k, v)` writes the
  canonical JSON *and* pokes the matching reactive bag — every
  subscriber wired by `ctx.prop_*` wakes on the next dirty drain.
  The pre-existing `apply_prop_to_node` and four open-coded
  `Value::Object(ref mut map).insert(...)` patterns retired in
  favour of this one builder.
- **`CanvasSlot::bindings`** (shell side) — one
  `prism_builder::DocumentBindings` lives per canvas, threaded into
  `LowerCtx::with_bindings(..)` from the render walk and into
  `NodeMutator::with_bindings(..)` from the prop-write seam. Block
  bodies that read `ctx.prop_str(node, "label")` subscribe; a later
  `state.set_node_prop("n", "label", "Hi", reg)` writes through the
  canvas's bag and the BlockInvalidator marks `"n"` dirty next
  frame — zero per-block plumbing required.

Three new accessors on `LowerCtx`: `prop(node, key) -> Value`
(subscribing JSON read), `prop_signal(node, key) -> Option<Signal<Value>>`
(raw signal for memos and cross-block effects), and the typed
`prop_str` / `prop_bool` shortcuts. Old free functions deleted; the
106-call-site sweep across `prism-shell/src/components/*` and
`prism-builder/src/starter.rs` rerouted in one pass.

The previous draft of this document was a ranked shopping list of
Dioxus subpackages worth borrowing. Useful, but it dodged the real
question. Prism Builder works — blocks register, the document
renders, `on:click="emit save"` fires the right connection — but
the system underneath is two unrelated halves stapled together,
and you can feel it the moment you try to do anything ambitious.

This document replaces the shopping list with an honest plan.

## 1. Diagnosis: what is actually wrong

Today Prism has **two unrelated reactive systems** wearing the
same word "signal":

1. **`prism-core::kernel::atom`** — a real fine-grained reactive
   cell. `Atom<T>::set` checks `PartialEq` and notifies
   subscribers; `select(store, sel)` projects `Store<S>` into a
   `SharedAtom<T>` that only fires when the projection changes.
   `kernel::crdt_sync::CrdtSync` already bridges Loro mutations
   into per-object / per-edge atoms. This is the closest thing
   Prism has to a Dioxus `Signal<T>` — but every subscriber is
   wired by hand via `atom.subscribe(callback)`. There is no
   reactive context. Reading does not subscribe. The render walk
   does not run inside any tracking scope. Atoms are an
   *opt-in* fine-grained system that nobody has opted into.

2. **`prism-builder::signal`** — a static event-routing layer.
   Components declare a `Vec<SignalDef>`; the document carries a
   `Vec<Connection { source_node, signal, target_node, action }>`;
   `dispatch_signal(event, connections)` is a pure function that
   produces a `Vec<DispatchResult>`. The shell's `SignalsService`
   runs that dispatch and mutates `state.canvas.document` directly
   per result. This is the layer authors interact with — the one
   that lights up when you wire a click to a property change in
   the UI. But it is *not reactive at all*. It is a switchboard.
   Nothing reads from it derives a value; nothing in it propagates.

The result of having both, with no relationship between them:

- **CRDT changes don't drive UI re-renders.** `CrdtSync` updates
  atoms; atoms have no subscribers from the render side; the
  render side polls the whole document at coarse cadence.
- **There is no derived state.** No memos, no computed properties.
  Every property panel reads `props["x"]` straight from the JSON.
- **Authored signals can't read state.** `Connection::SetProperty`
  takes a *literal* `Value`. You cannot say "set the modal's
  `title` to the current selection's name" because there is
  nowhere in the model for "the current selection's name" to
  exist as a first-class read-able thing.
- **`on:click="emit save"` is fine but `on:click="bind to save"`
  is unspeakable.** There is no read/write of reactive state from
  the inline-action grammar, because reactive state has no surface
  the grammar can reach.
- **Luau handlers are dead-ends.** `ActionKind::Custom { handler }`
  runs Luau code, but Luau can't `read()` or `write()` anything
  reactive — it sees JSON props and mutates JSON props.
- **No render-scope.** `lower_ui` walks the document and produces
  a `Surface` tree; there is no notion of "this node depends on
  these signals, invalidate just this subtree when they change."

Prism Builder has the bones of a real declarative tool but no
nervous system. The plan below is the nervous system.

## 2. Target philosophy

> **Everything reactive. One graph. One subscription model. One
> transport-agnostic API.**

A `BuilderDocument` node is reactive. A Loro container is
reactive. A CRDT-backed object is reactive. A daemon-published
value is reactive. A computed property is reactive. **A peer's
cursor is reactive. A relay's federation feed is reactive. The
SSR-rendered HTML is reactive output.** A user-authored
"connection" is just a declarative façade that compiles down to an
`Effect`. The render walk runs inside a reactive context, so reads
auto-subscribe and writes auto-invalidate. Luau and Rust call the
same `Signal::read` / `Signal::write` API. **The daemon, the
shell, the relay, and a remote peer all call the same
`Signal::read` / `Signal::write` API.**

This is the Dioxus insight worth taking — not its virtual DOM, not
its `rsx!`, not its scheduler. The insight is:

**a single thread-local `ReactiveContext::current()` plus
`Copy + 'static` reactive handles is enough to unify rendering,
state, async, IO, codegen, and — for Prism, going further than
Dioxus does — *the network*, behind one mental model.**

Prism already has the storage half of that (atoms, `CrdtSync`),
the network half (the 17 relay modules, federation, presence,
signaling), and the SSR half (`prism-relay` rendering
`BuilderDocument` to HTML through `lower_semantic_html`). What it
lacks is the *tracking* half (reactive contexts, automatic
subscription), the *propagation* half (effects, dirty queues,
render invalidation), and the *transport* half (one `Signal<T>`
type that knows how to deliver a notification locally, across
IPC, or to a peer over a relay).

### 2.1 The scope ladder

A `Signal<T>` is a reactive cell with a **scope**. The
read/write/subscribe semantics are identical; only the transport
that delivers the notification differs.

| Scope | Transport | Carrier in Prism today | Latency target |
|---|---|---|---|
| **Local** | thread-local subscribers HashSet | `kernel::atom::Atom<T>` listeners | < 1 µs |
| **Process** | `Store`-style synchronous bus | `kernel::store::Store::subscribe` | < 10 µs |
| **IPC** | `interprocess` + `postcard` frames | `prism-daemon` stdio channel | < 100 µs |
| **Federated** | relay envelope over WebSocket | `network::relay` `RelayEnvelope` | 1–100 ms |
| **P2P** | WebRTC data channel | `signaling` + `presence` modules | 10–100 ms |
| **SSR** | HTTP response body | `prism-relay::ssr_routes` `lower_semantic_html` | per-request |

All five rows want the same `Signal<T>::read()` /
`Signal<T>::write(v)` / `effect(|| ...)` user-facing API. The
*scope* is a property of how the signal is constructed —
`Owner::insert(v)` for local, `Owner::insert_remote(addr, v)` for
federated, etc. — and possibly a marker type parameter
(`Signal<T, Local>` vs `Signal<T, Remote>`) when the transport
needs to surface error semantics the local form does not have
(network failure, peer offline, response timeout).

This is why this work is *not* "a Dioxus signals port." Dioxus
stops at the IPC boundary with `#[server]`. Prism keeps going,
because the whole point of the framework is that
**user-authored intent crosses machine boundaries without
changing shape**.

## 3. The Dioxus pieces we take, ranked

After studying `dioxuslabs/dioxus` (`packages/signals`,
`packages/generational-box`, the tutorial through 0.7), four
primitives carry their weight. The rest is virtual-DOM machinery
that does not apply to a retained-mode `Surface` runtime.

### 3.1 `generational-box` — vendor or depend (highest leverage)

A typed handle into a shared arena of `RefCell<Box<dyn Any>>` slots
with a generation counter. The handle is bitwise `Copy` because it
is just `(slot_index, generation)`. An `Owner` collects slots and
reclaims them on drop. Stale handles return `BorrowError` instead
of UB.

This is *the* enabling primitive. Without it every reactive cell is
`Rc<RefCell<…>>` or `Arc<RwLock<…>>` and the ergonomics collapse —
you cannot hand a `Signal<T>` to an event handler, an async task,
and a child block without ceremonial cloning. With it, every
reactive cell is `Copy + 'static`, and the surface area of the
plan shrinks by half.

Today `SharedAtom<T> = Rc<RefCell<Atom<T>>>`. That has to go.

**Decision:** depend on `generational-box` as a workspace
dependency. Re-export it from `prism-core::reactive::storage` so
host crates do not import it directly.

### 3.2 `ReactiveContext` — the thread-local that does all the work

```rust
// One thread-local. Everything else falls out.
ReactiveContext::current() -> Option<ReactiveContext>;

// A reactive scope runs a closure, tracks signal reads,
// re-registers subscriptions, and fires the callback on
// mark_dirty.
rc.reset_and_run_in(&mut closure);
```

Every `Signal::read()` queries `ReactiveContext::current()` and, if
present, adds the current context to the signal's `subscribers`
set. Every `Signal::write()` walks that set and calls
`mark_dirty()` on each. There is *no other tracking mechanism* —
no `#[reactive]` annotations, no dependency declarations, no
proxy objects. Just one thread-local and one subscribers set per
signal.

This is the trick. Once you have this, `Memo`, `Effect`,
`Resource`, suspense, and render invalidation are all 50-line
implementations on top.

### 3.3 `Signal<T>` / `Memo<T>` / `Effect` — the public surface

```rust
pub struct Signal<T> { /* GenerationalBox<SignalData<T>> */ }
impl<T> Signal<T> {
    pub fn new(value: T) -> Self;
    pub fn read(&self) -> ReadGuard<'_, T>;     // subscribes
    pub fn peek(&self) -> ReadGuard<'_, T>;     // does not
    pub fn write(&self) -> WriteGuard<'_, T>;   // notifies on drop
    pub fn set(&self, v: T);
}

pub struct Memo<T> { /* ReactiveContext + Signal<T> */ }
impl<T: PartialEq> Memo<T> {
    pub fn new(f: impl FnMut() -> T) -> Self;
    pub fn read(&self) -> ReadGuard<'_, T>;
}

pub struct Effect { /* ReactiveContext, kept alive by an Owner */ }
impl Effect {
    pub fn new(f: impl FnMut()) -> Self;
}
```

The `peek`/`read`/`write` split matters and is load-bearing —
prevents accidental write-during-read cycles when a derivation
also needs to inspect its own previous value.

### 3.4 `#[server]`-style codegen — extend `prism-luau-derive`

Dioxus's `#[server]` attribute emits a server-side axum handler
and a client-side stub from one signature. Prism already has
`#[daemon_command]` (server side only). The pattern that closes
the loop is the *type-equal contract*: the call site sees one
function, the macro generates the two bodies. Worth lifting
verbatim — but **generalize the target**:

| Macro variant | Server side runs in | Client stub for |
|---|---|---|
| `#[daemon_fn]` | the daemon sidecar | shell, studio host |
| `#[relay_fn]` | the relay (axum) | shell, studio, any peer |
| `#[peer_fn]` | another peer | relay-routed RPC |

All three are the same codegen pattern with different
serialization (`postcard` over `interprocess`, `postcard` over
WebSocket, `RelayEnvelope` over WebSocket). One proc-macro family,
three transports. This is independent of the reactive substrate
but lives in the same codegen crate and benefits from the same
unifying language.

## 4. The Dioxus pieces we do NOT take

Being explicit about this prevents drift later.

- **`rsx!` and the whole template/diff pipeline.** Its purpose is
  to feed a virtual-DOM `Mutation` stream that backends apply to
  a real DOM. Prism has a retained-mode `Surface` already — there
  is nothing to diff. Hot-reload templates and the literal-patch
  trick are interesting for `.prism-ui`, but they belong in a
  separate effort (`prism-ui-build` codegen) and are not part of
  the reactive overhaul.
- **The VDOM scheduler / `ScopeId` / `Mutation` queue.** None of
  it applies to retained-mode. Replace with: a `DirtyQueue<NodeId>`
  drained per frame.
- **Call-order hook indexing (`use_signal`, `use_memo`).** Hooks
  exist because component functions re-run top-to-bottom every
  tick. Prism blocks are *persistent* — `SpecBlock` instances live
  on the `BuilderDocument`. Reactive state hangs off the block /
  document / kernel, not off a per-render call site.
- **`#[component]` props-struct synthesis.** Prism components are
  CRDT-backed nodes with a richer `FieldSpec` schema than
  `TypedBuilder`. Keep our own.
- **`Routable` enum routing.** Cute but not load-bearing. Apps
  shell navigation is per-app and goes through dock + workspace
  slots. Revisit if URL routing becomes a hard requirement.
- **`Resource` (async memos).** Build later, after the synchronous
  core is solid. `Resource` is `Memo` + a spawned future.

## 5. Naming hygiene — coexistence with the existing "signal"

The word "signal" is taken twice:

- `prism-builder::signal::SignalDef` / `Connection` — **authored
  event channels** (user-facing).
- (proposed) `prism-core::reactive::Signal<T>` — **typed reactive
  cells** (runtime + Luau).

Renaming the builder side wholesale is wide blast radius:
`Component::signals()`, `SignalsService`, `signals.d.luau`, the
authored grammar `emit save`, the property-panel "Signals" tab,
and tests in three crates. Not now.

**Decision: live with both, scope them by module.**

- New reactive primitives land in `prism-core::reactive::*`. Type
  names: `Signal<T>`, `Memo<T>`, `Effect`, `ReactiveContext`,
  `Owner`. The builder's `SignalDef` stays where it is and means
  what it means.
- In docs / `CLAUDE.md` files: when we mean the reactive cell, we
  spell out `reactive::Signal<T>`. When we mean the authored
  event channel, we say "connection signal" or `SignalDef`.
- The end state is convergence: `SignalDef` becomes a sugar layer
  that compiles each authored signal to a hidden
  `reactive::Signal<()>` and each `Connection` to a hidden
  `Effect`. At that point the two names mean the same thing
  underneath and the user-facing word doesn't need to change.

If, after Phase 2 lands, the overlap is more painful than this
section anticipates, rename `prism-builder::signal` → `connection`
and `SignalDef` → `EventDef`. Don't pre-commit.

## 6. Phased plan

### Phase 0 — prep (this PR)

Land the dependencies and scaffold the namespace so subsequent
phases have somewhere to plug into. No behaviour change.

- Add `generational-box` to `[workspace.dependencies]`.
- Add `prism-core::reactive` module with `ReactiveContext` skeleton
  (thread-local + `current()` + `reset_and_run_in` body), an empty
  `Owner`, and a minimal `Signal<T>` placeholder built on a
  `GenerationalBox`. Behind no feature flag — pure-logic
  consumers can use it without dragging anything heavy.
- Re-export `Signal`, `Memo`, `Effect`, `ReactiveContext`, `Owner`
  from `prism-core::lib` so call sites have a stable import path
  before the internals are filled in.
- Note the future renames in the per-module `CLAUDE.md`s where
  the names currently overlap.

### Phase 1 — reactive substrate

Build the minimum viable reactive graph.

- Flesh out `ReactiveContext`: subscribers `HashSet`, `mark_dirty`
  callback, `reset_and_run_in` re-tracking, source `Location`
  carried for diagnostics.
- Real `Signal<T>` with `read` / `peek` / `write` / `set`. Drop
  guards that fire subscriber notification on `Drop` of
  `WriteGuard`.
- `Memo<T>` built directly on `ReactiveContext` + an inner
  `Signal<T>`. PartialEq gating to skip downstream notification
  on equal values.
- `Effect` with `Owner`-scoped lifetime. Effects drop when their
  owner drops. No global registry.
- `Owner` as the lifetime authority. One per `BuilderDocument`
  node, one per `Page`, one for the shell-global scope.
- Tests: signal read/write basics, memo recomputes only when
  inputs change, effect fires once per dirty cycle, owner drop
  reclaims everything.

### Phase 2 — bridge the existing atom layer

`kernel::atom::Atom<T>` and `kernel::crdt_sync::CrdtSync` already
exist and already have the storage + Loro-bridge half of the
problem. We do not throw them away; we promote them onto the new
reactive substrate.

- Re-implement `Atom<T>` *as* a thin wrapper around
  `reactive::Signal<T>`. The public `subscribe(callback)` API
  stays for callers that want explicit listeners (host shells,
  IPC clients); the new path is to read inside a reactive
  context.
- `select(store, sel)` becomes a `Memo<T>`. Same surface,
  reactive guts.
- `CrdtSync::write_object` / `write_edge` continue to update Loro
  first, then notify; the notification now flows through the
  reactive graph instead of one manual `subscribe` callback per
  call site.
- The `SharedAtom<T> = Rc<RefCell<Atom<T>>>` typedef goes away —
  `Signal<T>` is already `Copy + 'static`.

This phase is breaking but bounded: the call sites are
`kernel::crdt_sync`, the relay's collection host module, and a
handful of test fixtures. `cargo check --workspace` is the
safety net (per the project style rule).

### Phase 3 — render-scope wiring

The point of all this: re-renders must be driven by signal reads.

- The shell's per-frame render walk (`prism-shell::render::render_tree`)
  starts each frame inside an `Owner` and an outermost
  `ReactiveContext`.
- Per-block `lower_ui` calls run inside per-block contexts whose
  callback is "mark `NodeId` dirty in the next frame's
  `DirtyQueue`". Any `Signal::read` inside a block subscribes
  automatically.
- A `DirtyQueue<NodeId>` is drained at the start of each frame.
  If empty, the previous `Surface` tree is re-presented as-is —
  no `lower_ui` re-walk. If non-empty, only the dirty subtrees
  re-lower.
- The femtovg backend's redraw scheduling is driven by the
  queue: empty queue, no winit `request_redraw`; non-empty,
  redraw.

This is where fine-grained reactivity stops being abstract and
starts being visible — selecting a node in the property panel no
longer re-walks the whole canvas, only the selected subtree's
chrome.

### Phase 4 — reactive props on builder blocks

Make `props: serde_json::Value` reactive. **Landed** as the trio
above: disk format stays `serde_json::Value`, in-memory access goes
through `DocumentBindings.props_for(node.id, &node.props).signal(key)`
via the `LowerCtx::prop_*` accessors and writes through the
`NodeMutator` builder. The original plan called for a typed
`ctx.prop_signal::<String>("title")` returning `Signal<String>`; the
landed surface gives `ctx.prop_signal(node, "title") -> Option<Signal<Value>>`
plus the typed `prop_str` / `prop_bool` reads — full
`Signal<String>` is a follow-up `Memo` one-liner if it becomes a
common need.

- `Node::props` stays serializable JSON for the on-disk document.
  In-memory access goes through `DocumentBindings`, which lazily
  materialises a per-key `Signal<Value>` on first
  `ctx.prop_*(node, key)` read.
- Block authors get one declarative seam: `ctx.prop_str(node, "title")`
  / `ctx.prop_bool` / `ctx.prop` (`Value`) / `ctx.prop_signal`
  (raw signal). Every read subscribes the current per-block
  reactive context (Phase 3b).
- The `Connection` system gained a new `ActionKind::Bind` variant
  (Phase 4a — already shipped): `Bind { target_key, source }` — a
  reactive one-way binding from any signal to a prop. Authored as
  `bind title = $selection.name` in the inline-action grammar.
  Internally registers an `Effect` on `BuilderDocument::install_bindings()`.
- Existing `SetProperty` keeps working unchanged — it is the
  imperative variant. `Bind` is the declarative variant.

### Phase 5 — Luau reactive surface

Luau scripts get the same `Signal::read` / `Signal::write` API as
Rust.

- `Signal<T>` gains a `#[luau_expose]` impl for `T: IntoLua + FromLua`.
  Luau sees `signal:read()`, `signal:peek()`, `signal:write(v)`,
  `signal:set(v)`.
- `Memo<T>` and `Effect` are similarly exposed. Authoring a
  reactive Luau handler becomes `effect(function() ... end)`.
- The signal type-stub generator
  (`prism-builder::signal::generate_signal_type_stubs`) is
  extended to emit Luau types for every reactive signal exposed
  by a block, not just the authored event channels. Naming-wise:
  authored event channels stay in `signals.d.luau`; reactive
  cells land in a sibling `reactive.d.luau`.

### Phase 6 — IPC-scoped signals: `#[daemon_fn]` + `RemoteSignal<T>`

Cross the daemon ↔ shell boundary with the same `Signal<T>` API.
Two concrete artifacts:

- **`#[daemon_fn]`** in `prism-luau-derive`: lifts the Dioxus
  `#[server]` pattern. One signature, two bodies. Server-side
  handler registers with the existing `#[daemon_command]` machinery;
  client-side stub serializes via `postcard` over the
  `interprocess` socket. Type-equal contract; no JSON at call
  sites.
- **`RemoteSignal<T>` in `prism-core::reactive::ipc`**: a signal
  whose backing store lives in the daemon, accessed from the
  shell via a generated stub. `RemoteSignal::read()` is an async
  read whose result feeds local subscribers when it arrives.
  `RemoteSignal::write(v)` is fire-and-forget; the daemon
  broadcasts the new value to every shell that has subscribed.
  Transport: bidirectional `postcard` frames over the existing
  stdio channel.

Both pieces compose: a `#[daemon_fn] async fn current_user() -> User`
returns a `RemoteSignal<User>` from the daemon's perspective, and
the shell consumes it as a `Signal<User>` like any other.

### Phase 7 — network-scoped signals: relay + federation + p2p

The point where Prism stops being "a UI framework that happens to
do networking" and becomes "a network framework that happens to
render UI."

Three concrete primitives, all in `prism-core::reactive::net`:

- **`FederatedSignal<T>`** — a signal whose subscribers can live
  on remote relays. Writes propagate via the
  `network::relay::modules::federation` module's existing
  cross-relay forwarding. Reads return a local cache that
  refreshes lazily. Eventually-consistent by construction;
  reconciles via Loro CRDT when `T` is a CRDT container.
- **`PeerSignal<T>`** — a signal whose subscribers are p2p peers
  connected via the `signaling` module's WebRTC data channels.
  Lower latency than `FederatedSignal`, no eventual consistency
  guarantees, suitable for presence / cursor / live-edit traffic.
  Reuses the existing `network::presence::PresenceManager` for
  participant tracking.
- **`RelaySignal<T>`** — a signal whose canonical store is a
  specific relay (one writer, many readers). Subscribers receive
  push updates through a relay-managed WebSocket. The relay's
  `RelayContext` capability registry already supports this
  shape; the work is to expose it as a `Signal<T>`.

These three replace what would otherwise be "presence APIs,"
"federation APIs," and "live-data APIs" with one model that
reuses every primitive from Phases 1–3. Authoring a real-time
collaboration feature collapses to:

```rust
let cursors: PeerSignal<HashMap<PeerId, CursorPos>> = ...;
Effect::new(move || render_cursors(cursors.read()));
```

— same shape as a local `Signal<T>`, different transport.

The corresponding `#[relay_fn]` / `#[peer_fn]` macros from §3.4
emit the matching server / client stubs so users authoring a
collaborative feature never touch a `RelayEnvelope` directly.

### Phase 8 — SSR-scoped signals: reactive `lower_semantic_html`

The relay's existing SSR path
(`prism-relay::ssr_routes` → `ui_runtime::lower_semantic_html`)
walks a `BuilderDocument` and emits semantic HTML. Today this is
fully recomputed per request. Once the document is reactive, SSR
is a *subscriber*:

- Each cacheable HTML fragment is an `Effect` whose body returns
  HTML, owned by a per-route `Owner`. When subscribed signals
  change, the cached HTML is invalidated; the next request
  re-renders only the dirty subtrees.
- Streaming SSR (HTTP chunked transfer) becomes natural: emit
  the static skeleton immediately, stream remaining chunks as
  their `Effect`s settle. Suspense without inventing a separate
  suspense protocol.
- `FederatedSignal` / `RelaySignal` reads inside SSR effects
  give you cache invalidation across the *whole fleet* of relay
  nodes for free — if a document is updated on one relay, every
  relay's cached SSR fragments invalidate via the federation
  bus.

This is the payoff of the scope ladder. SSR cache invalidation,
client cache invalidation, and reactive re-render are the same
mechanism — three subscribers to one signal graph.

### Phase 9 — hot reload (subsecond)

Wire `subsecond` into `prism-cli`'s dev loop as the primary
hot-patch path. Keep full respawn as the fallback for struct
layout changes. Tag reload anchor points in `prism-shell` at the
block-lowering boundary so a changed `lower_ui` body swaps in
without dropping the `Surface` tree or the `Owner` graph.

Pairs naturally with Phase 3 because the retained-mode tree
survives the patch — only the dirty queue needs to be replayed.

### Phase 10 — `.prism-ui` template hashing (optional)

If `.prism-ui` literal-only edits become a common hot-reload
case, lift Dioxus's `rsx!` template-hash trick into
`prism-ui-build`'s codegen: hash each template, fast-path
literal-only patches without re-evaluating the structure. Lower
priority than Phases 0–8.

## 7. Sequencing and dependencies

```
Phase 0 ── Phase 1 ── Phase 2 ── Phase 3 ── Phase 4 ── Phase 5
                          │                     │
                          ├── Phase 6 (IPC scope) ─┐
                          │                       │
                          ├── Phase 7 (network scope) ─┐
                          │                           │
                          └── Phase 8 (SSR scope) ────┘

                  Phase 9 (subsecond, parallel after 3)
                  Phase 10 (template hashing, any time)
```

Phases 0–3 are the spine: substrate, atom bridge, render-scope.
Phases 4–5 turn it into user-facing builder reactivity (props,
Luau).

Phases 6–8 climb the scope ladder. Each is independent of the
others once Phase 3 lands — IPC, network, and SSR scopes share
the `Signal<T>` API but their transports don't depend on one
another. Pick the order based on the application driving the
work:

- "I want collaborative editing" → 7 first.
- "I want the daemon to feel like a typed Rust function call" → 6 first.
- "I want fast SSR with cross-relay cache invalidation" → 8 first.

Phases 9–10 are dev-ergonomics and can land any time.

## 8. Open questions

- **Async / `Resource`.** Deferred. The synchronous core needs
  to land first; once it does, `Resource<T>` is `Memo<T>` + a
  spawned future. Decide async runtime then (likely the existing
  `tokio` workspace pin).
- **Send/Sync story.** Dioxus splits `UnsyncStorage` and
  `SyncStorage` in `generational-box`. Prism's shell is `!Send`
  by design (`ConfigModel`, `ActivityStore`); the daemon is
  multi-threaded `tokio`. Likely outcome: `Signal<T>` defaults
  to `UnsyncStorage` in shell crates, `SyncStorage` in daemon
  crates, with a `SendSignal<T>` alias for the cross-process
  cases. Decide concretely at Phase 1.
- **Reactive context across reloads.** When `subsecond` swaps in
  a new `lower_ui` body, the existing `Owner` is preserved but
  the reactive context captured the *old* function pointer's
  source `Location`. Verify the diagnostics layer survives.
- **CRDT atomicity.** A single Loro transaction can update many
  containers; today `CrdtSync` notifies per-container. With
  reactive dependents, batched notification (one dirty flush per
  transaction) is important to avoid flicker. Implement via a
  `ReactiveContext::batch(|| { ... })` scope.
- **Remote signal failure modes.** Local `Signal<T>::read()`
  cannot fail. `RemoteSignal`, `FederatedSignal`, `PeerSignal`,
  `RelaySignal` can — peer offline, relay unavailable, timeout.
  Two shapes were considered: (a) a state-enum carrier so
  subscribers can react to connectivity itself, (b) a fallible
  `try_read` + an infallible `last_known` cached read so
  most callers ignore connectivity. **Both have merits; merge
  them.** The merged carrier:

  ```rust
  pub enum RemoteState<T> {
      /// Never resolved yet — no cached value exists.
      Loading,
      /// Connected and the value is fresh.
      Live(T),
      /// Disconnected but the last received value is still
      /// available. Subscribers wanting "best effort" UI read
      /// this without distinguishing it from Live.
      Stale { value: T, since_ms: u64 },
      /// Connection failed. `last` carries the last known value
      /// if any — `None` means we never got one.
      Errored { last: Option<T>, err: RemoteError },
  }

  impl<T> RemoteState<T> {
      /// The "if you don't care about connectivity, just give me
      /// the value" accessor. Returns `None` only in Loading or
      /// Errored-without-history cases.
      pub fn last_known(&self) -> Option<&T> {
          match self {
              Self::Live(v) | Self::Stale { value: v, .. } => Some(v),
              Self::Errored { last, .. } => last.as_ref(),
              Self::Loading => None,
          }
      }
      pub fn is_live(&self) -> bool { matches!(self, Self::Live(_)) }
  }

  pub struct RemoteSignal<T: 'static> {
      inner: Signal<RemoteState<T>>,
  }

  impl<T: Clone + 'static> RemoteSignal<T> {
      /// Subscribing — returns the full state.
      pub fn read(&self) -> RemoteState<T> where T: Clone { ... }
      /// Subscribing — best-effort value (None during Loading
      /// or unrecoverable Errored).
      pub fn last_known(&self) -> Option<T> { ... }
      /// Fallible — `Ok` only when Live.
      pub fn try_read(&self) -> Result<T, RemoteError> { ... }
  }
  ```

  Why this merge works: the **carrier is always the enum** (so the
  reactive graph propagates connectivity transitions to anyone who
  cares — a "disconnected" badge, a stale-data tint, an offline
  banner), but the **accessors give you the (b) ergonomics for
  free** (`last_known()` for permissive UI, `try_read()` for code
  paths that must distinguish live from stale). Subscribers that
  truly don't care about connectivity write
  `if let Some(v) = remote.last_known() { ... }` and never see the
  enum. Subscribers that *do* care write
  `match remote.read() { Live(v) => …, Stale { value, since_ms } => …, … }`.
  One graph, two reading styles, no duplicate plumbing.

  Edge cases the merged shape resolves cleanly:
  - **First load.** Initial state is `Loading`; subscribers
    show a spinner. When the first value arrives, transitions to
    `Live`. `last_known()` returns `None` then `Some(v)`.
  - **Reconnect with the same value.** Transitions
    `Stale → Live` without changing the inner `T`. UI re-renders
    once (connectivity changed); `last_known()` returns the same
    `T` both times. PartialEq gating on the *outer enum*, not
    the inner T, is correct here.
  - **Optimistic write.** Writers go `set(Live(v))` directly;
    transport-layer failure transitions to
    `Errored { last: Some(v), err }` — UI keeps showing `v` with
    an error indicator instead of flicker-clearing.

  The four `Remote* / Federated* / Peer* / RelaySignal` variants
  in Phases 6–7 all use this carrier; differing transports
  populate different variants under different conditions.
- **SSR stream coordination.** Phase 8's streaming SSR needs a
  way to express "this fragment is allowed to be a placeholder
  until its signal settles." Likely a per-fragment timeout +
  fallback HTML. Out of scope for the substrate; lands in
  `prism-relay`.
- **Authentication scope of network signals.** A `RelaySignal<T>`
  may be public, capability-token-gated, or sovereign-portal-gated.
  The capability check happens at subscribe time, not read time —
  the relay's existing `capability_tokens` module is the
  enforcement point. Spell this out in the Phase 7 design.

## 9. References

- Dioxus repo: `dioxuslabs/dioxus`, especially `packages/signals`
  and `packages/generational-box`.
- Dioxus tutorial: <https://dioxuslabs.com/learn/0.7/tutorial/>.
- Prism today: `prism-core::kernel::atom`,
  `prism-core::kernel::crdt_sync`, `prism-builder::signal`,
  `prism-shell::services::signals`, `prism-luau-derive`.
- Prism network surface: `prism-core::network::relay` (17 modules:
  federation, signaling, presence, sovereign portals, …),
  `prism-core::network::presence`,
  `prism-relay::routes::federation` / `ssr_routes`.
- Adjacent docs: `docs/dev/clay-migration-plan.md` (post-Slint
  runtime), `docs/dev/luau-integration-plan.md` (Luau surface).
