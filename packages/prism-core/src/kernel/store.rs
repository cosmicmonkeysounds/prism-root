//! `store` — generic `Store<S>` + `Action<S>` trait + subscription
//! bus. Ports the zustand + reducer-bus shape the TS tree had onto
//! pure Rust, with no framework coupling.
//!
//! Per §6.1 of `docs/dev/slint-migration-plan.md` we are *not* porting
//! zustand — we are rewriting the tiny part of it the codebase
//! actually uses: a single owning container for app state, a way to
//! mutate it via serializable actions, and a synchronous
//! subscription bus for UI redraw fan-out.
//!
//! §7 hot-reload constraints the store is designed to satisfy:
//!
//! 1. **One root state struct.** `Store<S>` is parameterised on a
//!    single `S`. Everything reloadable lives inside that `S`;
//!    snapshot/restore is exactly one serde call.
//! 2. **No global mutable state.** The store owns its state by value
//!    and is itself a plain struct — no `OnceCell`, no `static mut`.
//!    Hosts are expected to hold the store inside their main event
//!    loop (on the stack or in a local `RefCell`).
//! 3. **Subscribers are owned, not global.** Listeners live in a
//!    `Vec` inside the store. They are dropped with the store and
//!    rebuilt with it across a hot reload.
//! 4. **Actions round-trip serde.** [`Action<S>`] exists so actions
//!    can later be logged, replayed, or sent over IPC without
//!    leaking reducer closures. Ad-hoc mutations still work via
//!    [`Store::mutate`] for the cases where action boilerplate would
//!    be overkill.
//!
//! The API is deliberately single-threaded. The shell reducer runs
//! on the host event loop thread; background work goes through the
//! `kernel::actor` message bus (Phase 2 TODO) and marshals state
//! changes back as actions.

use std::marker::PhantomData;

/// A serialisable, replayable mutation against a store state of
/// type `S`. Implementors are typically plain data — an enum
/// listing every state transition the host supports — which makes
/// them free to log, replay on reload, or ship over IPC.
///
/// The contract is intentionally narrow: `apply` gets `&mut S` and
/// nothing else. No access to the subscription bus, no back-channel
/// to the store, no async. Actions that need to kick off side
/// effects (IO, network, actor messages) should mutate `S` with an
/// "intent" field and let an outer loop observe the change via
/// [`Store::subscribe`].
pub trait Action<S> {
    fn apply(self, state: &mut S);
}

/// Handle returned by [`Store::subscribe`] that can be used to
/// unsubscribe later. Opaque on purpose so callers can't forge or
/// compare ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscription(u64);

impl Subscription {
    /// Raw id exposed for diagnostics / snapshot tests. Not part of
    /// the public contract — do not round-trip through it.
    pub fn raw(&self) -> u64 {
        self.0
    }
}

type Listener<S> = Box<dyn FnMut(&S)>;

/// Owning container for the single reloadable root state `S`.
///
/// Create with [`Store::new`], mutate with [`Store::dispatch`] or
/// [`Store::mutate`], read with [`Store::state`], and subscribe for
/// change notifications with [`Store::subscribe`]. For hot-reload,
/// use [`Store::snapshot`] / [`Store::restore`] when `S: Serialize
/// + DeserializeOwned`.
pub struct Store<S> {
    state: S,
    listeners: Vec<(u64, Listener<S>)>,
    next_listener_id: u64,
    _marker: PhantomData<S>,
}

impl<S> Store<S> {
    /// Wrap an initial state in a fresh store with no subscribers.
    pub fn new(state: S) -> Self {
        Self {
            state,
            listeners: Vec::new(),
            next_listener_id: 0,
            _marker: PhantomData,
        }
    }

    /// Borrow the current state. Read-only — all mutation goes
    /// through [`dispatch`](Store::dispatch) / [`mutate`](Store::mutate)
    /// so subscribers stay in sync.
    pub fn state(&self) -> &S {
        &self.state
    }

