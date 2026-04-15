//! `machine` — flat, context-free finite state machine.
//!
//! Port of `kernel/state-machine/machine.ts` (commit `8426588`) onto
//! pure Rust. The legacy TS module described itself as:
//!
//! > Context-free: the machine holds no data. State is a plain string.
//! > Observable: subscribe to state changes with on().
//! > Serializable: toJSON() for persistence.
//! >
//! > Wildcard from: use '*' to match any current state.
//! > Guards: transitions can be blocked by a guard predicate.
//! > Actions: run during the transition (after exit, before enter).
//!
//! This port keeps that shape verbatim with three Rust adjustments:
//!
//! 1. State and event types are generic over `Eq + Hash + Clone`
//!    instead of TypeScript string literal unions. Callers typically
//!    hand in a `#[derive(Debug, Clone, PartialEq, Eq, Hash)]` enum.
//! 2. `on()` returns an opaque [`Subscription`] handle (mirroring
//!    `kernel::store::Subscription`) instead of an unsubscribe
//!    closure. Drop the subscription to leak the listener; call
//!    [`Machine::off`] to stop notifications.
//! 3. The TS `createMachine(def).start()` factory is collapsed into
//!    [`Machine::start`]. The factory pattern is convenience in JS —
//!    the host can keep its own `fn build_def() -> MachineDefinition`
//!    helper if it needs multiple instances.
//!
//! The xstate-backed `tool.machine.ts` is intentionally *not* ported
//! here. The migration plan §9 calls for a `statig` rewrite of tool
//! mode tracking alongside the rest of the Studio kernel wiring; that
//! lands in its own submodule once the design doc is written.

use std::collections::HashMap;
use std::hash::Hash;

/// Which source states a [`Transition`] applies to.
///
/// Mirrors the TS `from: TState | TState[] | "*"` union.
pub enum TransitionFrom<S> {
    /// Transition applies only when the machine is in exactly this
    /// state. Corresponds to TS `from: "a"`.
    One(S),
    /// Transition applies when the machine is in any of the listed
    /// states. Corresponds to TS `from: ["a", "b"]`.
    Many(Vec<S>),
    /// Transition applies from any state (the TS `"*"` wildcard).
    /// Terminal states still block it — matches TS behaviour.
    Any,
}

impl<S: Eq> TransitionFrom<S> {
    fn matches(&self, current: &S) -> bool {
        match self {
            TransitionFrom::One(s) => s == current,
            TransitionFrom::Many(v) => v.iter().any(|s| s == current),
            TransitionFrom::Any => true,
        }
    }
}

/// A state in the machine, with optional enter/exit lifecycle hooks
/// and a terminal flag.
///
/// Build with [`StateNode::new`] and chain `.on_enter` / `.on_exit` /
/// `.terminal` as needed.
pub struct StateNode<S> {
    pub id: S,
    pub on_enter: Option<Box<dyn FnMut()>>,
    pub on_exit: Option<Box<dyn FnMut()>>,
    pub terminal: bool,
}

impl<S> StateNode<S> {
    pub fn new(id: S) -> Self {
        Self {
            id,
            on_enter: None,
            on_exit: None,
            terminal: false,
        }
    }

    /// Mark the state terminal. Transitions out of a terminal state
    /// are refused by both [`Machine::send`] and [`Machine::can`].
    pub fn terminal(mut self) -> Self {
        self.terminal = true;
        self
    }

    pub fn on_enter<F>(mut self, f: F) -> Self
    where
        F: FnMut() + 'static,
    {
        self.on_enter = Some(Box::new(f));
        self
    }

    pub fn on_exit<F>(mut self, f: F) -> Self
    where
        F: FnMut() + 'static,
    {
        self.on_exit = Some(Box::new(f));
        self
    }
}

/// An edge in the state graph: `from -> to` triggered by `event`,
/// optionally guarded by a predicate and optionally running an
/// action between exit and enter hooks.
pub struct Transition<S, E> {
    pub from: TransitionFrom<S>,
    pub event: E,
    pub to: S,
    pub guard: Option<Box<dyn FnMut() -> bool>>,
    pub action: Option<Box<dyn FnMut()>>,
}

