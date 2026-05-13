//! `reactive` — Phase 1 of the Dioxus-inspired reactive overhaul
//! (`docs/dev/dioxus-inspiration.md`).
//!
//! The plan, in one sentence: a single thread-local
//! [`ReactiveContext::current`] plus `Copy + 'static` [`Signal`]
//! handles is enough to unify rendering, builder state, daemon
//! state, and CRDT-backed values behind one mental model. This
//! module is where that lands.
//!
//! ## What is here (Phase 1)
//!
//! - [`Signal<T>`] — `Copy + 'static` reactive cell over a
//!   `generational-box` slot. Subscribing reads ([`Signal::read`],
//!   [`Signal::get`]) register the current [`ReactiveContext`].
//!   Non-subscribing reads ([`Signal::peek`], [`Signal::snapshot`])
//!   do not. Writes ([`Signal::set`], [`Signal::write`]) notify
//!   every subscribed context.
//! - [`Memo<T>`] — a derived signal whose body re-runs whenever any
//!   tracked dependency fires. PartialEq-gates downstream
//!   notification.
//! - [`Effect`] — a side-effecting reactive context with `Drop`-based
//!   disposal.
//! - [`ReactiveContext`] — the unit of subscription. Lifted directly
//!   from `dioxus_signals::ReactiveContext`: thread-local stack,
//!   `reset_and_run_in` clears subscriptions before re-tracking, a
//!   per-context callback fired by `mark_dirty`.
//! - [`Owner`] — `generational-box` owner wrapper. Each owner owns
//!   a set of [`Signal`] slots and reclaims them on drop.
//!
//! ## What lands in Phase 2
//!
//! The existing [`crate::kernel::atom`] layer (`Atom<T>` +
//! `SharedAtom<T>` + `select`) gains a `reactive_signal()` accessor
//! so any reactive subscriber wakes on `Atom::set`. `kernel::crdt_sync`
//! follows.
//!
//! ## The scope ladder (Phases 6–8)
//!
//! Reactivity does not stop at the process boundary. The full plan
//! climbs a ladder: **Local** (this module) → **IPC**
//! (`RemoteSignal<T>` over `interprocess`+`postcard` to the daemon)
//! → **Network** (`FederatedSignal<T>` / `PeerSignal<T>` /
//! `RelaySignal<T>` over the relay's federation, signaling, and
//! WebSocket transports) → **SSR**
//! (`prism-relay::lower_semantic_html` as a reactive subscriber).
//! Same `read` / `write` / `effect` API at every level; only the
//! transport that delivers the notification changes. See
//! `docs/dev/dioxus-inspiration.md` §2.1 for the table and §6
//! Phases 6–8 for the per-scope landing plan.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::panic::Location;
use std::rc::Rc;

use generational_box::{AnyStorage, GenerationalBox, UnsyncStorage};

/// Phase 6 of `docs/dev/dioxus-inspiration.md`: IPC-scoped reactive
/// primitives. `RemoteState<T>` carrier, `RemoteSignal<T>` wrapper,
/// `DaemonInvoker` transport trait + `MockInvoker` test seam.
pub mod ipc;

/// Phase 7 of `docs/dev/dioxus-inspiration.md`: network-scoped
/// reactive primitives. `FederatedSignal<T>` (cross-relay
/// federation bus), `PeerSignal<T>` (WebRTC data channel),
/// `RelaySignal<T>` (canonical-store push). All three share the
/// `RemoteState<T>` carrier; differ in their transport trait
/// (`FederationTransport` / `PeerTransport` / `RelayTransport`).
pub mod net;

// ---------------------------------------------------------------
// ReactiveContext
// ---------------------------------------------------------------

/// Subscriber set held by each [`Signal`]. Stored behind an `Rc<RefCell<…>>`
/// so contexts can hold a back-reference and remove themselves on reset.
type SubscriberSet = Rc<RefCell<HashSet<u64>>>;

struct ContextSlot {
    /// `None` while the callback is *running* — see [`mark_dirty`].
    /// This take-and-restore pattern doubles as a re-entrancy
    /// circuit breaker: a context's callback cannot re-fire itself.
    callback: Option<Box<dyn FnMut()>>,
    /// Subscriber sets this context has inserted itself into. On
    /// [`ReactiveContext::reset_and_run_in`] and
    /// [`ReactiveContext::dispose`] the context removes itself from
    /// every set in this Vec.
    subscribed_to: Vec<SubscriberSet>,
    /// Call-site of [`ReactiveContext::new`] (resolved through
    /// `#[track_caller]` on the constructor — which `Effect::new` and
    /// `Memo::new` also wear). Phase 9 of
    /// `docs/dev/dioxus-inspiration.md` (subsecond hot-reload) needs
    /// this so a swapped-in `lower_ui` body whose `Effect` panics can
    /// be traced back to its authoring location; render-scope wiring
    /// (Phase 3) uses it to name the NodeId boundary owning each
    /// reactive scope. Always populated, even outside debug builds —
    /// `Location` is zero-cost (a single 'static pointer).
    source: &'static Location<'static>,
}

