//! `atom` — fine-grained reactive cell with per-value subscribers.
//!
//! `Atom<T>` is the high-frequency update path for UI rendering.
//! Where `Store<S>` fires every subscriber on every dispatch
//! (whole-state), an `Atom<T>` only fires when its specific value
//! actually changes (via `PartialEq`). This makes it safe to
//! create many atoms — one per UI field, one per CRDT object —
//! without drowning the listener bus in redundant notifications.
//!
//! The companion [`select`] function bridges `Store<S>` to atoms:
//! it installs a store subscriber that projects a field via a
//! selector closure and only fires atom subscribers when the
//! projection changes.

use std::cell::RefCell;
use std::rc::Rc;

use super::store::Store;

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

/// A reactive cell that notifies subscribers only when its value
/// changes. `T: PartialEq` is required so `set` can suppress
/// redundant notifications.
///
/// Single-threaded by design — same constraint as `Store<S>`.
pub struct Atom<T> {
    value: T,
    listeners: Vec<(u64, AtomListener<T>)>,
    next_id: u64,
}

impl<T: PartialEq> Atom<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            listeners: Vec::new(),
            next_id: 0,
        }
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    /// Replace the value. Notifies subscribers only if the new
    /// value differs from the current one.
    pub fn set(&mut self, value: T) {
        if self.value != value {
            self.value = value;
            self.notify();
        }
    }

    /// Mutate the value in place and unconditionally notify.
    pub fn update<F: FnOnce(&mut T)>(&mut self, f: F) {
        f(&mut self.value);
        self.notify();
    }

    /// Force-notify all subscribers regardless of whether the value
    /// changed.
    pub fn force_notify(&mut self) {
        self.notify();
    }

    pub fn subscribe<F>(&mut self, listener: F) -> AtomSubscription
    where
        F: FnMut(&T) + 'static,
    {
        let id = self.next_id;
        self.next_id += 1;
        self.listeners.push((id, Box::new(listener)));
        AtomSubscription(id)
    }

    pub fn unsubscribe(&mut self, sub: AtomSubscription) {
        self.listeners.retain(|(id, _)| *id != sub.0);
    }

    pub fn subscriber_count(&self) -> usize {
        self.listeners.len()
    }

    pub fn into_inner(self) -> T {
        self.value
    }

    fn notify(&mut self) {
        for (_, listener) in self.listeners.iter_mut() {
            listener(&self.value);
        }
    }
}

impl<T: PartialEq + Default> Default for Atom<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// A shared, reference-counted atom. This is the type [`select`]
/// returns and the type UI code typically holds.
pub type SharedAtom<T> = Rc<RefCell<Atom<T>>>;

/// Create a [`SharedAtom<T>`] that tracks a projected field of a
/// [`Store<S>`]. A store subscriber runs the `selector` on every
/// dispatch and calls `Atom::set` with the result. Because `set`
/// checks `PartialEq`, downstream atom subscribers only fire when
/// the specific projected value actually changes.
///
/// ```ignore
/// let panel = select(&mut store, |s| s.active_panel);
/// panel.borrow_mut().subscribe(|p| { /* fires only on panel change */ });
/// ```
pub fn select<S, T, F>(store: &mut Store<S>, selector: F) -> SharedAtom<T>
where
    S: 'static,
    T: PartialEq + Clone + 'static,
    F: Fn(&S) -> T + 'static,
{
    let initial = selector(store.state());
    let atom = Rc::new(RefCell::new(Atom::new(initial)));
    let atom_ref = atom.clone();
    store.subscribe(move |state| {
        let next = selector(state);
        atom_ref.borrow_mut().set(next);
    });
    atom
}