impl<S, E> Transition<S, E> {
    pub fn new(from: TransitionFrom<S>, event: E, to: S) -> Self {
        Self {
            from,
            event,
            to,
            guard: None,
            action: None,
        }
    }

    pub fn guard<F>(mut self, g: F) -> Self
    where
        F: FnMut() -> bool + 'static,
    {
        self.guard = Some(Box::new(g));
        self
    }

    pub fn action<F>(mut self, a: F) -> Self
    where
        F: FnMut() + 'static,
    {
        self.action = Some(Box::new(a));
        self
    }
}

/// Pure data definition of a machine: its initial state, all named
/// states, and every transition. Consumed by [`Machine::new`] /
/// [`Machine::start`] / [`Machine::restore`].
pub struct MachineDefinition<S, E> {
    pub initial: S,
    pub states: Vec<StateNode<S>>,
    pub transitions: Vec<Transition<S, E>>,
}

/// Options passed to [`Machine::new`].
///
/// - `initial` overrides the definition's `initial` state.
/// - `skip_initial_enter` suppresses the `onEnter` hook for the
///   starting state — used by [`Machine::restore`] to rebuild a
///   machine from a persisted state without re-running side effects.
pub struct MachineOptions<S> {
    pub initial: Option<S>,
    pub skip_initial_enter: bool,
}

impl<S> Default for MachineOptions<S> {
    fn default() -> Self {
        Self {
            initial: None,
            skip_initial_enter: false,
        }
    }
}

/// Handle returned by [`Machine::on`]. Opaque on purpose — hand it
/// back to [`Machine::off`] to unsubscribe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscription(u64);

impl Subscription {
    pub fn raw(&self) -> u64 {
        self.0
    }
}

type Listener<S, E> = Box<dyn FnMut(&S, &E)>;

/// A live finite state machine. Hold one per instance; transitions
/// run synchronously via [`Machine::send`], subscribers fire in
/// registration order right after the state update.
pub struct Machine<S, E>
where
    S: Eq + Hash + Clone,
    E: Eq + Hash + Clone,
{
    state: S,
    nodes: HashMap<S, StateNode<S>>,
    trans_by_event: HashMap<E, Vec<Transition<S, E>>>,
    listeners: Vec<(u64, Listener<S, E>)>,
    next_listener_id: u64,
}