thread_local! {
    /// LIFO stack of currently-active reactive contexts.
    /// [`ReactiveContext::current`] returns the top.
    static STACK: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    static CONTEXTS: RefCell<HashMap<u64, ContextSlot>> = RefCell::new(HashMap::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
    /// Active [`ReactiveContext::batch`] scopes on this thread. While
    /// `> 0`, [`mark_dirty`] collects ids into `BATCH_PENDING` instead
    /// of firing callbacks; the outermost scope drains them once on
    /// exit. Coalesces multi-write transactions (e.g. a single Loro
    /// commit that updates many containers) into one wave of subscriber
    /// notifications.
    static BATCH_DEPTH: Cell<u32> = const { Cell::new(0) };
    static BATCH_PENDING: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

fn fresh_id() -> u64 {
    NEXT_ID.with(|n| {
        let v = n.get();
        n.set(v + 1);
        v
    })
}

/// A unit of reactive subscription. Created with a callback; the
/// callback fires whenever any signal read inside a
/// [`ReactiveContext::reset_and_run_in`] scope (with this context
/// on top of the stack) is later written to.
///
/// `ReactiveContext` is `Copy` and `'static` — the inner state lives
/// in a thread-local table keyed by [`ReactiveContext::id`]. Call
/// [`ReactiveContext::dispose`] to release the slot when the context
/// is no longer needed; [`Effect`] and [`Memo`] handle disposal
/// automatically through their `Drop` impl.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReactiveContext {
    id: u64,
}

impl ReactiveContext {
    /// Construct a fresh reactive context with the given dirty callback.
    ///
    /// Carries the call-site `Location` via `#[track_caller]` for the
    /// diagnostics path described on [`ContextSlot::source`]. Callers
    /// that already wrap `ReactiveContext::new` (such as
    /// [`Effect::new`] and [`Memo::new`]) propagate the attribute so
    /// the recorded location is the user's call site, not the
    /// wrapper.
    #[track_caller]
    pub fn new<F>(callback: F) -> Self
    where
        F: FnMut() + 'static,
    {
        Self::new_at(callback, Location::caller())
    }

    /// Construct a reactive context with an explicit source location.
    /// Used by call sites that want to override what `#[track_caller]`
    /// would have captured — e.g. an internal helper that builds the
    /// context on behalf of a user-visible API but wants the user's
    /// own location recorded.
    pub fn new_at<F>(callback: F, source: &'static Location<'static>) -> Self
    where
        F: FnMut() + 'static,
    {
        let id = fresh_id();
        CONTEXTS.with(|m| {
            m.borrow_mut().insert(
                id,
                ContextSlot {
                    callback: Some(Box::new(callback)),
                    subscribed_to: Vec::new(),
                    source,
                },
            );
        });
        Self { id }
    }

    /// The call-site `Location` captured when this context was
    /// constructed. Returns `None` only if the context has already
    /// been disposed.
    pub fn source(&self) -> Option<&'static Location<'static>> {
        CONTEXTS.with(|m| m.borrow().get(&self.id).map(|slot| slot.source))
    }

    /// The reactive context currently on top of the stack on this
    /// thread, if any. Returns `None` outside any
    /// [`ReactiveContext::reset_and_run_in`] scope.
    pub fn current() -> Option<Self> {
        STACK.with(|s| s.borrow().last().copied().map(|id| Self { id }))
    }

    /// Clear all existing subscriptions, push self onto the stack,
    /// run `f`, pop. Any signal reads inside `f` will subscribe this
    /// context to those signals — replacing whatever dependency set
    /// the context had before.
    pub fn reset_and_run_in<R>(&self, f: impl FnOnce() -> R) -> R {
        // 1. Remove ourselves from every signal subscriber set we'd
        //    been inserted into.
        CONTEXTS.with(|m| {
            if let Some(slot) = m.borrow_mut().get_mut(&self.id) {
                for subs in slot.subscribed_to.drain(..) {
                    subs.borrow_mut().remove(&self.id);
                }
            }
        });
        // 2. Push onto the stack and run.
        STACK.with(|s| s.borrow_mut().push(self.id));
        let result = f();
        STACK.with(|s| {
            let popped = s.borrow_mut().pop();
            debug_assert_eq!(popped, Some(self.id));
        });
        result
    }

    /// Run `f` with subscriber notifications deferred until the
    /// outermost batch exits. Every [`Signal::write`] / [`Signal::set`]
    /// / [`Signal::try_set`] inside the scope queues its subscriber
    /// ids; on exit each unique id fires exactly once.
    ///
    /// **Use case (Phase 8 open Q of
    /// `docs/dev/dioxus-inspiration.md`):** a single Loro transaction
    /// can update many CRDT containers. Without batching, each
    /// container's reactive listeners fire mid-transaction —
    /// `Effect`s observing several containers re-run once per write.
    /// Wrapping the commit in `batch(..)` collapses every dependent
    /// reactive scope to one wake-up per transaction, regardless of
    /// how many containers it touched.
    ///
    /// Nested batches are fine — only the outermost flushes. The
    /// callback's return value passes through verbatim.
    pub fn batch<R>(f: impl FnOnce() -> R) -> R {
        BATCH_DEPTH.with(|d| d.set(d.get() + 1));
        let result = f();
        let outermost = BATCH_DEPTH.with(|d| {
            let next = d.get() - 1;
            d.set(next);
            next == 0
        });
        if outermost {
            // Drain in FIFO order. New writes triggered by a fired
            // callback land in the freshly-empty queue and run inline
            // (depth has already dropped to zero), matching the
            // non-batched semantics for callback-internal writes.
            let drained = BATCH_PENDING.with(|q| std::mem::take(&mut *q.borrow_mut()));
            for id in drained {
                mark_dirty(id);
            }
        }
        result
    }

    /// Release the context's slot. Removes the context from every
    /// signal it was subscribed to. After dispose, [`mark_dirty`] for
    /// this id is a no-op.
    pub fn dispose(self) {
        CONTEXTS.with(|m| {
            if let Some(slot) = m.borrow_mut().remove(&self.id) {
                for subs in slot.subscribed_to {
                    subs.borrow_mut().remove(&self.id);
                }
            }
        });
    }

    fn note_subscription(&self, subs: SubscriberSet) {
        CONTEXTS.with(|m| {
            if let Some(slot) = m.borrow_mut().get_mut(&self.id) {
                slot.subscribed_to.push(subs);
            }
        });
    }
}

/// Fire the callback associated with `id`, if any.
///
/// Two re-entrancy guards:
///
/// 1. **On-stack guard.** If the context is currently running on
///    this thread (its id is on `STACK`), a self-write triggered
///    from inside its own body would otherwise re-fire it
///    synchronously. Skip. This is the "self-cycling effect"
///    guard: an effect that reads *and* writes the same signal
///    won't infinite-loop or re-borrow its own closure cell.
/// 2. **Take-and-restore.** The callback is taken out of the slot
///    while it runs so it can mutate the contexts table (e.g. via
///    `reset_and_run_in`) without violating the `RefCell` borrow.
///    A nested `mark_dirty` for the same id from a different
///    callback path would find the slot's callback `None` and
///    become a no-op.
fn mark_dirty(id: u64) {
    let on_stack = STACK.with(|s| s.borrow().contains(&id));
    if on_stack {
        return;
    }
    // Inside a `ReactiveContext::batch` scope, defer the callback —
    // the outermost scope's drain will fire each unique id once after
    // the transaction completes.
    if BATCH_DEPTH.with(|d| d.get()) > 0 {
        BATCH_PENDING.with(|q| {
            let mut q = q.borrow_mut();
            if !q.contains(&id) {
                q.push(id);
            }
        });
        return;
    }
    let cb = CONTEXTS.with(|m| {
        m.borrow_mut()
            .get_mut(&id)
            .and_then(|slot| slot.callback.take())
    });
    if let Some(mut cb) = cb {
        cb();
        CONTEXTS.with(|m| {
            if let Some(slot) = m.borrow_mut().get_mut(&id) {
                // Restore only if the slot still exists — a dispose
                // mid-callback removes it, and we'd want to drop cb
                // on the floor in that case.
                slot.callback = Some(cb);
            }
        });
    }
}

// ---------------------------------------------------------------
// Owner + Signal
// ---------------------------------------------------------------

/// Lifetime guard for a set of [`Signal`] slots and the reactive
/// scopes that share their lifetime. Dropping the owner reclaims
/// every slot allocated through it *and* disposes every
/// [`Effect`] / [`Memo`] inserted through
/// [`Owner::insert_effect`] / [`Owner::insert_memo`].
///
/// Prism's policy (per `docs/dev/dioxus-inspiration.md` §6 Phase 3):
/// one `Owner` per `BuilderDocument` node, one per `Page`, one for
/// the shell-global scope. The render walker constructs effects
/// into the per-node owner so a node removed from the tree tears
/// down its reactive scopes deterministically.
pub struct Owner {
    inner: generational_box::Owner<UnsyncStorage>,
    /// Reactive scopes that share this owner's lifetime. Boxed so
    /// the Vec is type-erased; dropping the Vec calls each value's
    /// `Drop` impl, which disposes the context the scope held.
    /// Insertion-order drop is sufficient — reactive scopes don't
    /// depend on one another's `Drop` ordering.
    retained: RefCell<Vec<Box<dyn std::any::Any>>>,
}

impl Owner {
    /// Construct a fresh owner with no allocated slots.
    pub fn new() -> Self {
        Self {
            inner: <UnsyncStorage as AnyStorage>::owner(),
            retained: RefCell::new(Vec::new()),
        }
    }

    /// Insert a value into the owner's arena and return a
    /// `Copy + 'static` [`Signal`] handle.
    pub fn insert<T: 'static>(&self, value: T) -> Signal<T> {
        Signal {
            inner: self.inner.insert(SignalData {
                value,
                subscribers: Rc::new(RefCell::new(HashSet::new())),
            }),
        }
    }

    /// Build an [`Effect`] whose lifetime is tied to this owner. The
    /// effect runs once immediately and again on every dirty signal
    /// it depends on; when the owner drops, the effect's context is
    /// disposed alongside the owner's signal slots.
    ///
    /// Use this instead of [`Effect::new`] when the effect's natural
    /// home is the same scope as the signals it observes — the
    /// typical case for render-walk effects (Phase 3 of
    /// `docs/dev/dioxus-inspiration.md`).
    #[track_caller]
    pub fn insert_effect<F>(&self, f: F)
    where
        F: FnMut() + 'static,
    {
        let source = Location::caller();
        let f = Rc::new(RefCell::new(f));
        let ctx_holder: Rc<Cell<Option<ReactiveContext>>> = Rc::new(Cell::new(None));

        let f_for_cb = Rc::clone(&f);
        let ctx_for_cb = Rc::clone(&ctx_holder);
        let context = ReactiveContext::new_at(
            move || {
                let Some(ctx) = ctx_for_cb.get() else {
                    return;
                };
                ctx.reset_and_run_in(|| {
                    let mut f = f_for_cb.borrow_mut();
                    (*f)();
                });
            },
            source,
        );
        ctx_holder.set(Some(context));

        context.reset_and_run_in(|| {
            let mut f = f.borrow_mut();
            (*f)();
        });

        self.retained
            .borrow_mut()
            .push(Box::new(Effect { context }));
    }

    /// Build a [`Memo`] whose lifetime is tied to this owner. The
    /// memo's slot is allocated from this owner; the returned handle
    /// is `Clone`-as-`Rc`-bump and may be passed around freely, but
    /// the underlying context is disposed when the owner drops.
    #[track_caller]
    pub fn insert_memo<T, F>(&self, f: F) -> Memo<T>
    where
        T: PartialEq + 'static,
        F: FnMut() -> T + 'static,
    {
        let memo = Memo::new(self, f);
        self.retained.borrow_mut().push(Box::new(memo.clone()));
        memo
    }

    /// How many reactive scopes (effects + memos) this owner is
    /// currently retaining. Exposed for tests and diagnostics.
    pub fn retained_len(&self) -> usize {
        self.retained.borrow().len()
    }
}

