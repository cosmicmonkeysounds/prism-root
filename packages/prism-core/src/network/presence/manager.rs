//! `network::presence::manager` — reactive presence tracker.
//!
//! Port of `presence/presence-manager.ts`. The manager owns a local
//! peer snapshot plus an insertion-ordered map of remote peers, and
//! drives a TTL sweep through a pluggable [`TimerProvider`] so tests
//! can substitute a deterministic fake clock.
//!
//! Internally the state lives behind a shared `Rc<RefCell<Inner>>`
//! so the interval callback registered with the timer can drive the
//! same instance the caller still holds a handle to. Every public
//! method bounces through that borrow.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use indexmap::IndexMap;
use serde_json::Value as JsonValue;

use super::types::{
    CursorPosition, PeerIdentity, PresenceChange, PresenceChangeKind, PresenceListener,
    PresenceManagerOptions, PresenceState, SelectionRange, TimerHandle, TimerProvider,
};

// ── Defaults ────────────────────────────────────────────────────────────────

pub const DEFAULT_TTL_MS: u64 = 30_000;
pub const DEFAULT_SWEEP_INTERVAL_MS: u64 = 5_000;

// ── SystemTimer ─────────────────────────────────────────────────────────────

/// Wall-clock [`TimerProvider`]. `now` is the real system epoch in
/// milliseconds; `set_interval` is a no-op — Rust hosts drive
/// `PresenceManager::sweep` from their own event loop, mirroring how
/// `kernel::automation` and `interaction::notification::queue` are
/// wired.
pub struct SystemTimer {
    next_id: Cell<u64>,
}

impl SystemTimer {
    pub fn new() -> Self {
        Self {
            next_id: Cell::new(1),
        }
    }
}

impl Default for SystemTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl TimerProvider for SystemTimer {
    fn now(&self) -> u64 {
        let now = Utc::now();
        let secs = now.timestamp() as u64;
        let millis = u64::from(now.timestamp_subsec_millis());
        secs * 1000 + millis
    }

    fn set_interval(&self, _callback: Box<dyn FnMut()>, _interval_ms: u64) -> TimerHandle {
        // Host-driven sweeps — the manager keeps its own handle book
        // so `dispose` can still call `clear_interval` safely.
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        TimerHandle(id)
    }

    fn clear_interval(&self, _handle: TimerHandle) {
        // No-op for the real wall-clock timer.
    }
}

// ── Listener bus ────────────────────────────────────────────────────────────

struct Listeners {
    next_id: u64,
    entries: Vec<(u64, PresenceListener)>,
}