impl<S, E> Machine<S, E>
where
    S: Eq + Hash + Clone,
    E: Eq + Hash + Clone,
{
    /// Construct a machine from a definition. Prefer [`start`](Self::start)
    /// or [`restore`](Self::restore) unless you need both options at
    /// once.
    pub fn new(def: MachineDefinition<S, E>, options: MachineOptions<S>) -> Self {
        let initial = options.initial.unwrap_or(def.initial);

        let mut nodes: HashMap<S, StateNode<S>> = HashMap::new();
        for s in def.states {
            nodes.insert(s.id.clone(), s);
        }

        let mut trans_by_event: HashMap<E, Vec<Transition<S, E>>> = HashMap::new();
        for t in def.transitions {
            trans_by_event.entry(t.event.clone()).or_default().push(t);
        }

        let mut m = Self {
            state: initial,
            nodes,
            trans_by_event,
            listeners: Vec::new(),
            next_listener_id: 0,
        };

        if !options.skip_initial_enter {
            if let Some(node) = m.nodes.get_mut(&m.state) {
                if let Some(cb) = node.on_enter.as_mut() {
                    cb();
                }
            }
        }

        m
    }

    /// Start the machine fresh at its definition's initial state.
    /// Fires the initial state's `on_enter` hook.
    pub fn start(def: MachineDefinition<S, E>) -> Self {
        Self::new(def, MachineOptions::default())
    }

    /// Rebuild a machine at a specific state without firing the
    /// `on_enter` hook. Used for `Machine.toJSON()` → `restore` round
    /// trips across a hot reload.
    pub fn restore(def: MachineDefinition<S, E>, state: S) -> Self {
        Self::new(
            def,
            MachineOptions {
                initial: Some(state),
                skip_initial_enter: true,
            },
        )
    }

    /// Current state.
    pub fn state(&self) -> &S {
        &self.state
    }

    /// Whether the current state equals `candidate`. Mirrors the TS
    /// single-argument `matches(state)`.
    pub fn matches(&self, candidate: &S) -> bool {
        &self.state == candidate
    }

    /// Whether the current state is in `candidates`. Mirrors the TS
    /// array-argument `matches([a, b])`.
    pub fn matches_any(&self, candidates: &[S]) -> bool {
        candidates.iter().any(|c| c == &self.state)
    }

    /// Whether `event` would trigger a transition from the current
    /// state, respecting guards and the terminal flag. Mirrors the TS
    /// `can(event)`.
    pub fn can(&mut self, event: &E) -> bool {
        if self.is_terminal() {
            return false;
        }
        let Some(idx) = self.find_transition(event) else {
            return false;
        };
        let list = self.trans_by_event.get_mut(event).expect("indexed above");
        match list[idx].guard.as_mut() {
            Some(g) => g(),
            None => true,
        }
    }

    /// Drive a transition with `event`. Returns `true` if the machine
    /// transitioned (and notifies subscribers), `false` if the
    /// transition was refused by the terminal flag, a missing edge,
    /// or a guard.
    ///
    /// Order of side effects on a successful transition:
    /// `on_exit` on the current state → `action` on the transition →
    /// `self.state = to` → `on_enter` on the new state → listeners.
    pub fn send(&mut self, event: &E) -> bool {
        if self.is_terminal() {
            return false;
        }

        let Some(idx) = self.find_transition(event) else {
            return false;
        };

        // Guard check (mutable borrow of the transition, then dropped).
        let to = {
            let list = self.trans_by_event.get_mut(event).expect("indexed above");
            let t = &mut list[idx];
            if let Some(g) = t.guard.as_mut() {
                if !g() {
                    return false;
                }
            }
            t.to.clone()
        };

        // on_exit for the current state.
        if let Some(node) = self.nodes.get_mut(&self.state) {
            if let Some(cb) = node.on_exit.as_mut() {
                cb();
            }
        }

        // Transition action.
        {
            let list = self.trans_by_event.get_mut(event).expect("indexed above");
            if let Some(a) = list[idx].action.as_mut() {
                a();
            }
        }

        // Commit the new state.
        self.state = to;

        // on_enter for the new state.
        if let Some(node) = self.nodes.get_mut(&self.state) {
            if let Some(cb) = node.on_enter.as_mut() {
                cb();
            }
        }

        // Notify listeners.
        for (_, listener) in self.listeners.iter_mut() {
            listener(&self.state, event);
        }

        true
    }

    /// Register a transition listener. Fires synchronously after every
    /// successful [`send`](Self::send), in registration order.
    pub fn on<F>(&mut self, listener: F) -> Subscription
    where
        F: FnMut(&S, &E) + 'static,
    {
        let id = self.next_listener_id;
        self.next_listener_id += 1;
        self.listeners.push((id, Box::new(listener)));
        Subscription(id)
    }

    /// Drop a listener. No-op if `sub` is unknown.
    pub fn off(&mut self, sub: Subscription) {
        self.listeners.retain(|(id, _)| *id != sub.0);
    }

    /// Number of live listeners — exposed for diagnostics and tests.
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }

    fn is_terminal(&self) -> bool {
        self.nodes.get(&self.state).is_some_and(|n| n.terminal)
    }

    fn find_transition(&self, event: &E) -> Option<usize> {
        let candidates = self.trans_by_event.get(event)?;
        candidates.iter().position(|t| t.from.matches(&self.state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum S {
        Idle,
        Running,
        Paused,
        Done,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum E {
        Start,
        Pause,
        Resume,
        Stop,
        Reset,
    }

    fn timer_def() -> MachineDefinition<S, E> {
        MachineDefinition {
            initial: S::Idle,
            states: vec![
                StateNode::new(S::Idle),
                StateNode::new(S::Running),
                StateNode::new(S::Paused),
                StateNode::new(S::Done).terminal(),
            ],
            transitions: vec![
                Transition::new(TransitionFrom::One(S::Idle), E::Start, S::Running),
                Transition::new(TransitionFrom::One(S::Running), E::Pause, S::Paused),
                Transition::new(TransitionFrom::One(S::Paused), E::Resume, S::Running),
                Transition::new(
                    TransitionFrom::Many(vec![S::Running, S::Paused]),
                    E::Stop,
                    S::Done,
                ),
                Transition::new(TransitionFrom::Any, E::Reset, S::Idle),
            ],
        }
    }

    // ── start / restore ───────────────────────────────────────────

    #[test]
    fn start_uses_definition_initial_state() {
        let m = Machine::start(timer_def());
        assert_eq!(m.state(), &S::Idle);
    }

    #[test]
    fn new_with_custom_initial_overrides_definition() {
        let m = Machine::new(
            timer_def(),
            MachineOptions {
                initial: Some(S::Paused),
                skip_initial_enter: false,
            },
        );
        assert_eq!(m.state(), &S::Paused);
    }

    #[test]
    fn start_fires_on_enter_for_initial_state() {
        let flag = Rc::new(RefCell::new(false));
        let flag_clone = flag.clone();
        let def = MachineDefinition::<S, E> {
            initial: S::Idle,
            states: vec![
                StateNode::new(S::Idle).on_enter(move || *flag_clone.borrow_mut() = true),
                StateNode::new(S::Running),
            ],
            transitions: vec![Transition::new(
                TransitionFrom::One(S::Idle),
                E::Start,
                S::Running,
            )],
        };
        let _ = Machine::start(def);
        assert!(*flag.borrow());
    }

    #[test]
    fn restore_does_not_fire_on_enter() {
        let flag = Rc::new(RefCell::new(false));
        let flag_clone = flag.clone();
        let def = MachineDefinition::<S, E> {
            initial: S::Idle,
            states: vec![
                StateNode::new(S::Idle).on_enter(move || *flag_clone.borrow_mut() = true),
                StateNode::new(S::Running),
            ],
            transitions: vec![Transition::new(
                TransitionFrom::One(S::Idle),
                E::Start,
                S::Running,
            )],
        };
        let _ = Machine::restore(def, S::Idle);
        assert!(!*flag.borrow());
    }

    // ── send ──────────────────────────────────────────────────────

    #[test]
    fn send_transitions_on_valid_event() {
        let mut m = Machine::start(timer_def());
        assert!(m.send(&E::Start));
        assert_eq!(m.state(), &S::Running);
    }

    #[test]
    fn send_returns_false_on_invalid_event() {
        let mut m = Machine::start(timer_def());
        assert!(!m.send(&E::Pause));
        assert_eq!(m.state(), &S::Idle);
    }

    #[test]
    fn send_supports_many_from() {
        let mut m = Machine::start(timer_def());
        m.send(&E::Start);
        m.send(&E::Pause);
        assert!(m.send(&E::Stop));
        assert_eq!(m.state(), &S::Done);
    }

    #[test]
    fn send_supports_any_from() {
        let mut m = Machine::start(timer_def());
        m.send(&E::Start);
        assert!(m.send(&E::Reset));
        assert_eq!(m.state(), &S::Idle);
    }

    #[test]
    fn terminal_state_blocks_further_transitions() {
        let mut m = Machine::start(timer_def());
        m.send(&E::Start);
        m.send(&E::Stop);
        assert_eq!(m.state(), &S::Done);
        assert!(!m.send(&E::Reset));
        assert_eq!(m.state(), &S::Done);
    }

    // ── can ───────────────────────────────────────────────────────

    #[test]
    fn can_returns_true_for_available_events() {
        let mut m = Machine::start(timer_def());
        assert!(m.can(&E::Start));
        assert!(!m.can(&E::Pause));
    }

    #[test]
    fn can_returns_false_from_terminal_state() {
        let mut m = Machine::start(timer_def());
        m.send(&E::Start);
        m.send(&E::Stop);
        assert!(!m.can(&E::Reset));
    }

    // ── matches ───────────────────────────────────────────────────

    #[test]
    fn matches_checks_single_state() {
        let m = Machine::start(timer_def());
        assert!(m.matches(&S::Idle));
        assert!(!m.matches(&S::Running));
    }

    #[test]
    fn matches_any_checks_state_list() {
        let m = Machine::start(timer_def());
        assert!(m.matches_any(&[S::Idle, S::Paused]));
        assert!(!m.matches_any(&[S::Running, S::Done]));
    }

    // ── guards ────────────────────────────────────────────────────

    #[test]
    fn guard_blocks_transition_when_false() {
        let allow = Rc::new(RefCell::new(false));
        let allow_for_guard = allow.clone();
        let def = MachineDefinition::<S, E> {
            initial: S::Idle,
            states: vec![StateNode::new(S::Idle), StateNode::new(S::Running)],
            transitions: vec![
                Transition::new(TransitionFrom::One(S::Idle), E::Start, S::Running)
                    .guard(move || *allow_for_guard.borrow()),
            ],
        };
        let mut m = Machine::start(def);

        assert!(!m.send(&E::Start));
        assert_eq!(m.state(), &S::Idle);

        *allow.borrow_mut() = true;
        assert!(m.send(&E::Start));
        assert_eq!(m.state(), &S::Running);
    }

    #[test]
    fn can_respects_guards() {
        let allow = Rc::new(RefCell::new(false));
        let allow_for_guard = allow.clone();
        let def = MachineDefinition::<S, E> {
            initial: S::Idle,
            states: vec![StateNode::new(S::Idle), StateNode::new(S::Running)],
            transitions: vec![
                Transition::new(TransitionFrom::One(S::Idle), E::Start, S::Running)
                    .guard(move || *allow_for_guard.borrow()),
            ],
        };
        let mut m = Machine::start(def);

        assert!(!m.can(&E::Start));
        *allow.borrow_mut() = true;
        assert!(m.can(&E::Start));
    }

    // ── lifecycle hooks ───────────────────────────────────────────

    #[test]
    fn send_fires_exit_action_enter_in_order() {
        let order: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let a = order.clone();
        let b = order.clone();
        let c = order.clone();
        let def = MachineDefinition::<S, E> {
            initial: S::Idle,
            states: vec![
                StateNode::new(S::Idle).on_exit(move || a.borrow_mut().push("exit-idle")),
                StateNode::new(S::Running).on_enter(move || b.borrow_mut().push("enter-running")),
            ],
            transitions: vec![
                Transition::new(TransitionFrom::One(S::Idle), E::Start, S::Running)
                    .action(move || c.borrow_mut().push("action")),
            ],
        };
        let mut m = Machine::start(def);
        m.send(&E::Start);
        assert_eq!(&*order.borrow(), &["exit-idle", "action", "enter-running"]);
    }

    // ── listeners ─────────────────────────────────────────────────

    #[test]
    fn listener_fires_on_successful_transition() {
        let log: Rc<RefCell<Vec<(S, E)>>> = Rc::new(RefCell::new(Vec::new()));
        let log_clone = log.clone();
        let mut m = Machine::start(timer_def());
        m.on(move |s, e| log_clone.borrow_mut().push((s.clone(), e.clone())));
        m.send(&E::Start);
        m.send(&E::Pause);
        assert_eq!(
            &*log.borrow(),
            &[(S::Running, E::Start), (S::Paused, E::Pause)]
        );
    }

    #[test]
    fn off_stops_listener_notifications() {
        let count = Rc::new(RefCell::new(0usize));
        let count_clone = count.clone();
        let mut m = Machine::start(timer_def());
        let sub = m.on(move |_, _| *count_clone.borrow_mut() += 1);
        m.send(&E::Start);
        assert_eq!(*count.borrow(), 1);
        m.off(sub);
        m.send(&E::Pause);
        assert_eq!(*count.borrow(), 1);
        assert_eq!(m.listener_count(), 0);
    }

    #[test]
    fn listener_ids_are_unique_across_off() {
        let mut m = Machine::<S, E>::start(timer_def());
        let a = m.on(|_, _| {});
        let b = m.on(|_, _| {});
        m.off(a);
        let c = m.on(|_, _| {});
        assert_ne!(a.raw(), b.raw());
        assert_ne!(b.raw(), c.raw());
        assert_ne!(a.raw(), c.raw());
    }

    // ── restore round-trip ────────────────────────────────────────

    #[test]
    fn restore_resumes_at_given_state_and_accepts_transitions() {
        let mut m = Machine::start(timer_def());
        m.send(&E::Start);
        let snapshot = m.state().clone();

        // Simulated reload: rebuild the definition and restore to the
        // snapshot without re-firing `on_enter`.
        let mut reloaded = Machine::restore(timer_def(), snapshot);
        assert_eq!(reloaded.state(), &S::Running);
        assert!(reloaded.send(&E::Pause));
        assert_eq!(reloaded.state(), &S::Paused);
    }
}
