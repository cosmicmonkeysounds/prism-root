//! `atom` — fine-grained reactive cell with per-value subscribers.
//!
//! Phase 2 of the Dioxus-inspired reactive overhaul
//! (`docs/dev/dioxus-inspiration.md`): `Atom<T>` is now a thin
//! wrapper around `reactive::Owner` + a notification
//! `reactive::Signal<u64>` (a write-monotone tick counter) + a
//! `RefCell<T>` value cell + a direct-listener `Vec`. Two
//! independent subscriber channels coexist on the same write:
//!
//! 1. **Direct listeners** — `subscribe(callback)` registers an
//!    `FnMut(&T)` that fires synchronously on every mutating call.
//!    Same API as before; backwards compatible. Used by host shells
//!    and IPC clients that want explicit fire-and-forget callbacks
//!    without authoring an `Effect`.
//! 2. **Reactive subscribers** — every `set` / `update` /
//!    `force_notify` bumps the inner tick signal, so any `Effect` /
//!    `Memo` that called [`Atom::read`] / [`Atom::track`] /
//!    [`Atom::signal`].`track()` wakes automatically. This is the
//!    new path; the render walk (Phase 3) plumbs the atom through
//!    `read` and the substrate handles invalidation.
//!
//! [`Atom`] itself is `Clone`-cheap (an `Rc<AtomInner<T>>` inside),
//! which retires the `SharedAtom<T> = Rc<RefCell<Atom<T>>>` alias —
//! the outer `Rc<RefCell<…>>` was always cargo-culting a thread of
//! state through a single owner anyway. Cloning an `Atom` bumps the
//! `Rc`; all mutating methods take `&self`.
//!
//! The companion [`select`] / [`select_ref`] functions bridge
//! `Store<S>` to atoms: they install a store subscriber that
//! projects a field via a selector closure and only fires atom
//! subscribers when the projection changes. [`select_memo`] is the
//! preferred form for new code — it returns a `reactive::Memo<T>`
//! directly, the same shape every other Phase 1+ subscriber speaks.

use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;

use super::store::Store;
use crate::reactive::{Memo, Owner, Signal};

/// Handle returned by [`Atom::subscribe`]. Feed back to
/// [`Atom::unsubscribe`] to stop notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtomSubscription(u64);

impl AtomSubscription {
    pub fn raw(&self) -> u64 {
        self.0
    }
}

type AtomListener<T> = Box<dyn FnMut(&T)>;

struct AtomInner<T: 'static> {
    /// Current value. `RefCell` lets `&self` methods mutate it; the
    /// borrow checker keeps `read` / `peek` re-entrancy honest.
    value: RefCell<T>,
    /// Direct callback list. Independent from the reactive
    /// subscriber set on `tick`.
    listeners: RefCell<Vec<(u64, AtomListener<T>)>>,
    next_id: Cell<u64>,
    /// Owner for the notification signal — exists as long as the
    /// atom does. Cloning the atom doesn't allocate a new owner;
    /// dropping the last clone drops this and reclaims the signal
    /// slot.
    #[allow(dead_code)]
    owner: Owner,
    /// Monotone tick incremented on every mutating call. Reactive
    /// readers ([`Atom::read`], [`Atom::track`], or callers that
    /// hold the returned [`Atom::signal`]) subscribe to this and
    /// wake on the next mutation.
    tick: Signal<u64>,
}

/// A reactive cell with two independent subscriber channels
/// (direct listeners + reactive context auto-subscription).
///
/// `Atom<T>` is `Clone`-cheap (Rc-internal). All mutating methods
/// take `&self`; clones share state through the inner `Rc`.
pub struct Atom<T: 'static> {
    inner: Rc<AtomInner<T>>,
}