impl Default for Owner {
    fn default() -> Self {
        Self::new()
    }
}

struct SignalData<T> {
    value: T,
    subscribers: SubscriberSet,
}

/// A `Copy + 'static` handle to a reactive value.
///
/// Reads with [`Signal::read`] / [`Signal::get`] subscribe the
/// current [`ReactiveContext`]; reads with [`Signal::peek`] /
/// [`Signal::snapshot`] do not. Writes with [`Signal::set`] /
/// [`Signal::write`] notify every subscribed context.
pub struct Signal<T: 'static> {
    inner: GenerationalBox<SignalData<T>>,
}

impl<T: 'static> Copy for Signal<T> {}
impl<T: 'static> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Signal<T> {
    /// Subscribing scoped read. Registers the current reactive
    /// context (if any) as a subscriber, then calls `f` with a
    /// reference to the value.
    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let data = self.inner.read();
        if let Some(ctx) = ReactiveContext::current() {
            let newly_added = data.subscribers.borrow_mut().insert(ctx.id);
            if newly_added {
                ctx.note_subscription(Rc::clone(&data.subscribers));
            }
        }
        f(&data.value)
    }

    /// Non-subscribing scoped read.
    pub fn peek<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let data = self.inner.read();
        f(&data.value)
    }

    /// Subscribe the current reactive context without producing a
    /// value. Useful when you want a context to depend on a signal
    /// purely for its dirty signal.
    pub fn track(&self) {
        self.read(|_| ());
    }

    /// Scoped mutation. The closure receives `&mut T`; subscribers
    /// are notified after it returns.
    pub fn write<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let result = {
            let mut data = self.inner.write();
            f(&mut data.value)
        };
        self.notify();
        result
    }

    /// Replace the value and notify subscribers unconditionally.
    pub fn set(&self, value: T) {
        {
            let mut data = self.inner.write();
            data.value = value;
        }
        self.notify();
    }

    /// Replace and notify, but silently return `false` if the
    /// underlying slot has been reclaimed by its [`Owner`] or is
    /// currently borrowed elsewhere.
    ///
    /// Intended for cross-lifetime bridges — e.g. an
    /// [`crate::kernel::atom::Atom`] listener writing into a
    /// reactive [`Signal`] whose `Owner` may drop independently.
    /// Don't use for normal in-scope writes; use [`Signal::set`].
    pub fn try_set(&self, value: T) -> bool {
        let Ok(mut data) = self.inner.try_write() else {
            return false;
        };
        data.value = value;
        let subs: Vec<u64> = {
            let borrow = data.subscribers.borrow();
            borrow.iter().copied().collect()
        };
        drop(data);
        for id in subs {
            mark_dirty(id);
        }
        true
    }

    fn notify(&self) {
        let subs: Vec<u64> = {
            let data = self.inner.read();
            let borrow = data.subscribers.borrow();
            borrow.iter().copied().collect()
        };
        for id in subs {
            mark_dirty(id);
        }
    }
}

