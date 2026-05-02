//! Stopwatch engine — a state-machine timer with pause/resume,
//! hooks, listeners, and snapshot serialization.
//!
//! Port of `@core/timekeeping` engine. The stopwatch accumulates
//! active running time across pause/resume cycles and emits a
//! [`TimeEntry`] on stop. A pluggable [`Clock`] trait allows
//! deterministic testing via [`ManualClock`].

use std::cell::Cell;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};

use super::types::{TimeEntry, TimerHookSet, TimerPhase, TimerSnapshot};

// ── Clock trait ───────────────────────────────────────────────────

/// Abstraction over time sources so tests can inject deterministic
/// clocks.
pub trait Clock {
    fn now_ms(&self) -> u64;
}

/// Wall-clock time via `std::time::SystemTime`.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

/// Manually-controlled clock for deterministic tests.
pub struct ManualClock {
    current: Cell<u64>,
}

impl ManualClock {
    pub fn new(initial: u64) -> Self {
        Self {
            current: Cell::new(initial),
        }
    }

    pub fn set(&self, ms: u64) {
        self.current.set(ms);
    }

    pub fn advance(&self, ms: u64) {
        self.current.set(self.current.get() + ms);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.current.get()
    }
}

// ── ISO helpers ───────────────────────────────────────────────────

fn iso_from_millis(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let nsecs = ((ms % 1000) as u32) * 1_000_000;
    let dt: DateTime<Utc> = Utc
        .timestamp_opt(secs, nsecs)
        .single()
        .unwrap_or_else(Utc::now);
    dt.to_rfc3339_opts(SecondsFormat::Millis, true)
}

// ── Options ───────────────────────────────────────────────────────

/// Configuration for constructing a [`Stopwatch`].
#[derive(Default)]
pub struct StopwatchOptions {
    pub label: Option<String>,
    pub object_id: Option<String>,
    pub object_type: Option<String>,
}

// ── Internal hook/listener storage ────────────────────────────────

struct HookEntry {
    id: usize,
    hooks: TimerHookSet,
}

struct PhaseListener {
    id: usize,
    callback: Box<dyn Fn(TimerPhase)>,
}

// ── Stopwatch ─────────────────────────────────────────────────────

/// A state-machine stopwatch that accumulates active running time.
///
/// State transitions: `Idle → Running → Paused → Running → … → Stopped`.
/// `reset()` returns to `Idle` from any phase. Invalid transitions
/// are silently ignored (no panics).
pub struct Stopwatch {
    phase: TimerPhase,
    elapsed: u64,
    segment_start: Option<u64>,
    started_at: Option<u64>,
    clock: Box<dyn Clock>,
    label: Option<String>,
    object_id: Option<String>,
    object_type: Option<String>,
    hook_entries: Vec<HookEntry>,
    phase_listeners: Vec<PhaseListener>,
    next_id: usize,
}

impl Stopwatch {
    pub fn new(clock: Box<dyn Clock>, options: StopwatchOptions) -> Self {
        Self {
            phase: TimerPhase::Idle,
            elapsed: 0,
            segment_start: None,
            started_at: None,
            clock,
            label: options.label,
            object_id: options.object_id,
            object_type: options.object_type,
            hook_entries: Vec::new(),
            phase_listeners: Vec::new(),
            next_id: 0,
        }
    }

    pub fn with_system_clock(options: StopwatchOptions) -> Self {
        Self::new(Box::new(SystemClock), options)
    }

    // ── Properties ────────────────────────────────────────────────

    pub fn phase(&self) -> TimerPhase {
        self.phase
    }

    /// Returns `true` when the timer is Running or Paused (i.e. a
    /// timing session is in progress).
    pub fn is_active(&self) -> bool {
        matches!(self.phase, TimerPhase::Running | TimerPhase::Paused)
    }

    /// Live-computed elapsed time. If running, dynamically adds the
    /// current segment duration.
    pub fn elapsed(&self) -> u64 {
        match self.phase {
            TimerPhase::Running => {
                let seg = self
                    .segment_start
                    .map(|s| self.clock.now_ms().saturating_sub(s))
                    .unwrap_or(0);
                self.elapsed + seg
            }
            _ => self.elapsed,
        }
    }