impl<T: 'static> Clone for Atom<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: 'static> Atom<T> {
    /// Construct a new atom with the given initial value.
    pub fn new(value: T) -> Self {
        let owner = Owner::new();
        let tick = owner.insert(0_u64);
        Self {
            inner: Rc::new(AtomInner {
                value: RefCell::new(value),
                listeners: RefCell::new(Vec::new()),
                next_id: Cell::new(0),
                owner,
                tick,
            }),
        }
    }

    /// Borrow the value (non-subscribing). The returned guard
    /// participates in `RefCell` borrow checking — keep it short.
    ///
    /// To both read and subscribe the current reactive context in
    /// one call, use [`Atom::read`].
    pub fn get(&self) -> Ref<'_, T> {
        self.inner.value.borrow()
    }

    /// Subscribing scoped read. Tracks the current reactive context
    /// (if any) against this atom's notification tick, then calls
    /// `f` with a borrowed reference.
    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.inner.tick.track();
        f(&self.inner.value.borrow())
    }

    /// Non-subscribing scoped read. Equivalent to `f(&*self.get())`
    /// but lets call sites match the [`Atom::read`] shape when the
    /// subscription is intentionally omitted.
    pub fn peek<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.value.borrow())
    }

    /// Subscribe the current reactive context to this atom's
    /// notification tick without producing a value. Pairs with
    /// [`Atom::peek`] / [`Atom::get`] for "I want to know *when* it
    /// changes, I'll read the value myself."
    pub fn track(&self) {
        self.inner.tick.track();
    }

    /// The notification signal. Reactive callers may subscribe to
    /// this directly via `atom.signal().track()` /
    /// `atom.signal().get()`; the carried `u64` is a monotone write
    /// counter, not the atom's value.
    pub fn signal(&self) -> Signal<u64> {
        self.inner.tick
    }

    /// Register a direct listener. Returns a handle for
    /// [`Atom::unsubscribe`].
    pub fn subscribe<F>(&self, listener: F) -> AtomSubscription
    where
        F: FnMut(&T) + 'static,
    {
        let id = self.inner.next_id.get();
        self.inner.next_id.set(id + 1);
        self.inner
            .listeners
            .borrow_mut()
            .push((id, Box::new(listener)));
        AtomSubscription(id)
    }

    pub fn unsubscribe(&self, sub: AtomSubscription) {
        self.inner
            .listeners
            .borrow_mut()
            .retain(|(id, _)| *id != sub.0);
    }

    pub fn subscriber_count(&self) -> usize {
        self.inner.listeners.borrow().len()
    }

    /// Mutate the value in place and unconditionally notify both
    /// subscriber channels.
    pub fn update<F: FnOnce(&mut T)>(&self, f: F) {
        f(&mut self.inner.value.borrow_mut());
        self.notify();
    }

    /// Force-notify both subscriber channels without changing the
    /// value.
    pub fn force_notify(&self) {
        self.notify();
    }

    /// Try to unwrap the inner `T`. Succeeds when no other clones
    /// of this `Atom` exist; otherwise returns the original.
    pub fn try_into_inner(self) -> Result<T, Self> {
        match Rc::try_unwrap(self.inner) {
            Ok(inner) => Ok(inner.value.into_inner()),
            Err(rc) => Err(Self { inner: rc }),
        }
    }

    /// Consume the atom and return the inner `T`. Panics if other
    /// clones of this atom are still outstanding.
    pub fn into_inner(self) -> T {
        self.try_into_inner()
            .unwrap_or_else(|_| panic!("Atom::into_inner: outstanding clones prevent unwrap"))
    }

    fn notify(&self) {
        // 1. Bump the reactive tick. Wakes every `Effect` / `Memo`
        //    that called `read`/`track`/`signal().track()`.
        let next = self.inner.tick.snapshot().wrapping_add(1);
        self.inner.tick.set(next);
        // 2. Fire the direct-listener channel.
        let value = self.inner.value.borrow();
        let mut listeners = self.inner.listeners.borrow_mut();
        for (_, listener) in listeners.iter_mut() {
            listener(&value);
        }
    }
}