impl<T: Clone + 'static> Signal<T> {
    /// Subscribing clone-snapshot. `read(|v| v.clone())`.
    pub fn get(&self) -> T {
        self.read(|v| v.clone())
    }

    /// Non-subscribing clone-snapshot.
    pub fn snapshot(&self) -> T {
        self.peek(|v| v.clone())
    }
}

// ---------------------------------------------------------------
// Memo
// ---------------------------------------------------------------

/// A derived signal whose body re-runs whenever any tracked
/// dependency fires. PartialEq-gates the downstream notification:
/// recomputing to an equal value does not wake further subscribers.
///
/// `Memo` is `Clone`-as-handle (cheap `Rc` bump). The underlying
/// reactive context is disposed when the last clone drops. For a
/// pure `Copy + 'static` handle, call [`Memo::signal`] to get the
/// inner [`Signal`].
pub struct Memo<T: 'static> {
    inner: Rc<MemoInner<T>>,
}

struct MemoInner<T: 'static> {
    signal: Signal<T>,
    context: ReactiveContext,
}

impl<T: 'static> Clone for Memo<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: PartialEq + 'static> Memo<T> {
    /// Build a memo that owns its slot from `owner`. The closure `f`
    /// is run immediately inside a fresh reactive context to compute
    /// the initial value and capture the initial dependency set.
    #[track_caller]
    pub fn new<F>(owner: &Owner, f: F) -> Self
    where
        F: FnMut() -> T + 'static,
    {
        // Self-referential pattern: the dirty callback needs access
        // to its own context (to re-track deps) and to the signal
        // (to push the recomputed value). Both are stamped after
        // creation via these holders.
        let source = Location::caller();
        let ctx_holder: Rc<Cell<Option<ReactiveContext>>> = Rc::new(Cell::new(None));
        let signal_holder: Rc<Cell<Option<Signal<T>>>> = Rc::new(Cell::new(None));
        let f = Rc::new(RefCell::new(f));

        let ctx_for_cb = Rc::clone(&ctx_holder);
        let signal_for_cb = Rc::clone(&signal_holder);
        let f_for_cb = Rc::clone(&f);
        let context = ReactiveContext::new_at(
            move || {
                let (Some(ctx), Some(signal)) = (ctx_for_cb.get(), signal_for_cb.get()) else {
                    return;
                };
                let new_val = ctx.reset_and_run_in(|| {
                    let mut f = f_for_cb.borrow_mut();
                    (*f)()
                });
                let changed = signal.peek(|cur| cur != &new_val);
                if changed {
                    signal.set(new_val);
                }
            },
            source,
        );
        ctx_holder.set(Some(context));

        let initial = context.reset_and_run_in(|| {
            let mut f = f.borrow_mut();
            (*f)()
        });
        let signal = owner.insert(initial);
        signal_holder.set(Some(signal));

        Self {
            inner: Rc::new(MemoInner { signal, context }),
        }
    }
}

impl<T: 'static> Memo<T> {
    /// The inner [`Signal`] handle. `Copy + 'static` — pass into
    /// closures and child reactive scopes without cloning the Memo.
    pub fn signal(&self) -> Signal<T> {
        self.inner.signal
    }

    /// Subscribing scoped read.
    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.inner.signal.read(f)
    }

    /// Non-subscribing scoped read.
    pub fn peek<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.inner.signal.peek(f)
    }

    /// The call-site `Location` captured when this memo was
    /// constructed. Returns `None` only after the last `Memo` clone
    /// has been dropped and its context disposed.
    pub fn source(&self) -> Option<&'static Location<'static>> {
        self.inner.context.source()
    }
}