    /// Apply an [`Action`] to the state and notify every subscriber.
    ///
    /// Prefer this over [`mutate`](Store::mutate) whenever the
    /// mutation is structured enough to name — serialisable actions
    /// are what unlock replay, logging, and cross-transport
    /// shipping.
    pub fn dispatch<A: Action<S>>(&mut self, action: A) {
        action.apply(&mut self.state);
        self.notify();
    }

    /// Escape hatch for ad-hoc mutation. Runs `f` against `&mut S`
    /// and notifies subscribers afterward. Use sparingly — a named
    /// [`Action`] is better almost everywhere.
    pub fn mutate<F>(&mut self, f: F)
    where
        F: FnOnce(&mut S),
    {
        f(&mut self.state);
        self.notify();
    }

    /// Swap the entire state out for `next` and notify subscribers.
    /// This is the path hot-reload uses after deserializing a
    /// snapshot — the subscriber list is preserved so existing
    /// renderers keep receiving notifications.
    pub fn replace(&mut self, next: S) {
        self.state = next;
        self.notify();
    }

    /// Register a listener that runs synchronously on every
    /// dispatch/mutate/replace. Returns a [`Subscription`] handle
    /// the caller can later hand back to [`unsubscribe`](Store::unsubscribe).
    ///
    /// Listeners fire in registration order. They do **not** fire
    /// on [`Store::new`] — the host is responsible for the initial
    /// paint.
    pub fn subscribe<F>(&mut self, listener: F) -> Subscription
    where
        F: FnMut(&S) + 'static,
    {
        let id = self.next_listener_id;
        self.next_listener_id += 1;
        self.listeners.push((id, Box::new(listener)));
        Subscription(id)
    }

    /// Drop a previously registered listener. No-op if the
    /// subscription is unknown (e.g. already unsubscribed, or
    /// restored from a snapshot that cleared the list).
    pub fn unsubscribe(&mut self, subscription: Subscription) {
        self.listeners.retain(|(id, _)| *id != subscription.0);
    }

    /// Number of live subscribers. Exposed primarily for diagnostics
    /// and tests.
    pub fn subscriber_count(&self) -> usize {
        self.listeners.len()
    }

    /// Consume the store and return the inner state. Useful for
    /// hosts that need to hand `S` off to another owner — e.g. a
    /// hot-reload swap that rebuilds the store around the same
    /// state value.
    pub fn into_inner(self) -> S {
        self.state
    }

    fn notify(&mut self) {
        for (_, listener) in self.listeners.iter_mut() {
            listener(&self.state);
        }
    }
}

impl<S: Default> Default for Store<S> {
    fn default() -> Self {
        Self::new(S::default())
    }
}

// ── Hot-reload snapshot / restore ───────────────────────────────
//
// These land behind a `where` bound on `S` so the store itself
// doesn't require serde. Hosts that want snapshot/restore (the shell
// does) put `Serialize + DeserializeOwned` on their `AppState` and
// call these directly; hosts that don't, don't pay.

impl<S> Store<S>
where
    S: serde::Serialize,
{
    /// Serialise the current state to a JSON byte blob. The shell
    /// persists this into the browser's `localStorage` (web) or a
    /// sidecar file (native) across a hot reload, per §7.
    ///
    /// JSON is chosen over postcard here because the snapshot is
    /// written by humans as often as machines during the migration:
    /// readable diffs are worth the size cost. Flip to postcard if
    /// the snapshot ever grows past the point where that matters.
    pub fn snapshot(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.state)
    }
}

