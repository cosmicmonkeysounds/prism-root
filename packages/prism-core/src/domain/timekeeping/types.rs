//! Pure data types for the timekeeping engine.
//!
//! Port of `@core/timekeeping` type definitions. Timer phase
//! enumeration, event signals, time entries, and serializable
//! snapshots.

use serde::{Deserialize, Serialize};

// ── Timer Phase ───────────────────────────────────────────────────

/// The current phase of a stopwatch state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimerPhase {
    Idle,
    Running,
    Paused,
    Stopped,
}

// ── Timer Events ──────────────────────────────────────────────────

/// Discrete command events that drive timer transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimerEvent {
    Start,
    Pause,
    Resume,
    Stop,
    Reset,
}

// ── Time Entry ────────────────────────────────────────────────────

/// A completed time-tracking record emitted on `stop()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeEntry {
    /// ISO-8601 timestamp of when the timer was started.
    pub started_at: String,
    /// ISO-8601 timestamp of when the timer was stopped.
    pub stopped_at: String,
    /// Active (non-paused) duration in milliseconds.
    pub elapsed_ms: u64,
    /// Optional human label for this entry.
    pub label: Option<String>,
    /// Optional associated object ID.
    pub object_id: Option<String>,
    /// Optional associated object type.
    pub object_type: Option<String>,
}

// ── Timer Snapshot ────────────────────────────────────────────────

/// Serializable snapshot of a stopwatch's full state for
/// persistence and hot-reload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerSnapshot {
    pub phase: TimerPhase,
    /// Epoch-ms when the timer was first started.
    pub started_at: Option<u64>,
    /// Accumulated active milliseconds (completed segments).
    pub elapsed: u64,
    /// Epoch-ms when the current running segment began.
    pub segment_start: Option<u64>,
    pub label: Option<String>,
    pub object_id: Option<String>,
    pub object_type: Option<String>,
}

// ── Hook Set ──────────────────────────────────────────────────────

type StopCallback = Box<dyn Fn(&TimeEntry)>;

/// Optional callbacks invoked on timer transitions.
#[derive(Default)]
pub struct TimerHookSet {
    pub on_start: Option<Box<dyn Fn()>>,
    pub on_pause: Option<Box<dyn Fn()>>,
    pub on_resume: Option<Box<dyn Fn()>>,
    pub on_stop: Option<StopCallback>,
    pub on_reset: Option<Box<dyn Fn()>>,
}