impl<T: Clone + 'static> Memo<T> {
    pub fn get(&self) -> T {
        self.inner.signal.get()
    }
    pub fn snapshot(&self) -> T {
        self.inner.signal.snapshot()
    }
}

impl<T: 'static> Drop for MemoInner<T> {
    fn drop(&mut self) {
        self.context.dispose();
    }
}

// ---------------------------------------------------------------
// Effect
// ---------------------------------------------------------------

/// A side-effecting reactive context. The closure runs once on
/// construction and re-runs whenever any tracked signal changes.
/// Dropping the effect disposes its context.
pub struct Effect {
    context: ReactiveContext,
}

impl Effect {
    /// Build an effect that runs `f` immediately and then again on
    /// every dirty signal it depends on.
    #[track_caller]
    pub fn new<F>(f: F) -> Self
    where
        F: FnMut() + 'static,
    {
        let source = Location::caller();
        let f = Rc::new(RefCell::new(f));
        let ctx_holder: Rc<Cell<Option<ReactiveContext>>> = Rc::new(Cell::new(None));

        let f_for_cb = Rc::clone(&f);
        let ctx_for_cb = Rc::clone(&ctx_holder);
        let context = ReactiveContext::new_at(
            move || {
                let Some(ctx) = ctx_for_cb.get() else {
                    return;
                };
                ctx.reset_and_run_in(|| {
                    let mut f = f_for_cb.borrow_mut();
                    (*f)();
                });
            },
            source,
        );
        ctx_holder.set(Some(context));

        context.reset_and_run_in(|| {
            let mut f = f.borrow_mut();
            (*f)();
        });

        Self { context }
    }

    /// The call-site `Location` captured when this effect was
    /// constructed. Returns `None` only after the effect has been
    /// disposed (i.e. dropped).
    pub fn source(&self) -> Option<&'static Location<'static>> {
        self.context.source()
    }
}

impl Drop for Effect {
    fn drop(&mut self) {
        self.context.dispose();
    }
}

// ---------------------------------------------------------------
// DirtyQueue
// ---------------------------------------------------------------

/// Dedup'd FIFO of dirty keys, drained per frame / per tick / per
/// transaction. The render-scope (`prism-shell::render`), the SSR
/// fragment cache (`prism-relay`), and the daemon's IPC fan-out
/// will all use this primitive to batch reactive notifications
/// into discrete flushes.
///
/// Insertion is `O(1)` (hash lookup); drain is `O(n)`. Keys retain
/// insertion order (an `IndexSet` would do the same with a simpler
/// API, but we'd pull an extra dep just for this — the `HashSet +
/// Vec` combo is fine).
pub struct DirtyQueue<K: Eq + std::hash::Hash + Clone> {
    seen: HashSet<K>,
    order: Vec<K>,
}

impl<K: Eq + std::hash::Hash + Clone> DirtyQueue<K> {
    pub fn new() -> Self {
        Self {
            seen: HashSet::new(),
            order: Vec::new(),
        }
    }

    /// Insert `key` if not already present. Returns `true` if newly
    /// inserted.
    pub fn mark(&mut self, key: K) -> bool {
        if self.seen.insert(key.clone()) {
            self.order.push(key);
            true
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Drain the queue, returning keys in insertion order. The
    /// queue is empty after this call.
    pub fn drain(&mut self) -> Vec<K> {
        self.seen.clear();
        std::mem::take(&mut self.order)
    }
}

impl<K: Eq + std::hash::Hash + Clone> Default for DirtyQueue<K> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------
// Tests
// ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_round_trips_a_value() {
        let owner = Owner::new();
        let sig = owner.insert(7_i32);
        assert_eq!(sig.snapshot(), 7);
        sig.set(42);
        assert_eq!(sig.snapshot(), 42);
    }

    #[test]
    fn signal_write_updates_in_place() {
        let owner = Owner::new();
        let sig: Signal<Vec<i32>> = owner.insert(vec![1, 2]);
        sig.write(|v| v.push(3));
        sig.peek(|v| assert_eq!(v.as_slice(), &[1, 2, 3]));
    }

    #[test]
    fn signal_handles_are_copy() {
        let owner = Owner::new();
        let sig = owner.insert(String::from("hello"));
        let again = sig; // Copy, not move.
        assert_eq!(sig.snapshot(), "hello");
        assert_eq!(again.snapshot(), "hello");
    }

    #[test]
    fn reactive_context_current_is_none_outside_scope() {
        assert!(ReactiveContext::current().is_none());
    }

    #[test]
    fn effect_fires_initially_and_on_dep_change() {
        let owner = Owner::new();
        let sig = owner.insert(0_i32);
        let counter = Rc::new(Cell::new(0));
        let counter_for_effect = Rc::clone(&counter);

        let _e = Effect::new(move || {
            sig.read(|_| {});
            counter_for_effect.set(counter_for_effect.get() + 1);
        });
        assert_eq!(counter.get(), 1, "ran once on construction");

        sig.set(1);
        assert_eq!(counter.get(), 2, "ran again on dep change");

        sig.set(2);
        assert_eq!(counter.get(), 3, "ran again on dep change");
    }

    #[test]
    fn effect_does_not_fire_on_unrelated_signal() {
        let owner = Owner::new();
        let watched = owner.insert(0_i32);
        let unrelated = owner.insert(0_i32);
        let counter = Rc::new(Cell::new(0));
        let counter_for_effect = Rc::clone(&counter);

        let _e = Effect::new(move || {
            watched.read(|_| {});
            counter_for_effect.set(counter_for_effect.get() + 1);
        });
        assert_eq!(counter.get(), 1);

        unrelated.set(99);
        assert_eq!(counter.get(), 1, "no re-run from unrelated signal");

        watched.set(1);
        assert_eq!(counter.get(), 2);
    }