    pub fn started_at(&self) -> Option<u64> {
        self.started_at
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn set_label(&mut self, label: Option<String>) {
        self.label = label;
    }

    pub fn object_id(&self) -> Option<&str> {
        self.object_id.as_deref()
    }

    pub fn object_type(&self) -> Option<&str> {
        self.object_type.as_deref()
    }

    // ── Guards ────────────────────────────────────────────────────

    pub fn can_start(&self) -> bool {
        self.phase == TimerPhase::Idle
    }

    pub fn can_pause(&self) -> bool {
        self.phase == TimerPhase::Running
    }

    pub fn can_resume(&self) -> bool {
        self.phase == TimerPhase::Paused
    }

    pub fn can_stop(&self) -> bool {
        matches!(self.phase, TimerPhase::Running | TimerPhase::Paused)
    }

    // ── Commands ──────────────────────────────────────────────────

    /// Idle → Running. No-op if not idle.
    pub fn start(&mut self) {
        if !self.can_start() {
            return;
        }
        let now = self.clock.now_ms();
        self.started_at = Some(now);
        self.segment_start = Some(now);
        self.set_phase(TimerPhase::Running);
        self.fire_hooks(|h| {
            if let Some(f) = &h.on_start {
                f();
            }
        });
    }

    /// Running → Paused. Accumulates the current segment. No-op if
    /// not running.
    pub fn pause(&mut self) {
        if !self.can_pause() {
            return;
        }
        let now = self.clock.now_ms();
        if let Some(seg_start) = self.segment_start {
            self.elapsed += now.saturating_sub(seg_start);
        }
        self.segment_start = None;
        self.set_phase(TimerPhase::Paused);
        self.fire_hooks(|h| {
            if let Some(f) = &h.on_pause {
                f();
            }
        });
    }

    /// Paused → Running. Starts a new segment. No-op if not paused.
    pub fn resume(&mut self) {
        if !self.can_resume() {
            return;
        }
        let now = self.clock.now_ms();
        self.segment_start = Some(now);
        self.set_phase(TimerPhase::Running);
        self.fire_hooks(|h| {
            if let Some(f) = &h.on_resume {
                f();
            }
        });
    }

    /// Running|Paused → Stopped. Emits a [`TimeEntry`]. No-op if
    /// neither running nor paused.
    pub fn stop(&mut self) -> Option<TimeEntry> {
        if !self.can_stop() {
            return None;
        }
        let now = self.clock.now_ms();
        // Flush current segment if running.
        if self.phase == TimerPhase::Running {
            if let Some(seg_start) = self.segment_start {
                self.elapsed += now.saturating_sub(seg_start);
            }
        }
        self.segment_start = None;

        let entry = TimeEntry {
            started_at: iso_from_millis(self.started_at.unwrap_or(now)),
            stopped_at: iso_from_millis(now),
            elapsed_ms: self.elapsed,
            label: self.label.clone(),
            object_id: self.object_id.clone(),
            object_type: self.object_type.clone(),
        };

        self.set_phase(TimerPhase::Stopped);
        self.fire_hooks(|h| {
            if let Some(f) = &h.on_stop {
                f(&entry);
            }
        });
        Some(entry)
    }

    /// Any → Idle. Clears all accumulated state.
    pub fn reset(&mut self) {
        self.elapsed = 0;
        self.segment_start = None;
        self.started_at = None;
        self.set_phase(TimerPhase::Idle);
        self.fire_hooks(|h| {
            if let Some(f) = &h.on_reset {
                f();
            }
        });
    }

    // ── Hooks ─────────────────────────────────────────────────────

    /// Register a set of hooks. Returns an ID for later removal.
    pub fn add_hooks(&mut self, hooks: TimerHookSet) -> usize {
        let id = self.next_id();
        self.hook_entries.push(HookEntry { id, hooks });
        id
    }

    pub fn remove_hooks(&mut self, id: usize) {
        self.hook_entries.retain(|e| e.id != id);
    }

    // ── Phase listeners ───────────────────────────────────────────

    /// Register a listener that fires on every phase change. Returns
    /// an ID for later removal.
    pub fn on_phase_change(&mut self, listener: Box<dyn Fn(TimerPhase)>) -> usize {
        let id = self.next_id();
        self.phase_listeners.push(PhaseListener {
            id,
            callback: listener,
        });
        id
    }

    pub fn remove_listener(&mut self, id: usize) {
        self.phase_listeners.retain(|l| l.id != id);
    }

    // ── Snapshot ──────────────────────────────────────────────────

    pub fn to_snapshot(&self) -> TimerSnapshot {
        TimerSnapshot {
            phase: self.phase,
            started_at: self.started_at,
            elapsed: self.elapsed,
            segment_start: self.segment_start,
            label: self.label.clone(),
            object_id: self.object_id.clone(),
            object_type: self.object_type.clone(),
        }
    }

    pub fn from_snapshot(snapshot: TimerSnapshot, clock: Box<dyn Clock>) -> Self {
        Self {
            phase: snapshot.phase,
            elapsed: snapshot.elapsed,
            segment_start: snapshot.segment_start,
            started_at: snapshot.started_at,
            clock,
            label: snapshot.label,
            object_id: snapshot.object_id,
            object_type: snapshot.object_type,
            hook_entries: Vec::new(),
            phase_listeners: Vec::new(),
            next_id: 0,
        }
    }

    // ── Internal ──────────────────────────────────────────────────

    fn next_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn set_phase(&mut self, phase: TimerPhase) {
        self.phase = phase;
        // Fire listeners after mutation.
        for listener in &self.phase_listeners {
            (listener.callback)(phase);
        }
    }

    fn fire_hooks<F: Fn(&TimerHookSet)>(&self, f: F) {
        for entry in &self.hook_entries {
            f(&entry.hooks);
        }
    }
}

// ── Widget contributions ─────────────────────────────────────────

pub fn widget_contributions() -> Vec<crate::widget::WidgetContribution> {
    use crate::widget::{
        FieldSpec, LayoutDirection, NumericBounds, SelectOption, SignalSpec, TemplateNode,
        ToolbarAction, WidgetCategory, WidgetContribution, WidgetSize, WidgetTemplate,
    };
    use serde_json::json;

    vec![
        WidgetContribution {
            id: "stopwatch".into(),
            label: "Stopwatch".into(),
            description: "Live timer display with start/pause/stop controls".into(),
            icon: Some("clock".into()),
            category: WidgetCategory::Temporal,
            config_fields: vec![FieldSpec::boolean("show_laps", "Show Laps")],
            signals: vec![
                SignalSpec::new("started", "Timer started"),
                SignalSpec::new("stopped", "Timer stopped"),
                SignalSpec::new("paused", "Timer paused"),
                SignalSpec::new("lap-recorded", "A lap was recorded").with_payload(vec![
                    FieldSpec::number("elapsed_ms", "Elapsed (ms)", NumericBounds::unbounded()),
                ]),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("start", "Start", "play"),
                ToolbarAction::signal("pause", "Pause", "pause"),
                ToolbarAction::signal("stop", "Stop", "stop"),
                ToolbarAction::signal("lap", "Lap", "flag"),
                ToolbarAction::signal("reset", "Reset", "refresh"),
            ],
            default_size: WidgetSize::new(1, 1),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::DataBinding {
                            field: "elapsed".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::Conditional {
                            field: "show_laps".into(),
                            child: Box::new(TemplateNode::Repeater {
                                source: "laps".into(),
                                item_template: Box::new(TemplateNode::Component {
                                    component_id: "text".into(),
                                    props: json!({"body": "lap"}),
                                }),
                                empty_label: Some("No laps".into()),
                            }),
                            fallback: None,
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "time-log".into(),
            label: "Time Log".into(),
            description: "Time entry history table".into(),
            icon: Some("list".into()),
            category: WidgetCategory::Temporal,
            config_fields: vec![
                FieldSpec::select(
                    "group_by",
                    "Group By",
                    vec![
                        SelectOption::new("day", "Day"),
                        SelectOption::new("week", "Week"),
                        SelectOption::new("project", "Project"),
                    ],
                ),
                FieldSpec::number("limit", "Items to Show", NumericBounds::unbounded())
                    .with_default(json!(20)),
            ],
            signals: vec![
                SignalSpec::new("entry-selected", "A time entry was selected")
                    .with_payload(vec![FieldSpec::text("entry_id", "Entry ID")]),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("new-entry", "New Entry", "plus"),
                ToolbarAction::signal("export", "Export", "download"),
            ],
            default_size: WidgetSize::new(2, 2),
            min_size: Some(WidgetSize::new(1, 1)),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(4),
                    padding: Some(8),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Time Log", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "entries".into(),
                            item_template: Box::new(TemplateNode::Component {
                                component_id: "text".into(),
                                props: json!({"body": "entry"}),
                            }),
                            empty_label: Some("No entries".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "pomodoro".into(),
            label: "Pomodoro".into(),
            description: "Focus timer with work/break cycles".into(),
            icon: Some("clock".into()),
            category: WidgetCategory::Temporal,
            config_fields: vec![
                FieldSpec::number(
                    "work_minutes",
                    "Work (min)",
                    NumericBounds::min_max(1.0, 120.0),
                )
                .with_default(json!(25)),
                FieldSpec::number(
                    "break_minutes",
                    "Break (min)",
                    NumericBounds::min_max(1.0, 60.0),
                )
                .with_default(json!(5)),
                FieldSpec::number(
                    "long_break_minutes",
                    "Long Break (min)",
                    NumericBounds::min_max(1.0, 120.0),
                )
                .with_default(json!(15)),
            ],
            signals: vec![
                SignalSpec::new("work-completed", "Work session completed"),
                SignalSpec::new("break-completed", "Break session completed"),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("start", "Start", "play"),
                ToolbarAction::signal("skip", "Skip", "forward"),
                ToolbarAction::signal("reset", "Reset", "refresh"),
            ],
            default_size: WidgetSize::new(1, 1),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(16),
                    children: vec![
                        TemplateNode::DataBinding {
                            field: "phase".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::DataBinding {
                            field: "remaining".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                    ],
                },
            },
            ..Default::default()
        },
    ]
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    fn manual_clock(initial: u64) -> Box<ManualClock> {
        Box::new(ManualClock::new(initial))
    }

    fn default_opts() -> StopwatchOptions {
        StopwatchOptions::default()
    }

    fn labeled_opts(label: &str) -> StopwatchOptions {
        StopwatchOptions {
            label: Some(label.to_string()),
            ..Default::default()
        }
    }

    // ── Lifecycle ─────────────────────────────────────────────────

    #[test]
    fn initial_state_is_idle() {
        let clock = manual_clock(1000);
        let sw = Stopwatch::new(clock, default_opts());
        assert_eq!(sw.phase(), TimerPhase::Idle);
        assert!(!sw.is_active());
        assert_eq!(sw.elapsed(), 0);
        assert_eq!(sw.started_at(), None);
    }

    #[test]
    fn start_transitions_to_running() {
        let clock = manual_clock(1000);
        // Need a raw pointer to advance the clock after handing
        // ownership to the stopwatch.
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        assert_eq!(sw.phase(), TimerPhase::Running);
        assert!(sw.is_active());
        assert_eq!(sw.started_at(), Some(1000));
        // Advance time and check live elapsed.
        unsafe { &*clock_ptr }.advance(500);
        assert_eq!(sw.elapsed(), 500);
    }

    #[test]
    fn pause_transitions_to_paused() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        unsafe { &*clock_ptr }.advance(300);
        sw.pause();
        assert_eq!(sw.phase(), TimerPhase::Paused);
        assert!(sw.is_active());
        assert_eq!(sw.elapsed(), 300);
        // Elapsed should not advance while paused.
        unsafe { &*clock_ptr }.advance(1000);
        assert_eq!(sw.elapsed(), 300);
    }

    #[test]
    fn resume_transitions_to_running() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        unsafe { &*clock_ptr }.advance(200);
        sw.pause();
        unsafe { &*clock_ptr }.advance(500); // paused time, not counted
        sw.resume();
        assert_eq!(sw.phase(), TimerPhase::Running);
        // Only the 200ms before pause is accumulated.
        assert_eq!(sw.elapsed(), 200);
        // Now advance while running.
        unsafe { &*clock_ptr }.advance(100);
        assert_eq!(sw.elapsed(), 300);
    }

    #[test]
    fn stop_from_running() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, labeled_opts("work"));
        sw.start();
        unsafe { &*clock_ptr }.advance(5000);
        let entry = sw.stop();
        assert_eq!(sw.phase(), TimerPhase::Stopped);
        assert!(!sw.is_active());
        let entry = entry.expect("should produce a TimeEntry");
        assert_eq!(entry.elapsed_ms, 5000);
        assert_eq!(entry.label.as_deref(), Some("work"));
        assert!(!entry.started_at.is_empty());
        assert!(!entry.stopped_at.is_empty());
    }