impl<S> Store<S>
where
    S: serde::de::DeserializeOwned,
{
    /// Deserialise `bytes` into a fresh state value and
    /// [`replace`](Store::replace) the store's state with it. Fires
    /// every subscriber exactly once on success.
    pub fn restore(&mut self, bytes: &[u8]) -> Result<(), serde_json::Error> {
        let next = serde_json::from_slice::<S>(bytes)?;
        self.replace(next);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Counter {
        value: i32,
        label: String,
    }

    enum CounterAction {
        Increment,
        IncrementBy(i32),
        SetLabel(String),
    }

    impl Action<Counter> for CounterAction {
        fn apply(self, state: &mut Counter) {
            match self {
                CounterAction::Increment => state.value += 1,
                CounterAction::IncrementBy(n) => state.value += n,
                CounterAction::SetLabel(label) => state.label = label,
            }
        }
    }

    #[test]
    fn new_starts_with_provided_state() {
        let store = Store::new(Counter {
            value: 7,
            label: "seed".into(),
        });
        assert_eq!(store.state().value, 7);
        assert_eq!(store.state().label, "seed");
        assert_eq!(store.subscriber_count(), 0);
    }

    #[test]
    fn default_starts_with_default_state() {
        let store: Store<Counter> = Store::default();
        assert_eq!(store.state(), &Counter::default());
    }

    #[test]
    fn dispatch_applies_action_and_notifies() {
        let mut store = Store::new(Counter::default());
        let seen = Rc::new(RefCell::new(Vec::<i32>::new()));
        let seen_clone = seen.clone();
        store.subscribe(move |s| seen_clone.borrow_mut().push(s.value));

        store.dispatch(CounterAction::Increment);
        store.dispatch(CounterAction::IncrementBy(5));

        assert_eq!(store.state().value, 6);
        assert_eq!(&*seen.borrow(), &[1, 6]);
    }

    #[test]
    fn mutate_updates_state_and_notifies() {
        let mut store = Store::new(Counter::default());
        let seen = Rc::new(RefCell::new(0usize));
        let seen_clone = seen.clone();
        store.subscribe(move |_| *seen_clone.borrow_mut() += 1);

        store.mutate(|c| c.value = 42);

        assert_eq!(store.state().value, 42);
        assert_eq!(*seen.borrow(), 1);
    }

    #[test]
    fn replace_swaps_state_and_notifies() {
        let mut store = Store::new(Counter::default());
        let seen = Rc::new(RefCell::new(0usize));
        let seen_clone = seen.clone();
        store.subscribe(move |_| *seen_clone.borrow_mut() += 1);

        store.replace(Counter {
            value: 99,
            label: "replaced".into(),
        });

        assert_eq!(store.state().value, 99);
        assert_eq!(store.state().label, "replaced");
        assert_eq!(*seen.borrow(), 1);
    }

    #[test]
    fn multiple_subscribers_fire_in_registration_order() {
        let mut store = Store::new(Counter::default());
        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let a = log.clone();
        let b = log.clone();
        let c = log.clone();
        store.subscribe(move |_| a.borrow_mut().push("a"));
        store.subscribe(move |_| b.borrow_mut().push("b"));
        store.subscribe(move |_| c.borrow_mut().push("c"));

        store.dispatch(CounterAction::Increment);

        assert_eq!(&*log.borrow(), &["a", "b", "c"]);
    }

    #[test]
    fn unsubscribe_stops_notifications() {
        let mut store = Store::new(Counter::default());
        let calls = Rc::new(RefCell::new(0usize));
        let calls_clone = calls.clone();
        let sub = store.subscribe(move |_| *calls_clone.borrow_mut() += 1);

        store.dispatch(CounterAction::Increment);
        store.unsubscribe(sub);
        store.dispatch(CounterAction::Increment);

        assert_eq!(*calls.borrow(), 1);
        assert_eq!(store.subscriber_count(), 0);
    }

    #[test]
    fn unsubscribe_unknown_is_noop() {
        let mut store: Store<Counter> = Store::default();
        // Forge an id that was never issued — should be safely ignored.
        store.unsubscribe(Subscription(999));
        assert_eq!(store.subscriber_count(), 0);
    }

    #[test]
    fn unsubscribe_one_leaves_others_intact() {
        let mut store = Store::new(Counter::default());
        let a_calls = Rc::new(RefCell::new(0usize));
        let b_calls = Rc::new(RefCell::new(0usize));
        let ac = a_calls.clone();
        let bc = b_calls.clone();
        let a_sub = store.subscribe(move |_| *ac.borrow_mut() += 1);
        store.subscribe(move |_| *bc.borrow_mut() += 1);

        store.dispatch(CounterAction::Increment);
        store.unsubscribe(a_sub);
        store.dispatch(CounterAction::Increment);

        assert_eq!(*a_calls.borrow(), 1);
        assert_eq!(*b_calls.borrow(), 2);
        assert_eq!(store.subscriber_count(), 1);
    }

    #[test]
    fn subscribers_see_post_dispatch_state() {
        let mut store = Store::new(Counter::default());
        let captured = Rc::new(RefCell::new(Counter::default()));
        let captured_clone = captured.clone();
        store.subscribe(move |s| *captured_clone.borrow_mut() = s.clone());

        store.dispatch(CounterAction::IncrementBy(11));

        assert_eq!(captured.borrow().value, 11);
    }

    #[test]
    fn snapshot_restore_round_trips() {
        let mut store = Store::new(Counter {
            value: 3,
            label: "before".into(),
        });
        let bytes = store.snapshot().expect("snapshot");

        store.dispatch(CounterAction::SetLabel("after".into()));
        store.dispatch(CounterAction::IncrementBy(10));
        assert_eq!(store.state().value, 13);
        assert_eq!(store.state().label, "after");

        store.restore(&bytes).expect("restore");
        assert_eq!(store.state().value, 3);
        assert_eq!(store.state().label, "before");
    }

    #[test]
    fn restore_notifies_subscribers() {
        let mut store = Store::new(Counter::default());
        let bytes = serde_json::to_vec(&Counter {
            value: 42,
            label: "from-disk".into(),
        })
        .unwrap();
        let calls = Rc::new(RefCell::new(0usize));
        let calls_clone = calls.clone();
        store.subscribe(move |_| *calls_clone.borrow_mut() += 1);

        store.restore(&bytes).expect("restore");

        assert_eq!(store.state().value, 42);
        assert_eq!(store.state().label, "from-disk");
        assert_eq!(*calls.borrow(), 1);
    }

    #[test]
    fn restore_rejects_invalid_bytes() {
        let mut store = Store::new(Counter {
            value: 5,
            label: "keep".into(),
        });
        let err = store.restore(b"not-json");
        assert!(err.is_err());
        // State survives a failed restore.
        assert_eq!(store.state().value, 5);
        assert_eq!(store.state().label, "keep");
    }

    #[test]
    fn into_inner_yields_state() {
        let store = Store::new(Counter {
            value: 1,
            label: "x".into(),
        });
        let inner = store.into_inner();
        assert_eq!(inner.value, 1);
        assert_eq!(inner.label, "x");
    }

    #[test]
    fn subscription_ids_are_unique_across_unsubscribe() {
        let mut store: Store<Counter> = Store::default();
        let a = store.subscribe(|_| {});
        let b = store.subscribe(|_| {});
        store.unsubscribe(a);
        let c = store.subscribe(|_| {});
        assert_ne!(a.raw(), b.raw());
        assert_ne!(b.raw(), c.raw());
        assert_ne!(a.raw(), c.raw());
    }

    // ── Hot-reload simulation ────────────────────────────────────
    //
    // Models the §7 inner loop: serialise state, tear the store
    // down, rebuild it (fresh listener list), restore, and confirm
    // new subscribers see subsequent dispatches.

    #[test]
    fn hot_reload_cycle_preserves_state_and_accepts_new_subscribers() {
        let mut store = Store::new(Counter {
            value: 10,
            label: "pre-reload".into(),
        });
        store.subscribe(|_| { /* will not survive reload */ });
        let bytes = store.snapshot().unwrap();

        // Tear down + rebuild: new `Store`, no listeners.
        let restored: Counter = serde_json::from_slice(&bytes).unwrap();
        let mut reloaded = Store::new(restored);
        assert_eq!(reloaded.subscriber_count(), 0);
        assert_eq!(reloaded.state().value, 10);
        assert_eq!(reloaded.state().label, "pre-reload");

        let seen = Rc::new(RefCell::new(0i32));
        let seen_clone = seen.clone();
        reloaded.subscribe(move |s| *seen_clone.borrow_mut() = s.value);
        reloaded.dispatch(CounterAction::Increment);

        assert_eq!(reloaded.state().value, 11);
        assert_eq!(*seen.borrow(), 11);
    }
}