    #[test]
    fn effect_disposes_on_drop() {
        let owner = Owner::new();
        let sig = owner.insert(0_i32);
        let counter = Rc::new(Cell::new(0));
        let counter_for_effect = Rc::clone(&counter);

        let e = Effect::new(move || {
            sig.read(|_| {});
            counter_for_effect.set(counter_for_effect.get() + 1);
        });
        assert_eq!(counter.get(), 1);

        sig.set(1);
        assert_eq!(counter.get(), 2);

        drop(e);
        sig.set(2);
        assert_eq!(counter.get(), 2, "no re-run after drop");
    }

    #[test]
    fn effect_retracks_after_branch_changes() {
        // The effect branches on `cond`: when true it reads `a`, when
        // false it reads `b`. After flipping cond, the dep set should
        // swap — changes to the now-unused branch's signal stop
        // triggering.
        let owner = Owner::new();
        let cond = owner.insert(true);
        let a = owner.insert(10);
        let b = owner.insert(20);
        let last = Rc::new(Cell::new(0));
        let last_for_effect = Rc::clone(&last);

        let _e = Effect::new(move || {
            let pick = if cond.get() { a.get() } else { b.get() };
            last_for_effect.set(pick);
        });
        assert_eq!(last.get(), 10);

        a.set(11);
        assert_eq!(last.get(), 11);

        cond.set(false);
        assert_eq!(last.get(), 20, "swapped to b");

        // a is no longer a dep — changes shouldn't propagate.
        a.set(999);
        assert_eq!(last.get(), 20, "a is no longer tracked");

        b.set(21);
        assert_eq!(last.get(), 21);
    }

    #[test]
    fn memo_initial_value_is_the_closure_result() {
        let owner = Owner::new();
        let a = owner.insert(2);
        let b = owner.insert(3);
        let sum = Memo::new(&owner, move || a.get() + b.get());
        assert_eq!(sum.snapshot(), 5);
    }

    #[test]
    fn memo_recomputes_when_inputs_change() {
        let owner = Owner::new();
        let a = owner.insert(2);
        let b = owner.insert(3);
        let sum = Memo::new(&owner, move || a.get() + b.get());
        assert_eq!(sum.snapshot(), 5);
        a.set(10);
        assert_eq!(sum.snapshot(), 13);
        b.set(100);
        assert_eq!(sum.snapshot(), 110);
    }