    #[test]
    fn stop_from_paused() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        unsafe { &*clock_ptr }.advance(400);
        sw.pause();
        unsafe { &*clock_ptr }.advance(9999); // paused
        let entry = sw.stop().expect("entry");
        assert_eq!(entry.elapsed_ms, 400);
    }

    #[test]
    fn reset_clears_everything() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, labeled_opts("test"));
        sw.start();
        unsafe { &*clock_ptr }.advance(1000);
        sw.reset();
        assert_eq!(sw.phase(), TimerPhase::Idle);
        assert_eq!(sw.elapsed(), 0);
        assert_eq!(sw.started_at(), None);
        assert!(!sw.is_active());
    }

    #[test]
    fn reset_from_paused() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        unsafe { &*clock_ptr }.advance(100);
        sw.pause();
        sw.reset();
        assert_eq!(sw.phase(), TimerPhase::Idle);
        assert_eq!(sw.elapsed(), 0);
    }

    #[test]
    fn reset_from_stopped() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        unsafe { &*clock_ptr }.advance(100);
        sw.stop();
        sw.reset();
        assert_eq!(sw.phase(), TimerPhase::Idle);
        assert_eq!(sw.elapsed(), 0);
    }

    // ── Elapsed accumulation across pause/resume cycles ───────────

    #[test]
    fn multiple_pause_resume_cycles() {
        let clock = manual_clock(0);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());

        sw.start(); // t=0
        unsafe { &*clock_ptr }.advance(100); // +100 active
        sw.pause(); // t=100, elapsed=100
        assert_eq!(sw.elapsed(), 100);

        unsafe { &*clock_ptr }.advance(500); // paused 500ms
        sw.resume(); // t=600
        unsafe { &*clock_ptr }.advance(200); // +200 active
        sw.pause(); // t=800, elapsed=300
        assert_eq!(sw.elapsed(), 300);

        unsafe { &*clock_ptr }.advance(1000); // paused 1s
        sw.resume(); // t=1800
        unsafe { &*clock_ptr }.advance(50); // +50 active
        assert_eq!(sw.elapsed(), 350);

        let entry = sw.stop().expect("entry");
        assert_eq!(entry.elapsed_ms, 350);
    }

    // ── Invalid transitions are no-ops ────────────────────────────

    #[test]
    fn start_while_running_is_noop() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        unsafe { &*clock_ptr }.advance(100);
        sw.start(); // no-op
        assert_eq!(sw.phase(), TimerPhase::Running);
        assert_eq!(sw.started_at(), Some(1000)); // unchanged
    }

    #[test]
    fn pause_while_idle_is_noop() {
        let clock = manual_clock(1000);
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.pause();
        assert_eq!(sw.phase(), TimerPhase::Idle);
    }

    #[test]
    fn pause_while_paused_is_noop() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        unsafe { &*clock_ptr }.advance(100);
        sw.pause();
        assert_eq!(sw.elapsed(), 100);
        sw.pause(); // no-op
        assert_eq!(sw.elapsed(), 100);
        assert_eq!(sw.phase(), TimerPhase::Paused);
    }

    #[test]
    fn resume_while_idle_is_noop() {
        let clock = manual_clock(1000);
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.resume();
        assert_eq!(sw.phase(), TimerPhase::Idle);
    }

    #[test]
    fn resume_while_running_is_noop() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        unsafe { &*clock_ptr }.advance(100);
        sw.resume(); // no-op
        assert_eq!(sw.phase(), TimerPhase::Running);
    }

    #[test]
    fn stop_while_idle_returns_none() {
        let clock = manual_clock(1000);
        let mut sw = Stopwatch::new(clock, default_opts());
        assert!(sw.stop().is_none());
        assert_eq!(sw.phase(), TimerPhase::Idle);
    }

    #[test]
    fn stop_while_stopped_returns_none() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        unsafe { &*clock_ptr }.advance(100);
        sw.stop();
        assert!(sw.stop().is_none()); // already stopped
    }

    #[test]
    fn start_while_stopped_is_noop() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        unsafe { &*clock_ptr }.advance(100);
        sw.stop();
        sw.start(); // not idle, so no-op
        assert_eq!(sw.phase(), TimerPhase::Stopped);
    }

    // ── Guard methods ─────────────────────────────────────────────

    #[test]
    fn guard_methods() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());

        // Idle
        assert!(sw.can_start());
        assert!(!sw.can_pause());
        assert!(!sw.can_resume());
        assert!(!sw.can_stop());

        // Running
        sw.start();
        assert!(!sw.can_start());
        assert!(sw.can_pause());
        assert!(!sw.can_resume());
        assert!(sw.can_stop());

        // Paused
        unsafe { &*clock_ptr }.advance(100);
        sw.pause();
        assert!(!sw.can_start());
        assert!(!sw.can_pause());
        assert!(sw.can_resume());
        assert!(sw.can_stop());

        // Stopped
        sw.stop();
        assert!(!sw.can_start());
        assert!(!sw.can_pause());
        assert!(!sw.can_resume());
        assert!(!sw.can_stop());
    }

    // ── Snapshot round-trip ───────────────────────────────────────

    #[test]
    fn snapshot_round_trip_idle() {
        let clock = manual_clock(1000);
        let sw = Stopwatch::new(clock, labeled_opts("snapshot-test"));
        let snap = sw.to_snapshot();
        assert_eq!(snap.phase, TimerPhase::Idle);
        assert_eq!(snap.elapsed, 0);
        assert_eq!(snap.started_at, None);
        assert_eq!(snap.segment_start, None);
        assert_eq!(snap.label.as_deref(), Some("snapshot-test"));

        let clock2 = manual_clock(2000);
        let restored = Stopwatch::from_snapshot(snap, clock2);
        assert_eq!(restored.phase(), TimerPhase::Idle);
        assert_eq!(restored.elapsed(), 0);
        assert_eq!(restored.label(), Some("snapshot-test"));
    }

    #[test]
    fn snapshot_round_trip_running() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(
            clock,
            StopwatchOptions {
                label: Some("rt".into()),
                object_id: Some("obj-1".into()),
                object_type: Some("task".into()),
            },
        );
        sw.start();
        unsafe { &*clock_ptr }.advance(250);
        sw.pause();
        unsafe { &*clock_ptr }.advance(100);
        sw.resume();

        let snap = sw.to_snapshot();
        assert_eq!(snap.phase, TimerPhase::Running);
        assert_eq!(snap.elapsed, 250);
        assert!(snap.segment_start.is_some());
        assert_eq!(snap.object_id.as_deref(), Some("obj-1"));
        assert_eq!(snap.object_type.as_deref(), Some("task"));

        // Restore with a new clock at the same time.
        let clock2 = manual_clock(snap.segment_start.unwrap());
        let clock2_ptr: *const ManualClock = &*clock2;
        let restored = Stopwatch::from_snapshot(snap, clock2);
        assert_eq!(restored.phase(), TimerPhase::Running);
        assert_eq!(restored.elapsed(), 250); // no extra time yet

        unsafe { &*clock2_ptr }.advance(50);
        assert_eq!(restored.elapsed(), 300);
    }

    #[test]
    fn snapshot_serialization() {
        let snap = TimerSnapshot {
            phase: TimerPhase::Paused,
            started_at: Some(1000),
            elapsed: 500,
            segment_start: None,
            label: Some("ser".into()),
            object_id: None,
            object_type: None,
        };
        let json = serde_json::to_string(&snap).expect("serialize");
        let deser: TimerSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deser.phase, TimerPhase::Paused);
        assert_eq!(deser.elapsed, 500);
        assert_eq!(deser.label.as_deref(), Some("ser"));
    }

    // ── Hook invocation ───────────────────────────────────────────

    #[test]
    fn hooks_fire_on_transitions() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());

        let log: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

        let l = log.clone();
        let hooks = TimerHookSet {
            on_start: Some(Box::new({
                let l = l.clone();
                move || l.borrow_mut().push("start".into())
            })),
            on_pause: Some(Box::new({
                let l = l.clone();
                move || l.borrow_mut().push("pause".into())
            })),
            on_resume: Some(Box::new({
                let l = l.clone();
                move || l.borrow_mut().push("resume".into())
            })),
            on_stop: Some(Box::new({
                let l = l.clone();
                move |entry: &TimeEntry| l.borrow_mut().push(format!("stop:{}", entry.elapsed_ms))
            })),
            on_reset: Some(Box::new({
                let l = l.clone();
                move || l.borrow_mut().push("reset".into())
            })),
        };

        sw.add_hooks(hooks);

        sw.start();
        unsafe { &*clock_ptr }.advance(100);
        sw.pause();
        unsafe { &*clock_ptr }.advance(50);
        sw.resume();
        unsafe { &*clock_ptr }.advance(200);
        sw.stop();
        sw.reset();

        let log = log.borrow();
        assert_eq!(*log, vec!["start", "pause", "resume", "stop:300", "reset"]);
    }

    #[test]
    fn hooks_can_be_removed() {
        let clock = manual_clock(1000);
        let mut sw = Stopwatch::new(clock, default_opts());

        let count = Rc::new(Cell::new(0u32));
        let c = count.clone();
        let id = sw.add_hooks(TimerHookSet {
            on_start: Some(Box::new(move || {
                c.set(c.get() + 1);
            })),
            ..Default::default()
        });

        sw.start();
        assert_eq!(count.get(), 1);
        sw.reset();
        sw.remove_hooks(id);
        sw.start();
        assert_eq!(count.get(), 1); // no longer fires
    }

    #[test]
    fn multiple_hook_sets() {
        let clock = manual_clock(0);
        let mut sw = Stopwatch::new(clock, default_opts());

        let a = Rc::new(Cell::new(false));
        let b = Rc::new(Cell::new(false));

        let a2 = a.clone();
        sw.add_hooks(TimerHookSet {
            on_start: Some(Box::new(move || a2.set(true))),
            ..Default::default()
        });
        let b2 = b.clone();
        sw.add_hooks(TimerHookSet {
            on_start: Some(Box::new(move || b2.set(true))),
            ..Default::default()
        });

        sw.start();
        assert!(a.get());
        assert!(b.get());
    }

    // ── Phase change listeners ────────────────────────────────────

    #[test]
    fn phase_change_listener() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());

        let phases: Rc<RefCell<Vec<TimerPhase>>> = Rc::new(RefCell::new(Vec::new()));
        let p = phases.clone();
        sw.on_phase_change(Box::new(move |phase| {
            p.borrow_mut().push(phase);
        }));

        sw.start();
        unsafe { &*clock_ptr }.advance(100);
        sw.pause();
        sw.resume();
        unsafe { &*clock_ptr }.advance(50);
        sw.stop();
        sw.reset();

        let phases = phases.borrow();
        assert_eq!(
            *phases,
            vec![
                TimerPhase::Running,
                TimerPhase::Paused,
                TimerPhase::Running,
                TimerPhase::Stopped,
                TimerPhase::Idle,
            ]
        );
    }

    #[test]
    fn phase_listener_can_be_removed() {
        let clock = manual_clock(1000);
        let mut sw = Stopwatch::new(clock, default_opts());

        let count = Rc::new(Cell::new(0u32));
        let c = count.clone();
        let id = sw.on_phase_change(Box::new(move |_| {
            c.set(c.get() + 1);
        }));

        sw.start();
        assert_eq!(count.get(), 1);
        sw.remove_listener(id);
        sw.reset();
        assert_eq!(count.get(), 1); // no longer fires
    }

    #[test]
    fn invalid_transitions_do_not_fire_listeners() {
        let clock = manual_clock(1000);
        let mut sw = Stopwatch::new(clock, default_opts());

        let count = Rc::new(Cell::new(0u32));
        let c = count.clone();
        sw.on_phase_change(Box::new(move |_| {
            c.set(c.get() + 1);
        }));

        sw.pause(); // invalid from Idle
        sw.resume(); // invalid from Idle
        sw.stop(); // invalid from Idle
        assert_eq!(count.get(), 0);
    }

    // ── TimeEntry ISO timestamps ──────────────────────────────────

    #[test]
    fn time_entry_has_valid_iso_timestamps() {
        // epoch 1_700_000_000_000 ms = 2023-11-14T22:13:20.000Z
        let clock = manual_clock(1_700_000_000_000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        unsafe { &*clock_ptr }.advance(1500);
        let entry = sw.stop().expect("entry");
        assert!(entry.started_at.contains("2023-11-14"));
        assert!(entry.stopped_at.contains("2023-11-14"));
        assert_eq!(entry.elapsed_ms, 1500);
    }

    // ── Object metadata ───────────────────────────────────────────

    #[test]
    fn object_metadata_flows_through() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(
            clock,
            StopwatchOptions {
                label: Some("task-timer".into()),
                object_id: Some("task-42".into()),
                object_type: Some("task".into()),
            },
        );
        assert_eq!(sw.label(), Some("task-timer"));
        assert_eq!(sw.object_id(), Some("task-42"));
        assert_eq!(sw.object_type(), Some("task"));

        sw.start();
        unsafe { &*clock_ptr }.advance(100);
        let entry = sw.stop().expect("entry");
        assert_eq!(entry.label.as_deref(), Some("task-timer"));
        assert_eq!(entry.object_id.as_deref(), Some("task-42"));
        assert_eq!(entry.object_type.as_deref(), Some("task"));
    }

    #[test]
    fn set_label_updates_label() {
        let clock = manual_clock(1000);
        let mut sw = Stopwatch::new(clock, default_opts());
        assert_eq!(sw.label(), None);
        sw.set_label(Some("hello".into()));
        assert_eq!(sw.label(), Some("hello"));
        sw.set_label(None);
        assert_eq!(sw.label(), None);
    }

    // ── Elapsed edge cases ────────────────────────────────────────

    #[test]
    fn elapsed_is_zero_when_idle() {
        let clock = manual_clock(5000);
        let sw = Stopwatch::new(clock, default_opts());
        assert_eq!(sw.elapsed(), 0);
    }

    #[test]
    fn elapsed_is_frozen_when_stopped() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        unsafe { &*clock_ptr }.advance(500);
        sw.stop();
        unsafe { &*clock_ptr }.advance(9999);
        assert_eq!(sw.elapsed(), 500);
    }

    // ── Start after reset ─────────────────────────────────────────

    #[test]
    fn can_start_after_reset() {
        let clock = manual_clock(1000);
        let clock_ptr: *const ManualClock = &*clock;
        let mut sw = Stopwatch::new(clock, default_opts());
        sw.start();
        unsafe { &*clock_ptr }.advance(100);
        sw.stop();
        sw.reset();
        assert!(sw.can_start());
        sw.start();
        assert_eq!(sw.phase(), TimerPhase::Running);
        assert_eq!(sw.elapsed(), 0);
    }

    // ── with_system_clock ─────────────────────────────────────────

    #[test]
    fn with_system_clock_creates_valid_stopwatch() {
        let sw = Stopwatch::with_system_clock(default_opts());
        assert_eq!(sw.phase(), TimerPhase::Idle);
        assert_eq!(sw.elapsed(), 0);
    }

    // ── Widget contributions ─────────────────────────────────────

    #[test]
    fn timekeeping_widget_contributions_count_and_ids() {
        let widgets = super::widget_contributions();
        assert_eq!(widgets.len(), 3);
        assert_eq!(widgets[0].id, "stopwatch");
        assert_eq!(widgets[1].id, "time-log");
        assert_eq!(widgets[2].id, "pomodoro");
    }

    #[test]
    fn timekeeping_widgets_are_temporal_category() {
        use crate::widget::WidgetCategory;
        let widgets = super::widget_contributions();
        for w in &widgets {
            assert!(matches!(w.category, WidgetCategory::Temporal));
        }
    }

    #[test]
    fn stopwatch_has_expected_signals_and_toolbar() {
        let widgets = super::widget_contributions();
        let sw = &widgets[0];
        assert_eq!(sw.signals.len(), 4);
        assert_eq!(sw.toolbar_actions.len(), 5);
    }

    #[test]
    fn pomodoro_has_config_fields() {
        let widgets = super::widget_contributions();
        let pom = &widgets[2];
        assert_eq!(pom.config_fields.len(), 3);
        assert_eq!(pom.config_fields[0].key, "work_minutes");
        assert_eq!(pom.config_fields[1].key, "break_minutes");
        assert_eq!(pom.config_fields[2].key, "long_break_minutes");
    }
}