impl Listeners {
    fn add(&mut self, listener: PresenceListener) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push((id, listener));
        id
    }

    fn remove(&mut self, id: u64) {
        self.entries.retain(|(i, _)| *i != id);
    }

    fn notify(&mut self, change: &PresenceChange) {
        for (_, listener) in &mut self.entries {
            listener(change);
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Opaque subscription handle. Drop or call [`Subscription::unsubscribe`]
/// to stop receiving events.
pub struct Subscription {
    inner: Rc<RefCell<Listeners>>,
    id: u64,
    active: bool,
}

impl Subscription {
    pub fn unsubscribe(mut self) {
        self.active = false;
        self.inner.borrow_mut().remove(self.id);
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if self.active {
            self.inner.borrow_mut().remove(self.id);
        }
    }
}

// ── Inner state ─────────────────────────────────────────────────────────────

struct Inner {
    local_identity: PeerIdentity,
    ttl_ms: u64,
    local_state: PresenceState,
    remote_peers: IndexMap<String, PresenceState>,
    listeners: Rc<RefCell<Listeners>>,
}

impl Inner {
    fn notify(&self, change: &PresenceChange) {
        self.listeners.borrow_mut().notify(change);
    }

    fn touch_local(&mut self, now_ms: u64) {
        self.local_state.last_seen = iso_from_millis(now_ms);
    }

    fn emit_local_updated(&self) {
        self.notify(&PresenceChange {
            kind: PresenceChangeKind::Updated,
            peer_id: self.local_identity.peer_id.clone(),
            state: Some(self.local_state.clone()),
        });
    }

    fn receive_remote(&mut self, mut state: PresenceState, now_ms: u64) {
        let peer_id = state.identity.peer_id.clone();
        if peer_id == self.local_identity.peer_id {
            return; // ignore self
        }
        let existed = self.remote_peers.contains_key(&peer_id);
        state.last_seen = iso_from_millis(now_ms);
        self.remote_peers.insert(peer_id.clone(), state);
        let stored = self.remote_peers.get(&peer_id).cloned();
        self.notify(&PresenceChange {
            kind: if existed {
                PresenceChangeKind::Updated
            } else {
                PresenceChangeKind::Joined
            },
            peer_id,
            state: stored,
        });
    }

    fn remove_peer(&mut self, peer_id: &str) {
        if self.remote_peers.shift_remove(peer_id).is_none() {
            return;
        }
        self.notify(&PresenceChange {
            kind: PresenceChangeKind::Left,
            peer_id: peer_id.to_string(),
            state: None,
        });
    }

    fn sweep(&mut self, now_ms: u64) -> Vec<String> {
        let mut evicted: Vec<String> = Vec::new();
        for (peer_id, state) in &self.remote_peers {
            if let Some(last_seen_ms) = parse_iso_millis(&state.last_seen) {
                if now_ms.saturating_sub(last_seen_ms) > self.ttl_ms {
                    evicted.push(peer_id.clone());
                }
            }
        }
        for peer_id in &evicted {
            self.remove_peer(peer_id);
        }
        evicted
    }
}

// ── PresenceManager ─────────────────────────────────────────────────────────

/// Reactive presence tracker. See the module docs for semantics.
pub struct PresenceManager {
    inner: Rc<RefCell<Inner>>,
    timers: Rc<dyn TimerProvider>,
    sweep_handle: Option<TimerHandle>,
    disposed: bool,
}

impl PresenceManager {
    /// Local peer's current snapshot.
    pub fn local(&self) -> PresenceState {
        self.inner.borrow().local_state.clone()
    }

    /// Get a specific peer (local or remote) by id.
    pub fn get(&self, peer_id: &str) -> Option<PresenceState> {
        let inner = self.inner.borrow();
        if peer_id == inner.local_identity.peer_id {
            return Some(inner.local_state.clone());
        }
        inner.remote_peers.get(peer_id).cloned()
    }

    /// Whether a peer is currently tracked (includes local).
    pub fn has(&self, peer_id: &str) -> bool {
        let inner = self.inner.borrow();
        peer_id == inner.local_identity.peer_id || inner.remote_peers.contains_key(peer_id)
    }

    /// All remote peers, in insertion order.
    pub fn get_peers(&self) -> Vec<PresenceState> {
        self.inner.borrow().remote_peers.values().cloned().collect()
    }

    /// Local + all remote peers, with local first.
    pub fn get_all(&self) -> Vec<PresenceState> {
        let inner = self.inner.borrow();
        let mut out = Vec::with_capacity(inner.remote_peers.len() + 1);
        out.push(inner.local_state.clone());
        out.extend(inner.remote_peers.values().cloned());
        out
    }

    /// Number of remote peers (excludes local).
    pub fn peer_count(&self) -> usize {
        self.inner.borrow().remote_peers.len()
    }

    pub fn set_cursor(&self, cursor: Option<CursorPosition>) {
        let now = self.timers.now();
        let mut inner = self.inner.borrow_mut();
        inner.local_state.cursor = cursor;
        inner.touch_local(now);
        inner.emit_local_updated();
    }

    pub fn set_selections(&self, selections: Vec<SelectionRange>) {
        let now = self.timers.now();
        let mut inner = self.inner.borrow_mut();
        inner.local_state.selections = selections;
        inner.touch_local(now);
        inner.emit_local_updated();
    }

    pub fn set_active_view(&self, view: Option<String>) {
        let now = self.timers.now();
        let mut inner = self.inner.borrow_mut();
        inner.local_state.active_view = view;
        inner.touch_local(now);
        inner.emit_local_updated();
    }

    pub fn set_data(&self, data: std::collections::BTreeMap<String, JsonValue>) {
        let now = self.timers.now();
        let mut inner = self.inner.borrow_mut();
        inner.local_state.data = data;
        inner.touch_local(now);
        inner.emit_local_updated();
    }

    /// Bulk local update. `None` fields leave the corresponding value
    /// untouched, matching the TS `updateLocal(partial)` ergonomics.
    pub fn update_local(&self, patch: LocalPatch) {
        let now = self.timers.now();
        let mut inner = self.inner.borrow_mut();
        if let Some(cursor) = patch.cursor {
            inner.local_state.cursor = cursor;
        }
        if let Some(selections) = patch.selections {
            inner.local_state.selections = selections;
        }
        if let Some(active_view) = patch.active_view {
            inner.local_state.active_view = active_view;
        }
        if let Some(data) = patch.data {
            inner.local_state.data = data;
        }
        inner.touch_local(now);
        inner.emit_local_updated();
    }

    /// Receive a remote peer's awareness snapshot. `last_seen` is
    /// always overwritten with the current timer reading, mirroring
    /// the TS implementation.
    pub fn receive_remote(&self, state: PresenceState) {
        let now = self.timers.now();
        self.inner.borrow_mut().receive_remote(state, now);
    }

    /// Explicitly drop a remote peer (no-op for unknown peers).
    pub fn remove_peer(&self, peer_id: &str) {
        self.inner.borrow_mut().remove_peer(peer_id);
    }

    /// Subscribe to presence change events. The returned
    /// [`Subscription`] stops listening on `Drop` or explicit
    /// `unsubscribe`.
    pub fn subscribe(&self, listener: PresenceListener) -> Subscription {
        let listeners = Rc::clone(&self.inner.borrow().listeners);
        let id = listeners.borrow_mut().add(listener);
        Subscription {
            inner: listeners,
            id,
            active: true,
        }
    }

    /// Force a TTL sweep. Returns the list of peer ids evicted.
    pub fn sweep(&self) -> Vec<String> {
        let now = self.timers.now();
        self.inner.borrow_mut().sweep(now)
    }

    /// Stop the sweep timer, remove every remote peer (firing `Left`
    /// events for each), then drop all listeners.
    pub fn dispose(&mut self) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        if let Some(handle) = self.sweep_handle.take() {
            self.timers.clear_interval(handle);
        }
        let peer_ids: Vec<String> = self.inner.borrow().remote_peers.keys().cloned().collect();
        for peer_id in peer_ids {
            self.inner.borrow_mut().remove_peer(&peer_id);
        }
        self.inner.borrow().listeners.borrow_mut().clear();
    }
}

impl Drop for PresenceManager {
    fn drop(&mut self) {
        if !self.disposed {
            if let Some(handle) = self.sweep_handle.take() {
                self.timers.clear_interval(handle);
            }
        }
    }
}

/// Partial local state update. Any `Some(…)` replaces the matching
/// field; `None` leaves it untouched. `set_cursor(None)` still clears
/// the cursor — `Some(None)` is the "clear" sentinel for that slot.
#[derive(Debug, Default, Clone)]
pub struct LocalPatch {
    pub cursor: Option<Option<CursorPosition>>,
    pub selections: Option<Vec<SelectionRange>>,
    pub active_view: Option<Option<String>>,
    pub data: Option<std::collections::BTreeMap<String, JsonValue>>,
}

// ── Factory ─────────────────────────────────────────────────────────────────

/// Build a new `PresenceManager`. Takes ownership of the
/// [`TimerProvider`] so the sweep interval callback can share it with
/// the returned handle.
pub fn create_presence_manager(options: PresenceManagerOptions) -> PresenceManager {
    let ttl_ms = if options.ttl_ms == 0 {
        DEFAULT_TTL_MS
    } else {
        options.ttl_ms
    };
    let sweep_interval_ms = options.sweep_interval_ms;
    let timers: Rc<dyn TimerProvider> = Rc::from(options.timers);

    let now_ms = timers.now();
    let listeners = Rc::new(RefCell::new(Listeners {
        next_id: 0,
        entries: Vec::new(),
    }));

    let local_state = PresenceState {
        identity: options.local_identity.clone(),
        cursor: None,
        selections: Vec::new(),
        active_view: None,
        last_seen: iso_from_millis(now_ms),
        data: std::collections::BTreeMap::new(),
    };

    let inner = Rc::new(RefCell::new(Inner {
        local_identity: options.local_identity,
        ttl_ms,
        local_state,
        remote_peers: IndexMap::new(),
        listeners,
    }));

    let sweep_handle = if sweep_interval_ms > 0 {
        let inner_for_timer = Rc::clone(&inner);
        let timers_for_cb = Rc::clone(&timers);
        let callback: Box<dyn FnMut()> = Box::new(move || {
            let now = timers_for_cb.now();
            inner_for_timer.borrow_mut().sweep(now);
        });
        Some(timers.set_interval(callback, sweep_interval_ms))
    } else {
        None
    };

    PresenceManager {
        inner,
        timers,
        sweep_handle,
        disposed: false,
    }
}

// ── ISO helpers ─────────────────────────────────────────────────────────────

fn iso_from_millis(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let nsecs = ((ms % 1000) as u32) * 1_000_000;
    let dt: DateTime<Utc> = Utc
        .timestamp_opt(secs, nsecs)
        .single()
        .unwrap_or_else(Utc::now);
    dt.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_iso_millis(iso: &str) -> Option<u64> {
    let dt = DateTime::parse_from_rfc3339(iso).ok()?;
    let utc = dt.with_timezone(&Utc);
    let secs = utc.timestamp();
    if secs < 0 {
        return Some(0);
    }
    Some((secs as u64) * 1000 + u64::from(utc.timestamp_subsec_millis()))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    // ── MockTimer ────────────────────────────────────────────────────────

    // (handle, interval_ms, next_at_ms, callback)
    type MockCallback = (TimerHandle, u64, u64, Box<dyn FnMut()>);

    struct MockTimerInner {
        current: u64,
        next_id: u64,
        callbacks: Vec<MockCallback>,
    }

    impl MockTimerInner {
        fn new(start: u64) -> Self {
            Self {
                current: start,
                next_id: 1,
                callbacks: Vec::new(),
            }
        }
    }

    struct MockTimer {
        inner: Rc<RefCell<MockTimerInner>>,
    }

    impl MockTimer {
        fn new(start: u64) -> Self {
            Self {
                inner: Rc::new(RefCell::new(MockTimerInner::new(start))),
            }
        }

        fn handle(&self) -> MockTimerHandle {
            MockTimerHandle {
                inner: Rc::clone(&self.inner),
            }
        }
    }

    /// Second handle that the test uses to drive time forward + inspect
    /// registered callbacks without giving up ownership of the provider
    /// passed into the manager.
    struct MockTimerHandle {
        inner: Rc<RefCell<MockTimerInner>>,
    }

    impl MockTimerHandle {
        fn advance(&self, ms: u64) {
            let target = self.inner.borrow().current + ms;
            loop {
                let mut next_inner = self.inner.borrow_mut();
                // Find earliest next_at we need to service.
                let earliest = next_inner
                    .callbacks
                    .iter()
                    .map(|(_, _, next_at, _)| *next_at)
                    .min()
                    .unwrap_or(target);
                let advance_to = earliest.min(target);
                if advance_to <= next_inner.current && advance_to != target {
                    // Prevent infinite loop if a zero-interval slipped in.
                    next_inner.current = target;
                    break;
                }
                next_inner.current = advance_to;
                // Collect callbacks due, fire after releasing the borrow.
                let mut due_indices: Vec<usize> = Vec::new();
                for (idx, (_, _, next_at, _)) in next_inner.callbacks.iter().enumerate() {
                    if *next_at <= next_inner.current {
                        due_indices.push(idx);
                    }
                }
                // Swap callbacks out one at a time, fire, then restore.
                for idx in due_indices {
                    // Replace with an empty closure to satisfy the borrow
                    // checker; we'll put the original back.
                    let placeholder: Box<dyn FnMut()> = Box::new(|| {});
                    let (handle, interval, _, mut cb) = std::mem::replace(
                        &mut next_inner.callbacks[idx],
                        (TimerHandle(0), 0, 0, placeholder),
                    );
                    // Release borrow while firing so the callback can
                    // reenter the manager freely.
                    drop(next_inner);
                    cb();
                    next_inner = self.inner.borrow_mut();
                    let new_next_at = next_inner.current + interval;
                    // The callback may have cleared itself; only put it
                    // back if the handle slot is still the placeholder.
                    if next_inner.callbacks[idx].0 == TimerHandle(0) {
                        next_inner.callbacks[idx] = (handle, interval, new_next_at, cb);
                    }
                }
                // Clean up any entries removed via clear_interval (handle
                // set to 0 and no valid interval) that weren't refilled.
                next_inner
                    .callbacks
                    .retain(|(h, _, _, _)| *h != TimerHandle(0));
                if next_inner.current >= target {
                    next_inner.current = target;
                    break;
                }
            }
        }

        fn callback_count(&self) -> usize {
            self.inner.borrow().callbacks.len()
        }
    }

    impl TimerProvider for MockTimer {
        fn now(&self) -> u64 {
            self.inner.borrow().current
        }

        fn set_interval(&self, callback: Box<dyn FnMut()>, interval_ms: u64) -> TimerHandle {
            let mut inner = self.inner.borrow_mut();
            let id = inner.next_id;
            inner.next_id += 1;
            let handle = TimerHandle(id);
            let next_at = inner.current + interval_ms;
            inner
                .callbacks
                .push((handle, interval_ms, next_at, callback));
            handle
        }

        fn clear_interval(&self, handle: TimerHandle) {
            let mut inner = self.inner.borrow_mut();
            inner.callbacks.retain(|(h, _, _, _)| *h != handle);
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    fn identity(peer_id: &str) -> PeerIdentity {
        PeerIdentity {
            peer_id: peer_id.into(),
            display_name: format!("User {}", peer_id),
            color: "#ff0000".into(),
            avatar_url: None,
        }
    }

    fn remote_state(peer_id: &str) -> PresenceState {
        PresenceState {
            identity: identity(peer_id),
            cursor: None,
            selections: Vec::new(),
            active_view: None,
            last_seen: iso_from_millis(0),
            data: BTreeMap::new(),
        }
    }

    fn make_manager() -> (PresenceManager, MockTimerHandle) {
        let timer = MockTimer::new(1000);
        let handle = timer.handle();
        let pm = create_presence_manager(PresenceManagerOptions {
            local_identity: identity("local-1"),
            ttl_ms: 30_000,
            sweep_interval_ms: 5_000,
            timers: Box::new(timer),
        });
        (pm, handle)
    }

    fn events_for(pm: &PresenceManager) -> (Rc<RefCell<Vec<PresenceChange>>>, Subscription) {
        let changes: Rc<RefCell<Vec<PresenceChange>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&changes);
        let sub = pm.subscribe(Box::new(move |c| {
            sink.borrow_mut().push(c.clone());
        }));
        (changes, sub)
    }

    // ── Local state ──────────────────────────────────────────────────────

    #[test]
    fn local_state_initialises_with_identity_and_null_cursor() {
        let (pm, _t) = make_manager();
        let local = pm.local();
        assert_eq!(local.identity.peer_id, "local-1");
        assert!(local.cursor.is_none());
        assert!(local.selections.is_empty());
        assert!(local.active_view.is_none());
    }

    #[test]
    fn set_cursor_updates_local_cursor() {
        let (pm, _t) = make_manager();
        pm.set_cursor(Some(CursorPosition {
            object_id: "obj-1".into(),
            field: Some("name".into()),
            offset: Some(5),
        }));
        let cursor = pm.local().cursor.unwrap();
        assert_eq!(cursor.object_id, "obj-1");
        assert_eq!(cursor.field.as_deref(), Some("name"));
        assert_eq!(cursor.offset, Some(5));
    }

    #[test]
    fn set_cursor_none_clears_cursor() {
        let (pm, _t) = make_manager();
        pm.set_cursor(Some(CursorPosition {
            object_id: "obj-1".into(),
            field: None,
            offset: None,
        }));
        pm.set_cursor(None);
        assert!(pm.local().cursor.is_none());
    }

    #[test]
    fn set_selections_updates_local_selections() {
        let (pm, _t) = make_manager();
        let sels = vec![
            SelectionRange {
                object_id: "obj-1".into(),
                field: Some("name".into()),
                anchor: Some(0),
                head: Some(5),
            },
            SelectionRange {
                object_id: "obj-2".into(),
                field: None,
                anchor: None,
                head: None,
            },
        ];
        pm.set_selections(sels.clone());
        assert_eq!(pm.local().selections, sels);
    }

    #[test]
    fn set_active_view_updates_active_view() {
        let (pm, _t) = make_manager();
        pm.set_active_view(Some("collection-abc".into()));
        assert_eq!(pm.local().active_view.as_deref(), Some("collection-abc"));
    }

    #[test]
    fn set_data_updates_arbitrary_data() {
        let (pm, _t) = make_manager();
        let mut data = BTreeMap::new();
        data.insert("status".into(), JsonValue::from("typing"));
        data.insert("draft".into(), JsonValue::from(true));
        pm.set_data(data.clone());
        assert_eq!(pm.local().data, data);
    }

    #[test]
    fn update_local_does_a_bulk_update() {
        let (pm, _t) = make_manager();
        let mut data = BTreeMap::new();
        data.insert("typing".into(), JsonValue::from(true));
        pm.update_local(LocalPatch {
            cursor: Some(Some(CursorPosition {
                object_id: "obj-1".into(),
                field: None,
                offset: None,
            })),
            active_view: Some(Some("view-1".into())),
            data: Some(data.clone()),
            selections: None,
        });
        let local = pm.local();
        assert_eq!(local.cursor.as_ref().unwrap().object_id, "obj-1");
        assert_eq!(local.active_view.as_deref(), Some("view-1"));
        assert_eq!(local.data, data);
        assert!(local.selections.is_empty()); // unchanged
    }

    #[test]
    fn update_local_only_updates_provided_fields() {
        let (pm, _t) = make_manager();
        pm.set_cursor(Some(CursorPosition {
            object_id: "obj-1".into(),
            field: None,
            offset: None,
        }));
        pm.set_active_view(Some("view-1".into()));
        pm.update_local(LocalPatch {
            active_view: Some(Some("view-2".into())),
            ..Default::default()
        });
        let local = pm.local();
        assert_eq!(local.cursor.as_ref().unwrap().object_id, "obj-1");
        assert_eq!(local.active_view.as_deref(), Some("view-2"));
    }

    #[test]
    fn local_state_accessible_via_get_local_peer_id() {
        let (pm, _t) = make_manager();
        assert!(pm.get("local-1").is_some());
    }

    #[test]
    fn has_returns_true_for_local_peer() {
        let (pm, _t) = make_manager();
        assert!(pm.has("local-1"));
    }

    // ── Remote peers ─────────────────────────────────────────────────────

    #[test]
    fn starts_with_no_remote_peers() {
        let (pm, _t) = make_manager();
        assert_eq!(pm.peer_count(), 0);
        assert!(pm.get_peers().is_empty());
    }

    #[test]
    fn receive_remote_adds_a_new_peer() {
        let (pm, _t) = make_manager();
        pm.receive_remote(remote_state("remote-1"));
        assert_eq!(pm.peer_count(), 1);
        assert!(pm.has("remote-1"));
        assert_eq!(
            pm.get("remote-1").unwrap().identity.peer_id,
            "remote-1".to_string()
        );
    }

    #[test]
    fn receive_remote_updates_existing_peer() {
        let (pm, _t) = make_manager();
        let mut first = remote_state("remote-1");
        first.cursor = Some(CursorPosition {
            object_id: "a".into(),
            field: None,
            offset: None,
        });
        pm.receive_remote(first);
        let mut second = remote_state("remote-1");
        second.cursor = Some(CursorPosition {
            object_id: "b".into(),
            field: None,
            offset: None,
        });
        pm.receive_remote(second);
        assert_eq!(pm.peer_count(), 1);
        let state = pm.get("remote-1").unwrap();
        assert_eq!(state.cursor.unwrap().object_id, "b");
    }

    #[test]
    fn receive_remote_ignores_self() {
        let (pm, _t) = make_manager();
        pm.receive_remote(remote_state("local-1"));
        assert_eq!(pm.peer_count(), 0);
    }

    #[test]
    fn remove_peer_removes_a_remote_peer() {
        let (pm, _t) = make_manager();
        pm.receive_remote(remote_state("remote-1"));
        pm.remove_peer("remote-1");
        assert_eq!(pm.peer_count(), 0);
        assert!(!pm.has("remote-1"));
    }

    #[test]
    fn remove_peer_is_a_noop_for_unknown_peers() {
        let (pm, _t) = make_manager();
        pm.remove_peer("unknown");
        assert_eq!(pm.peer_count(), 0);
    }

    #[test]
    fn get_peers_returns_only_remote_peers() {
        let (pm, _t) = make_manager();
        pm.receive_remote(remote_state("remote-1"));
        pm.receive_remote(remote_state("remote-2"));
        let peers = pm.get_peers();
        assert_eq!(peers.len(), 2);
        let mut ids: Vec<String> = peers.into_iter().map(|p| p.identity.peer_id).collect();
        ids.sort();
        assert_eq!(ids, vec!["remote-1".to_string(), "remote-2".to_string()]);
    }

    #[test]
    fn get_all_returns_local_plus_remote() {
        let (pm, _t) = make_manager();
        pm.receive_remote(remote_state("remote-1"));
        let all = pm.get_all();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].identity.peer_id, "local-1");
        assert_eq!(all[1].identity.peer_id, "remote-1");
    }

    #[test]
    fn receive_remote_stamps_last_seen_from_timer() {
        let (pm, timer) = make_manager();
        timer.advance(5000); // now = 6000
        let mut state = remote_state("remote-1");
        state.last_seen = iso_from_millis(0); // old timestamp
        pm.receive_remote(state);
        let peer = pm.get("remote-1").unwrap();
        // 6000ms = 1970-01-01T00:00:06.000Z, parse back to ms.
        assert_eq!(parse_iso_millis(&peer.last_seen), Some(6000));
    }

    // ── Subscriptions ────────────────────────────────────────────────────

    #[test]
    fn fires_joined_when_new_remote_peer_appears() {
        let (pm, _t) = make_manager();
        let (changes, _sub) = events_for(&pm);
        pm.receive_remote(remote_state("remote-1"));
        let evs = changes.borrow();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, PresenceChangeKind::Joined);
        assert_eq!(evs[0].peer_id, "remote-1");
        assert!(evs[0].state.is_some());
    }

    #[test]
    fn fires_updated_when_existing_remote_peer_is_refreshed() {
        let (pm, _t) = make_manager();
        pm.receive_remote(remote_state("remote-1"));
        let (changes, _sub) = events_for(&pm);
        let mut second = remote_state("remote-1");
        second.cursor = Some(CursorPosition {
            object_id: "x".into(),
            field: None,
            offset: None,
        });
        pm.receive_remote(second);
        let evs = changes.borrow();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, PresenceChangeKind::Updated);
    }

    #[test]
    fn fires_left_when_peer_is_removed() {
        let (pm, _t) = make_manager();
        pm.receive_remote(remote_state("remote-1"));
        let (changes, _sub) = events_for(&pm);
        pm.remove_peer("remote-1");
        let evs = changes.borrow();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, PresenceChangeKind::Left);
        assert!(evs[0].state.is_none());
    }

    #[test]
    fn fires_updated_on_local_cursor_change() {
        let (pm, _t) = make_manager();
        let (changes, _sub) = events_for(&pm);
        pm.set_cursor(Some(CursorPosition {
            object_id: "obj-1".into(),
            field: None,
            offset: None,
        }));
        let evs = changes.borrow();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, PresenceChangeKind::Updated);
        assert_eq!(evs[0].peer_id, "local-1");
    }

    #[test]
    fn fires_on_local_set_selections_active_view_data() {
        let (pm, _t) = make_manager();
        let (changes, _sub) = events_for(&pm);
        pm.set_selections(vec![SelectionRange {
            object_id: "a".into(),
            field: None,
            anchor: None,
            head: None,
        }]);
        pm.set_active_view(Some("v".into()));
        let mut data = BTreeMap::new();
        data.insert("x".into(), JsonValue::from(1));
        pm.set_data(data);
        let evs = changes.borrow();
        assert_eq!(evs.len(), 3);
        assert!(evs
            .iter()
            .all(|c| c.kind == PresenceChangeKind::Updated && c.peer_id == "local-1"));
    }

    #[test]
    fn unsubscribe_stops_notifications() {
        let (pm, _t) = make_manager();
        let changes: Rc<RefCell<Vec<PresenceChange>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&changes);
        let sub = pm.subscribe(Box::new(move |c| {
            sink.borrow_mut().push(c.clone());
        }));
        sub.unsubscribe();
        pm.receive_remote(remote_state("remote-1"));
        assert!(changes.borrow().is_empty());
    }

    #[test]
    fn multiple_listeners_all_fire() {
        let (pm, _t) = make_manager();
        let count1: Rc<Cell<u32>> = Rc::new(Cell::new(0));
        let count2: Rc<Cell<u32>> = Rc::new(Cell::new(0));
        let c1 = Rc::clone(&count1);
        let c2 = Rc::clone(&count2);
        let _s1 = pm.subscribe(Box::new(move |_| c1.set(c1.get() + 1)));
        let _s2 = pm.subscribe(Box::new(move |_| c2.set(c2.get() + 1)));
        pm.set_cursor(Some(CursorPosition {
            object_id: "x".into(),
            field: None,
            offset: None,
        }));
        assert_eq!(count1.get(), 1);
        assert_eq!(count2.get(), 1);
    }

    // ── TTL eviction ─────────────────────────────────────────────────────

    #[test]
    fn sweep_evicts_peers_older_than_ttl_ms() {
        // Mirrors TS "sweep evicts peers older than ttlMs": disable
        // the auto sweep so only the manual `pm.sweep()` after advance
        // drives eviction.
        let timer = MockTimer::new(1000);
        let handle = timer.handle();
        let pm = create_presence_manager(PresenceManagerOptions {
            local_identity: identity("local-1"),
            ttl_ms: 30_000,
            sweep_interval_ms: 0,
            timers: Box::new(timer),
        });
        pm.receive_remote(remote_state("remote-1"));
        assert_eq!(pm.peer_count(), 1);
        handle.advance(31_000);
        let evicted = pm.sweep();
        assert_eq!(evicted, vec!["remote-1".to_string()]);
        assert_eq!(pm.peer_count(), 0);
    }

    #[test]
    fn sweep_keeps_peers_within_ttl() {
        let (pm, timer) = make_manager();
        pm.receive_remote(remote_state("remote-1"));
        timer.advance(10_000);
        let evicted = pm.sweep();
        assert!(evicted.is_empty());
        assert_eq!(pm.peer_count(), 1);
    }

    #[test]
    fn receive_remote_refreshes_last_seen_preventing_eviction() {
        let (pm, timer) = make_manager();
        pm.receive_remote(remote_state("remote-1"));
        timer.advance(20_000);
        pm.receive_remote(remote_state("remote-1"));
        timer.advance(20_000);
        let evicted = pm.sweep();
        assert!(evicted.is_empty());
    }

    #[test]
    fn automatic_sweep_fires_on_interval() {
        let (pm, timer) = make_manager();
        pm.receive_remote(remote_state("remote-1"));
        timer.advance(35_000);
        assert_eq!(pm.peer_count(), 0);
    }

    #[test]
    fn sweep_fires_left_event_for_evicted_peers() {
        // Mirrors TS "sweep fires 'left' event for evicted peers": use
        // a manual sweep after advancing past TTL with auto sweep off.
        let timer = MockTimer::new(1000);
        let handle = timer.handle();
        let pm = create_presence_manager(PresenceManagerOptions {
            local_identity: identity("local-1"),
            ttl_ms: 30_000,
            sweep_interval_ms: 0,
            timers: Box::new(timer),
        });
        pm.receive_remote(remote_state("remote-1"));
        pm.receive_remote(remote_state("remote-2"));
        let (changes, _sub) = events_for(&pm);
        handle.advance(31_000);
        pm.sweep();
        let left_count = changes
            .borrow()
            .iter()
            .filter(|c| c.kind == PresenceChangeKind::Left)
            .count();
        assert_eq!(left_count, 2);
    }

    #[test]
    fn sweep_only_evicts_stale_peers_keeps_fresh_ones() {
        // Use a fresh manager with no auto-sweep so we control timing.
        let timer = MockTimer::new(1000);
        let handle = timer.handle();
        let pm = create_presence_manager(PresenceManagerOptions {
            local_identity: identity("local-1"),
            ttl_ms: 30_000,
            sweep_interval_ms: 0,
            timers: Box::new(timer),
        });
        pm.receive_remote(remote_state("remote-1"));
        handle.advance(25_000);
        pm.receive_remote(remote_state("remote-2"));
        handle.advance(6_000);
        let evicted = pm.sweep();
        assert_eq!(evicted, vec!["remote-1".to_string()]);
        assert_eq!(pm.peer_count(), 1);
        assert!(pm.has("remote-2"));
    }

    // ── Dispose ──────────────────────────────────────────────────────────

    #[test]
    fn dispose_clears_all_remote_peers_and_fires_left_events() {
        let (mut pm, _t) = make_manager();
        pm.receive_remote(remote_state("remote-1"));
        pm.receive_remote(remote_state("remote-2"));
        let (changes, _sub) = events_for(&pm);
        pm.dispose();
        assert_eq!(pm.peer_count(), 0);
        let left_count = changes
            .borrow()
            .iter()
            .filter(|c| c.kind == PresenceChangeKind::Left)
            .count();
        assert_eq!(left_count, 2);
    }

    #[test]
    fn dispose_stops_the_sweep_timer() {
        let (mut pm, timer) = make_manager();
        pm.dispose();
        assert_eq!(timer.callback_count(), 0);
    }

    #[test]
    fn dispose_clears_listeners() {
        let (mut pm, _t) = make_manager();
        let changes: Rc<RefCell<Vec<PresenceChange>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&changes);
        let _sub = pm.subscribe(Box::new(move |c| {
            sink.borrow_mut().push(c.clone());
        }));
        pm.dispose();
        let left_count = changes.borrow().len();
        pm.receive_remote(remote_state("remote-1"));
        assert_eq!(changes.borrow().len(), left_count);
    }

    // ── Edge cases ───────────────────────────────────────────────────────

    #[test]
    fn get_all_always_has_local_first() {
        let (pm, _t) = make_manager();
        pm.receive_remote(remote_state("aaa"));
        pm.receive_remote(remote_state("zzz"));
        assert_eq!(pm.get_all()[0].identity.peer_id, "local-1");
    }

    #[test]
    fn handles_rapid_cursor_updates() {
        let (pm, _t) = make_manager();
        let (changes, _sub) = events_for(&pm);
        for i in 0..100 {
            pm.set_cursor(Some(CursorPosition {
                object_id: format!("obj-{}", i),
                field: None,
                offset: None,
            }));
        }
        assert_eq!(changes.borrow().len(), 100);
        assert_eq!(
            pm.local().cursor.as_ref().unwrap().object_id,
            "obj-99".to_string()
        );
    }

    #[test]
    fn works_with_zero_sweep_interval_no_auto_sweep() {
        let timer = MockTimer::new(1000);
        let handle = timer.handle();
        let pm = create_presence_manager(PresenceManagerOptions {
            local_identity: identity("local-1"),
            ttl_ms: 30_000,
            sweep_interval_ms: 0,
            timers: Box::new(timer),
        });
        pm.receive_remote(remote_state("remote-1"));
        handle.advance(60_000);
        assert_eq!(pm.peer_count(), 1);
        pm.sweep();
        assert_eq!(pm.peer_count(), 0);
    }

    // ── ISO helpers ──────────────────────────────────────────────────────

    #[test]
    fn iso_round_trip_millis() {
        let ms = 1_700_000_123_456u64;
        let iso = iso_from_millis(ms);
        assert_eq!(parse_iso_millis(&iso), Some(ms));
    }
}