impl<T: PartialEq + 'static> Atom<T> {
    /// Replace the value. Notifies subscribers only if the new
    /// value differs from the current one.
    pub fn set(&self, value: T) {
        if *self.inner.value.borrow() == value {
            return;
        }
        *self.inner.value.borrow_mut() = value;
        self.notify();
    }
}

impl<T: PartialEq + Default + 'static> Default for Atom<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: Clone + 'static> Atom<T> {
    /// Clone the current value out (non-subscribing).
    pub fn snapshot(&self) -> T {
        self.inner.value.borrow().clone()
    }
}

impl<T: PartialEq + Clone + 'static> Atom<T> {
    /// Mirror this atom's value into a reactive
    /// [`reactive::Signal<T>`](crate::reactive::Signal) allocated in
    /// `owner`. The mirror updates on every [`Atom::set`] /
    /// [`Atom::update`] / [`Atom::force_notify`].
    ///
    /// The bridge listener captures only the `Signal<T>` handle
    /// (a generational index), so the cost is constant. Writes use
    /// [`Signal::try_set`] so that if the signal's owner is dropped
    /// before the atom, the listener silently no-ops instead of
    /// panicking.
    ///
    /// Prefer this over [`Atom::signal`] when the reactive consumer
    /// outlives the atom or needs the value carried in the signal
    /// (the inner tick signal carries only a `u64`).
    pub fn reactive_signal(&self, owner: &Owner) -> Signal<T> {
        let signal = owner.insert(self.snapshot());
        self.subscribe(move |v| {
            signal.try_set(v.clone());
        });
        signal
    }
}

/// Create an [`Atom<T>`] that tracks a projected field of a
/// [`Store<S>`]. A store subscriber runs the `selector` on every
/// dispatch and calls `Atom::set` with the result. Because `set`
/// checks `PartialEq`, downstream atom subscribers only fire when
/// the specific projected value actually changes.
///
/// ```ignore
/// let panel = select(&mut store, |s| s.active_panel);
/// panel.subscribe(|p| { /* fires only on panel change */ });
/// ```
pub fn select<S, T, F>(store: &mut Store<S>, selector: F) -> Atom<T>
where
    S: 'static,
    T: PartialEq + Clone + 'static,
    F: Fn(&S) -> T + 'static,
{
    let initial = selector(store.state());
    let atom = Atom::new(initial);
    let atom_for_sub = atom.clone();
    store.subscribe(move |state| {
        let next = selector(state);
        atom_for_sub.set(next);
    });
    atom
}

/// Like [`select`] but for selectors that return a reference.
/// Clones the value on every store dispatch, but only fires atom
/// subscribers when the clone differs from the previous value.
pub fn select_ref<S, T, F>(store: &mut Store<S>, selector: F) -> Atom<T>
where
    S: 'static,
    T: PartialEq + Clone + 'static,
    F: Fn(&S) -> &T + 'static,
{
    let initial = selector(store.state()).clone();
    let atom = Atom::new(initial);
    let atom_for_sub = atom.clone();
    store.subscribe(move |state| {
        let next = selector(state).clone();
        atom_for_sub.set(next);
    });
    atom
}