    #[test]
    fn memo_skips_downstream_notify_if_value_unchanged() {
        // Squaring 3 and -3 both give 9. Downstream effect should
        // fire on the first computation but not the second.
        let owner = Owner::new();
        let x = owner.insert(3);
        let square = Memo::new(&owner, move || x.get() * x.get());
        let square_sig = square.signal();
        let runs = Rc::new(Cell::new(0));
        let runs_for_effect = Rc::clone(&runs);
        let _e = Effect::new(move || {
            square_sig.read(|_| {});
            runs_for_effect.set(runs_for_effect.get() + 1);
        });
        assert_eq!(runs.get(), 1);

        x.set(-3); // squared still 9
        assert_eq!(square.snapshot(), 9);
        assert_eq!(runs.get(), 1, "PartialEq gating suppressed downstream");

        x.set(4); // squared is 16 — real change
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn memo_chains() {
        let owner = Owner::new();
        let a = owner.insert(2);
        let doubled = Memo::new(&owner, move || a.get() * 2);
        let doubled_sig = doubled.signal();
        let quadrupled = Memo::new(&owner, move || doubled_sig.get() * 2);
        assert_eq!(quadrupled.snapshot(), 8);
        a.set(5);
        assert_eq!(doubled.snapshot(), 10);
        assert_eq!(quadrupled.snapshot(), 20);
    }

    #[test]
    fn self_cycling_effect_does_not_infinite_loop() {
        // An effect that both reads and writes the same signal would
        // recurse forever without the re-entrancy guard in
        // `mark_dirty`. The on-stack guard makes the inner write a
        // no-op (the effect is already running), so each outer write
        // triggers exactly one re-run.
        let owner = Owner::new();
        let sig = owner.insert(0_i32);
        let runs = Rc::new(Cell::new(0));
        let runs_for_effect = Rc::clone(&runs);

        let _e = Effect::new(move || {
            let cur = sig.get();
            runs_for_effect.set(runs_for_effect.get() + 1);
            if cur < 3 {
                // Self-write — guarded by the on-stack check in
                // `mark_dirty`. The set lands in the signal but does
                // not re-fire this effect.
                sig.set(cur + 1);
            }
        });
        // Initial run: cur=0, runs=1, set sig=1.
        assert_eq!(runs.get(), 1);
        assert_eq!(sig.snapshot(), 1);

        // External set fires the effect once: cur=10, runs=2, no
        // inner set (cur >= 3).
        sig.set(10);
        assert_eq!(runs.get(), 2);
        assert_eq!(sig.snapshot(), 10);
    }

    #[test]
    fn dispose_removes_context_from_signal_subscribers() {
        let owner = Owner::new();
        let sig = owner.insert(0_i32);
        let counter = Rc::new(Cell::new(0));
        let counter_for_effect = Rc::clone(&counter);
        let ctx = ReactiveContext::new(move || {
            counter_for_effect.set(counter_for_effect.get() + 1);
        });
        ctx.reset_and_run_in(|| {
            sig.read(|_| {});
        });
        sig.set(1);
        assert_eq!(counter.get(), 1);

        ctx.dispose();
        sig.set(2);
        assert_eq!(
            counter.get(),
            1,
            "disposed context received no further dirty"
        );
    }

    #[test]
    fn multiple_subscribers_all_fire() {
        let owner = Owner::new();
        let sig = owner.insert(0);
        let a = Rc::new(Cell::new(0));
        let b = Rc::new(Cell::new(0));
        let a_for = Rc::clone(&a);
        let b_for = Rc::clone(&b);

        let _e1 = Effect::new(move || {
            a_for.set(sig.get());
        });
        let _e2 = Effect::new(move || {
            b_for.set(sig.get() * 10);
        });
        assert_eq!(a.get(), 0);
        assert_eq!(b.get(), 0);

        sig.set(7);
        assert_eq!(a.get(), 7);
        assert_eq!(b.get(), 70);
    }

    #[test]
    fn dirty_queue_dedups_and_preserves_order() {
        let mut q: DirtyQueue<&'static str> = DirtyQueue::new();
        assert!(q.is_empty());

        assert!(q.mark("a"));
        assert!(q.mark("b"));
        assert!(!q.mark("a"), "duplicate insert returns false");
        assert!(q.mark("c"));
        assert_eq!(q.len(), 3);

        let drained = q.drain();
        assert_eq!(drained, vec!["a", "b", "c"]);
        assert!(q.is_empty());
    }

    #[test]
    fn dirty_queue_can_be_driven_from_effect() {
        // The Phase 3 pattern: a `DirtyQueue<NodeId>` is the shared
        // state between the reactive substrate and the per-frame
        // render walk. Effects mark nodes dirty; the render walk
        // drains the queue.
        let owner = Owner::new();
        let sig_a = owner.insert(0);
        let sig_b = owner.insert(0);
        let queue: Rc<RefCell<DirtyQueue<&'static str>>> = Rc::new(RefCell::new(DirtyQueue::new()));
        let queue_a = Rc::clone(&queue);
        let queue_b = Rc::clone(&queue);

        let _e1 = Effect::new(move || {
            sig_a.read(|_| {});
            queue_a.borrow_mut().mark("node-a");
        });
        let _e2 = Effect::new(move || {
            sig_b.read(|_| {});
            queue_b.borrow_mut().mark("node-b");
        });
        // Initial run marks both.
        assert_eq!(queue.borrow_mut().drain(), vec!["node-a", "node-b"]);

        sig_a.set(1);
        assert_eq!(queue.borrow_mut().drain(), vec!["node-a"]);

        sig_b.set(1);
        assert_eq!(queue.borrow_mut().drain(), vec!["node-b"]);

        // Two writes between drains coalesce.
        sig_a.set(2);
        sig_a.set(3);
        assert_eq!(queue.borrow_mut().drain(), vec!["node-a"]);
    }

    #[test]
    fn reactive_context_carries_source_location() {
        // `#[track_caller]` on the constructor records the user's
        // call site, not the internal wrapper. This is the
        // diagnostics seam Phase 3 (render-scope wiring) and Phase 9
        // (subsecond hot-reload) read.
        let ctx = ReactiveContext::new(|| {});
        let loc = ctx.source().expect("source recorded");
        // The location's file is *this* test module — proves
        // `#[track_caller]` resolved through `new` → `new_at`.
        assert!(
            loc.file().ends_with("reactive.rs"),
            "expected this file, got {}",
            loc.file()
        );
        ctx.dispose();
        // After dispose the slot is gone; source returns None.
        assert!(ctx.source().is_none());
    }

    #[test]
    fn effect_source_resolves_to_user_call_site() {
        let owner = Owner::new();
        let _sig: Signal<i32> = owner.insert(0);
        let e = Effect::new(|| {});
        let loc = e.source().expect("source recorded");
        assert!(loc.file().ends_with("reactive.rs"));
    }

    #[test]
    fn memo_source_resolves_to_user_call_site() {
        let owner = Owner::new();
        let a = owner.insert(0_i32);
        let m = Memo::new(&owner, move || a.get() + 1);
        let loc = m.source().expect("source recorded");
        assert!(loc.file().ends_with("reactive.rs"));
    }

    #[test]
    fn owner_insert_effect_runs_once_and_on_dep_change() {
        let owner = Owner::new();
        let sig = owner.insert(0_i32);
        let counter = Rc::new(Cell::new(0));
        let counter_for = Rc::clone(&counter);

        owner.insert_effect(move || {
            sig.track();
            counter_for.set(counter_for.get() + 1);
        });
        assert_eq!(counter.get(), 1);
        assert_eq!(owner.retained_len(), 1);

        sig.set(1);
        assert_eq!(counter.get(), 2);
    }

    #[test]
    fn owner_drop_disposes_inserted_effects() {
        // The point of Phase 1's "Effect with Owner-scoped lifetime":
        // dropping the owner reclaims every Effect inserted through
        // it, in addition to every Signal slot.
        let sig: Signal<i32>;
        let counter = Rc::new(Cell::new(0));
        {
            let owner = Owner::new();
            sig = owner.insert(0);
            let counter_for = Rc::clone(&counter);
            owner.insert_effect(move || {
                sig.track();
                counter_for.set(counter_for.get() + 1);
            });
            assert_eq!(counter.get(), 1);
            sig.set(1);
            assert_eq!(counter.get(), 2);
            // owner drops here — sig slot reclaimed, effect context
            // disposed.
        }
        // sig.set on a reclaimed slot is silently dropped by
        // `try_set`; using `set` would panic. The effect must not
        // re-fire either way — its context was disposed with the
        // owner.
        sig.try_set(99);
        assert_eq!(counter.get(), 2, "no re-run after owner drop");
    }

    #[test]
    fn owner_insert_memo_lifetime_tied_to_owner() {
        // Inserting a memo through the owner allocates the memo's
        // slot in the owner *and* retains a clone so the memo's
        // context survives until the owner drops, even if the
        // user-side handle goes out of scope.
        let memo_snapshot = {
            let owner = Owner::new();
            let a = owner.insert(3_i32);
            let m = owner.insert_memo(move || a.get() * a.get());
            assert_eq!(m.snapshot(), 9);
            assert_eq!(owner.retained_len(), 1);
            // Drop the user-side memo handle. The owner still
            // retains a clone, so the memo's context stays live.
            let snap = m.snapshot();
            drop(m);
            // The owner drops below; the retained Memo clone goes
            // away with it.
            (snap, owner.retained_len())
        };
        // After owner drop the retained-len observation is from
        // before the drop; we just confirm the captured value was
        // sane while the owner was alive.
        assert_eq!(memo_snapshot, (9, 1));
    }

    #[test]
    fn owner_insert_effect_does_not_fire_after_owner_drop() {
        // A second guard: even if a foreign Signal (allocated on a
        // separate owner that *outlives* the owner-of-the-effect)
        // is written to, the effect must not re-run.
        let foreign_owner = Owner::new();
        let foreign_sig = foreign_owner.insert(0_i32);
        let counter = Rc::new(Cell::new(0));

        {
            let effect_owner = Owner::new();
            let counter_for = Rc::clone(&counter);
            effect_owner.insert_effect(move || {
                foreign_sig.track();
                counter_for.set(counter_for.get() + 1);
            });
            assert_eq!(counter.get(), 1);
            foreign_sig.set(1);
            assert_eq!(counter.get(), 2);
        } // effect_owner drops, the effect's context is disposed

        foreign_sig.set(2);
        assert_eq!(counter.get(), 2, "disposed effect did not re-run");
    }

    #[test]
    fn nested_reactive_context_inner_wins_for_subscription() {
        // While running inside an outer context, opening an inner
        // context with `reset_and_run_in` should make signal reads
        // subscribe the *inner* context, not the outer.
        let owner = Owner::new();
        let sig = owner.insert(0);
        let outer_runs = Rc::new(Cell::new(0));
        let inner_runs = Rc::new(Cell::new(0));
        let outer_for = Rc::clone(&outer_runs);
        let inner_for = Rc::clone(&inner_runs);

        let inner_ctx = ReactiveContext::new(move || {
            inner_for.set(inner_for.get() + 1);
        });
        let _outer = Effect::new(move || {
            outer_for.set(outer_for.get() + 1);
            inner_ctx.reset_and_run_in(|| {
                sig.read(|_| {});
            });
        });
        assert_eq!(outer_runs.get(), 1);
        assert_eq!(inner_runs.get(), 0, "inner ctx hasn't been dirtied yet");

        sig.set(1);
        // The signal was subscribed by inner_ctx (top of stack at
        // read time), so only inner fires. Outer is unaffected.
        assert_eq!(inner_runs.get(), 1);
        assert_eq!(outer_runs.get(), 1);
    }

    #[test]
    fn batch_collapses_multiple_writes_to_a_single_callback_run() {
        // Two writes to the same signal inside one batch wake the
        // subscriber exactly once. Without batching, every set fires
        // the dependent effect synchronously — the headline reason to
        // expose `batch` (Phase 8 open Q of dioxus-inspiration.md).
        let owner = Owner::new();
        let sig = owner.insert(0_i32);
        let runs = Rc::new(Cell::new(0));
        let runs_for_effect = Rc::clone(&runs);
        let _e = Effect::new(move || {
            sig.read(|_| {});
            runs_for_effect.set(runs_for_effect.get() + 1);
        });
        assert_eq!(runs.get(), 1, "effect runs once on construction");

        ReactiveContext::batch(|| {
            sig.set(1);
            sig.set(2);
            sig.set(3);
            // No subscriber wake-up during the batch.
            assert_eq!(runs.get(), 1);
        });
        assert_eq!(runs.get(), 2, "single drain after batch exits");
    }

    #[test]
    fn batch_dedups_dirty_set_across_multiple_signals() {
        // A single effect subscribed to two signals fires once per
        // batch even if both signals write — the per-id dedup in
        // `BATCH_PENDING` collapses the wave.
        let owner = Owner::new();
        let a = owner.insert(0_i32);
        let b = owner.insert(0_i32);
        let runs = Rc::new(Cell::new(0));
        let runs_for_effect = Rc::clone(&runs);
        let _e = Effect::new(move || {
            a.read(|_| {});
            b.read(|_| {});
            runs_for_effect.set(runs_for_effect.get() + 1);
        });
        assert_eq!(runs.get(), 1);

        ReactiveContext::batch(|| {
            a.set(10);
            b.set(20);
        });
        assert_eq!(runs.get(), 2, "one wake-up across both writes");
    }

    #[test]
    fn batch_nested_scopes_only_flush_at_outermost_exit() {
        let owner = Owner::new();
        let sig = owner.insert(0_i32);
        let runs = Rc::new(Cell::new(0));
        let runs_for_effect = Rc::clone(&runs);
        let _e = Effect::new(move || {
            sig.read(|_| {});
            runs_for_effect.set(runs_for_effect.get() + 1);
        });
        assert_eq!(runs.get(), 1);

        ReactiveContext::batch(|| {
            sig.set(1);
            ReactiveContext::batch(|| {
                sig.set(2);
                assert_eq!(runs.get(), 1);
            });
            // Inner scope exited; outer still open — no flush yet.
            assert_eq!(runs.get(), 1);
            sig.set(3);
        });
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn batch_returns_callback_value() {
        let n = ReactiveContext::batch(|| 7_i32 + 3);
        assert_eq!(n, 10);
    }

    #[test]
    fn batch_with_no_writes_does_not_run_callbacks() {
        let owner = Owner::new();
        let sig = owner.insert(0_i32);
        let runs = Rc::new(Cell::new(0));
        let runs_for_effect = Rc::clone(&runs);
        let _e = Effect::new(move || {
            sig.read(|_| {});
            runs_for_effect.set(runs_for_effect.get() + 1);
        });
        assert_eq!(runs.get(), 1);
        ReactiveContext::batch(|| {});
        assert_eq!(runs.get(), 1);
    }
}