/// Like [`select`] but for selectors that return a reference.
/// Clones the value on every store dispatch, but only fires atom
/// subscribers when the clone differs from the previous value.
pub fn select_ref<S, T, F>(store: &mut Store<S>, selector: F) -> SharedAtom<T>
where
    S: 'static,
    T: PartialEq + Clone + 'static,
    F: Fn(&S) -> &T + 'static,
{
    let initial = selector(store.state()).clone();
    let atom = Rc::new(RefCell::new(Atom::new(initial)));
    let atom_ref = atom.clone();
    store.subscribe(move |state| {
        let next = selector(state).clone();
        atom_ref.borrow_mut().set(next);
    });
    atom
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::store::Action;
    use std::cell::Cell;

    #[test]
    fn atom_starts_with_initial_value() {
        let atom = Atom::new(42);
        assert_eq!(*atom.get(), 42);
        assert_eq!(atom.subscriber_count(), 0);
    }

    #[test]
    fn set_notifies_on_change() {
        let mut atom = Atom::new(0);
        let count = Rc::new(Cell::new(0usize));
        let cc = count.clone();
        atom.subscribe(move |_| cc.set(cc.get() + 1));

        atom.set(1);
        assert_eq!(count.get(), 1);
        assert_eq!(*atom.get(), 1);
    }

    #[test]
    fn set_suppresses_when_equal() {
        let mut atom = Atom::new(5);
        let count = Rc::new(Cell::new(0usize));
        let cc = count.clone();
        atom.subscribe(move |_| cc.set(cc.get() + 1));

        atom.set(5);
        assert_eq!(count.get(), 0);
    }

    #[test]
    fn update_always_notifies() {
        let mut atom = Atom::new(10);
        let count = Rc::new(Cell::new(0usize));
        let cc = count.clone();
        atom.subscribe(move |_| cc.set(cc.get() + 1));

        atom.update(|v| *v += 0);
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn force_notify_fires_subscribers() {
        let mut atom = Atom::new(7);
        let count = Rc::new(Cell::new(0usize));
        let cc = count.clone();
        atom.subscribe(move |_| cc.set(cc.get() + 1));

        atom.force_notify();
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn unsubscribe_stops_notifications() {
        let mut atom = Atom::new(0);
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
        let mut atom = Atom::new(0);
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
    fn default_uses_t_default() {
        let atom: Atom<i32> = Atom::default();
        assert_eq!(*atom.get(), 0);
    }

    #[test]
    fn subscriber_sees_new_value() {
        let mut atom = Atom::new(0);
        let seen = Rc::new(Cell::new(0));
        let sc = seen.clone();
        atom.subscribe(move |v| sc.set(*v));

        atom.set(42);
        assert_eq!(seen.get(), 42);
    }

    #[test]
    fn unsubscribe_unknown_is_noop() {
        let mut atom: Atom<i32> = Atom::new(0);
        atom.unsubscribe(AtomSubscription(999));
        assert_eq!(atom.subscriber_count(), 0);
    }

    #[test]
    fn unsubscribe_one_leaves_others_intact() {
        let mut atom = Atom::new(0);
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
        let mut atom: Atom<i32> = Atom::new(0);
        let a = atom.subscribe(|_| {});
        let b = atom.subscribe(|_| {});
        atom.unsubscribe(a);
        let c = atom.subscribe(|_| {});
        assert_ne!(a.raw(), b.raw());
        assert_ne!(b.raw(), c.raw());
        assert_ne!(a.raw(), c.raw());
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
        assert_eq!(*count_atom.borrow().get(), 5);
    }

    #[test]
    fn select_updates_atom_on_store_dispatch() {
        let mut store = Store::new(TestState::default());
        let count_atom = select(&mut store, |s| s.count);

        store.dispatch(Increment);
        assert_eq!(*count_atom.borrow().get(), 1);

        store.dispatch(Increment);
        assert_eq!(*count_atom.borrow().get(), 2);
    }

    #[test]
    fn select_fires_atom_subscribers_only_on_projected_change() {
        let mut store = Store::new(TestState::default());
        let count_atom = select(&mut store, |s| s.count);

        let fires = Rc::new(Cell::new(0usize));
        let fc = fires.clone();
        count_atom
            .borrow_mut()
            .subscribe(move |_| fc.set(fc.get() + 1));

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
        assert_eq!(*label_atom.borrow().get(), "initial");

        store.dispatch(SetLabel("updated".into()));
        assert_eq!(*label_atom.borrow().get(), "updated");
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
        count_atom
            .borrow_mut()
            .subscribe(move |_| cf.set(cf.get() + 1));
        label_atom
            .borrow_mut()
            .subscribe(move |_| lf.set(lf.get() + 1));

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
        count_atom.borrow_mut().subscribe(move |v| dc.set(*v * 2));

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
        assert_eq!(*count_atom.borrow().get(), 99);
    }
}