/// Phase 2 sibling to [`select`]: project a `Store<S>` field into
/// a reactive [`Memo<T>`] instead of an `Atom<T>`. Same projection
/// semantics, but the result speaks the universal reactive
/// substrate shape (read inside an `Effect` / `Memo` to
/// auto-subscribe).
///
/// Allocates the memo's slot in `owner`. The memo recomputes on
/// every store dispatch but PartialEq-gates downstream
/// notification so equal projections don't wake subscribers.
pub fn select_memo<S, T, F>(store: &mut Store<S>, owner: &Owner, selector: F) -> Memo<T>
where
    S: 'static,
    T: PartialEq + Clone + 'static,
    F: Fn(&S) -> T + 'static,
{
    // The memo's body reads from a `Signal<T>` we keep in sync
    // with the projection. The store dispatch is the dirty source;
    // the signal carries the actual value into the reactive graph.
    let initial = selector(store.state());
    let projection = owner.insert(initial);
    let projection_for_sub = projection;
    store.subscribe(move |state| {
        let next = selector(state);
        // Only set if changed — Signal::set is unconditional, but
        // we want PartialEq gating so downstream memos that read
        // through this don't see redundant ticks.
        projection_for_sub.write(|cur| {
            if *cur != next {
                *cur = next;
            }
        });
    });
    Memo::new(owner, move || projection.get())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::store::Action;
    use crate::reactive::Effect;
    use std::cell::Cell;

    #[test]
    fn atom_starts_with_initial_value() {
        let atom = Atom::new(42);
        assert_eq!(*atom.get(), 42);
        assert_eq!(atom.subscriber_count(), 0);
    }

    #[test]
    fn set_notifies_on_change() {
        let atom = Atom::new(0);
        let count = Rc::new(Cell::new(0usize));
        let cc = count.clone();
        atom.subscribe(move |_| cc.set(cc.get() + 1));

        atom.set(1);
        assert_eq!(count.get(), 1);
        assert_eq!(*atom.get(), 1);
    }

    #[test]
    fn set_suppresses_when_equal() {
        let atom = Atom::new(5);
        let count = Rc::new(Cell::new(0usize));
        let cc = count.clone();
        atom.subscribe(move |_| cc.set(cc.get() + 1));

        atom.set(5);
        assert_eq!(count.get(), 0);
    }

    #[test]
    fn update_always_notifies() {
        let atom = Atom::new(10);
        let count = Rc::new(Cell::new(0usize));
        let cc = count.clone();
        atom.subscribe(move |_| cc.set(cc.get() + 1));

        atom.update(|v| *v += 0);
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn force_notify_fires_subscribers() {
        let atom = Atom::new(7);
        let count = Rc::new(Cell::new(0usize));
        let cc = count.clone();
        atom.subscribe(move |_| cc.set(cc.get() + 1));

        atom.force_notify();
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn unsubscribe_stops_notifications() {
        let atom = Atom::new(0);
        let count = Rc::new(Cell::new(0usize));
        let cc = count.clone();
        let sub = atom.subscribe(move |_| cc.set(cc.get() + 1));

        atom.set(1);
        assert_eq!(count.get(), 1);

        atom.unsubscribe(sub);
        atom.set(2);
        assert_eq!(count.get(), 1);
        assert_eq!(atom.subscriber_count(), 0);
    }

    #[test]
    fn multiple_subscribers_fire_in_order() {
        let atom = Atom::new(0);
        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let a = log.clone();
        let b = log.clone();
        atom.subscribe(move |_| a.borrow_mut().push("a"));
        atom.subscribe(move |_| b.borrow_mut().push("b"));

        atom.set(1);
        assert_eq!(&*log.borrow(), &["a", "b"]);
    }

    #[test]
    fn into_inner_yields_value() {
        let atom = Atom::new("hello".to_string());
        assert_eq!(atom.into_inner(), "hello");
    }

    #[test]
    fn try_into_inner_returns_self_when_shared() {
        let atom = Atom::new(42);
        let clone = atom.clone();
        let result = atom.try_into_inner();
        assert!(result.is_err(), "expected Err while clone outstanding");
        drop(clone);
        // Re-acquire the original via the returned Err and unwrap now.
        let recovered = result.unwrap_err();
        assert_eq!(recovered.into_inner(), 42);
    }

    #[test]
    fn default_uses_t_default() {
        let atom: Atom<i32> = Atom::default();
        assert_eq!(*atom.get(), 0);
    }

    #[test]
    fn subscriber_sees_new_value() {
        let atom = Atom::new(0);
        let seen = Rc::new(Cell::new(0));
        let sc = seen.clone();
        atom.subscribe(move |v| sc.set(*v));

        atom.set(42);
        assert_eq!(seen.get(), 42);
    }

    #[test]
    fn unsubscribe_unknown_is_noop() {
        let atom: Atom<i32> = Atom::new(0);
        atom.unsubscribe(AtomSubscription(999));
        assert_eq!(atom.subscriber_count(), 0);
    }

    #[test]
    fn unsubscribe_one_leaves_others_intact() {
        let atom = Atom::new(0);
        let a = Rc::new(Cell::new(0usize));
        let b = Rc::new(Cell::new(0usize));
        let ac = a.clone();
        let bc = b.clone();
        let sub_a = atom.subscribe(move |_| ac.set(ac.get() + 1));
        atom.subscribe(move |_| bc.set(bc.get() + 1));

        atom.set(1);
        atom.unsubscribe(sub_a);
        atom.set(2);

        assert_eq!(a.get(), 1);
        assert_eq!(b.get(), 2);
        assert_eq!(atom.subscriber_count(), 1);
    }

    #[test]
    fn subscription_ids_are_unique() {
        let atom: Atom<i32> = Atom::new(0);
        let a = atom.subscribe(|_| {});
        let b = atom.subscribe(|_| {});
        atom.unsubscribe(a);
        let c = atom.subscribe(|_| {});
        assert_ne!(a.raw(), b.raw());
        assert_ne!(b.raw(), c.raw());
        assert_ne!(a.raw(), c.raw());
    }

    #[test]
    fn atom_is_clone_cheap() {
        let atom = Atom::new(7_i32);
        let cloned = atom.clone();
        atom.set(99);
        assert_eq!(*cloned.get(), 99, "clones share state");
    }

    // ── reactive::Signal subscriber channel ─────────────────────

    #[test]
    fn reactive_read_subscribes_to_atom() {
        let atom = Atom::new(0);
        let runs = Rc::new(Cell::new(0));
        let runs_for = Rc::clone(&runs);
        let atom_for = atom.clone();
        let _e = Effect::new(move || {
            atom_for.read(|_v| {});
            runs_for.set(runs_for.get() + 1);
        });
        assert_eq!(runs.get(), 1, "initial run");

        atom.set(1);
        assert_eq!(runs.get(), 2);

        atom.set(2);
        assert_eq!(runs.get(), 3);
    }

    #[test]
    fn reactive_track_wakes_without_reading_value() {
        let atom = Atom::new("a".to_string());
        let runs = Rc::new(Cell::new(0));
        let runs_for = Rc::clone(&runs);
        let atom_for = atom.clone();
        let _e = Effect::new(move || {
            atom_for.track();
            runs_for.set(runs_for.get() + 1);
        });
        assert_eq!(runs.get(), 1);
        atom.set("b".into());
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn reactive_peek_does_not_subscribe() {
        let atom = Atom::new(0);
        let runs = Rc::new(Cell::new(0));
        let runs_for = Rc::clone(&runs);
        let atom_for = atom.clone();
        let _e = Effect::new(move || {
            atom_for.peek(|_v| {});
            runs_for.set(runs_for.get() + 1);
        });
        assert_eq!(runs.get(), 1);
        atom.set(1);
        assert_eq!(runs.get(), 1, "peek didn't subscribe");
    }

    #[test]
    fn force_notify_wakes_reactive_subscribers() {
        let atom = Atom::new(0);
        let runs = Rc::new(Cell::new(0));
        let runs_for = Rc::clone(&runs);
        let atom_for = atom.clone();
        let _e = Effect::new(move || {
            atom_for.read(|_| {});
            runs_for.set(runs_for.get() + 1);
        });
        assert_eq!(runs.get(), 1);
        atom.force_notify();
        assert_eq!(runs.get(), 2);
    }

    // ── select tests ────────────────────────────────────────────

    #[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct TestState {
        count: i32,
        label: String,
    }

    struct Increment;
    impl Action<TestState> for Increment {
        fn apply(self, state: &mut TestState) {
            state.count += 1;
        }
    }

    struct SetLabel(String);
    impl Action<TestState> for SetLabel {
        fn apply(self, state: &mut TestState) {
            state.label = self.0;
        }
    }

    #[test]
    fn select_creates_atom_with_initial_value() {
        let mut store = Store::new(TestState {
            count: 5,
            label: "hi".into(),
        });
        let count_atom = select(&mut store, |s| s.count);
        assert_eq!(*count_atom.get(), 5);
    }

    #[test]
    fn select_updates_atom_on_store_dispatch() {
        let mut store = Store::new(TestState::default());
        let count_atom = select(&mut store, |s| s.count);

        store.dispatch(Increment);
        assert_eq!(*count_atom.get(), 1);

        store.dispatch(Increment);
        assert_eq!(*count_atom.get(), 2);
    }

    #[test]
    fn select_fires_atom_subscribers_only_on_projected_change() {
        let mut store = Store::new(TestState::default());
        let count_atom = select(&mut store, |s| s.count);

        let fires = Rc::new(Cell::new(0usize));
        let fc = fires.clone();
        count_atom.subscribe(move |_| fc.set(fc.get() + 1));

        store.dispatch(SetLabel("new".into()));
        assert_eq!(fires.get(), 0);

        store.dispatch(Increment);
        assert_eq!(fires.get(), 1);
    }

    #[test]
    fn select_ref_works_with_string_field() {
        let mut store = Store::new(TestState {
            count: 0,
            label: "initial".into(),
        });
        let label_atom = select_ref(&mut store, |s| &s.label);
        assert_eq!(*label_atom.get(), "initial");

        store.dispatch(SetLabel("updated".into()));
        assert_eq!(*label_atom.get(), "updated");
    }

    #[test]
    fn multiple_selectors_work_independently() {
        let mut store = Store::new(TestState::default());
        let count_atom = select(&mut store, |s| s.count);
        let label_atom = select(&mut store, |s| s.label.clone());

        let count_fires = Rc::new(Cell::new(0usize));
        let label_fires = Rc::new(Cell::new(0usize));
        let cf = count_fires.clone();
        let lf = label_fires.clone();
        count_atom.subscribe(move |_| cf.set(cf.get() + 1));
        label_atom.subscribe(move |_| lf.set(lf.get() + 1));

        store.dispatch(Increment);
        assert_eq!(count_fires.get(), 1);
        assert_eq!(label_fires.get(), 0);

        store.dispatch(SetLabel("x".into()));
        assert_eq!(count_fires.get(), 1);
        assert_eq!(label_fires.get(), 1);
    }

    #[test]
    fn select_chained_with_atom_subscriber() {
        let mut store = Store::new(TestState::default());
        let count_atom = select(&mut store, |s| s.count);

        let doubled = Rc::new(Cell::new(0));
        let dc = doubled.clone();
        count_atom.subscribe(move |v| dc.set(*v * 2));

        store.dispatch(Increment);
        store.dispatch(Increment);
        store.dispatch(Increment);
        assert_eq!(doubled.get(), 6);
    }

    #[test]
    fn select_with_store_replace() {
        let mut store = Store::new(TestState::default());
        let count_atom = select(&mut store, |s| s.count);

        store.replace(TestState {
            count: 99,
            label: "replaced".into(),
        });
        assert_eq!(*count_atom.get(), 99);
    }

    // ── select_memo tests ───────────────────────────────────────

    #[test]
    fn select_memo_initial_value_matches_projection() {
        let owner = Owner::new();
        let mut store = Store::new(TestState {
            count: 7,
            label: "x".into(),
        });
        let count_memo = select_memo(&mut store, &owner, |s| s.count);
        assert_eq!(count_memo.snapshot(), 7);
    }

    #[test]
    fn select_memo_recomputes_on_store_dispatch() {
        let owner = Owner::new();
        let mut store = Store::new(TestState::default());
        let count_memo = select_memo(&mut store, &owner, |s| s.count);
        let count_sig = count_memo.signal();

        let fires = Rc::new(Cell::new(0usize));
        let fc = Rc::clone(&fires);
        let _e = Effect::new(move || {
            count_sig.track();
            fc.set(fc.get() + 1);
        });
        assert_eq!(fires.get(), 1, "initial");

        store.dispatch(Increment);
        assert_eq!(count_memo.snapshot(), 1);
        assert_eq!(fires.get(), 2);
    }

    #[test]
    fn select_memo_partial_eq_gates_unchanged_projection() {
        let owner = Owner::new();
        let mut store = Store::new(TestState::default());
        let count_memo = select_memo(&mut store, &owner, |s| s.count);
        let count_sig = count_memo.signal();

        let fires = Rc::new(Cell::new(0usize));
        let fc = Rc::clone(&fires);
        let _e = Effect::new(move || {
            count_sig.track();
            fc.set(fc.get() + 1);
        });
        assert_eq!(fires.get(), 1);

        // Dispatch changes label only; the count projection is
        // unchanged so the memo body produces the same value and
        // downstream PartialEq gating suppresses the wake.
        store.dispatch(SetLabel("new".into()));
        assert_eq!(fires.get(), 1, "label change did not wake count memo");

        store.dispatch(Increment);
        assert_eq!(fires.get(), 2);
    }

    // ------------------------------------------------------------
    // Phase 2 — reactive::Signal bridge (cross-owner mirror)
    // ------------------------------------------------------------

    #[test]
    fn reactive_signal_starts_with_atom_value() {
        let atom = Atom::new(42);
        let owner = Owner::new();
        let signal = atom.reactive_signal(&owner);
        assert_eq!(signal.snapshot(), 42);
    }

    #[test]
    fn reactive_signal_updates_when_atom_set() {
        let atom = Atom::new(0);
        let owner = Owner::new();
        let signal = atom.reactive_signal(&owner);

        atom.set(5);
        assert_eq!(signal.snapshot(), 5);

        atom.set(7);
        assert_eq!(signal.snapshot(), 7);
    }

    #[test]
    fn reactive_signal_skips_when_atom_equal_value() {
        let atom = Atom::new(3);
        let owner = Owner::new();
        let signal = atom.reactive_signal(&owner);

        let fires = Rc::new(Cell::new(0));
        let fires_for = fires.clone();
        let _e = Effect::new(move || {
            signal.read(|_| {});
            fires_for.set(fires_for.get() + 1);
        });
        assert_eq!(fires.get(), 1, "initial run");

        atom.set(3);
        assert_eq!(fires.get(), 1, "no fire on equal set");

        atom.set(4);
        assert_eq!(fires.get(), 2);
    }

    #[test]
    fn reactive_effect_wakes_on_atom_change() {
        let atom = Atom::new("hello".to_string());
        let owner = Owner::new();
        let signal = atom.reactive_signal(&owner);

        let captured: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
        let captured_for = captured.clone();
        let _e = Effect::new(move || {
            let v = signal.get();
            *captured_for.borrow_mut() = v;
        });
        assert_eq!(*captured.borrow(), "hello");

        atom.set("world".into());
        assert_eq!(*captured.borrow(), "world");
    }

    #[test]
    fn reactive_signal_owner_drop_does_not_panic_atom_listener() {
        // If the Signal's Owner is dropped before the Atom, the
        // bridge listener should silently no-op via try_set. The
        // Atom can keep being used.
        let atom = Atom::new(0);
        {
            let owner = Owner::new();
            let _signal = atom.reactive_signal(&owner);
            atom.set(1); // listener writes into the live signal
        } // owner drops here; signal slot is reclaimed
          // This must not panic — the listener tries to write into a
          // dead slot and falls through.
        atom.set(2);
        atom.set(3);
        assert_eq!(*atom.get(), 3);
    }
}
