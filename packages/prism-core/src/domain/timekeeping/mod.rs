//! `domain::timekeeping` — stopwatch / timer engine.
//!
//! Port of `@core/timekeeping` TypeScript module. Provides a
//! state-machine [`Stopwatch`] that accumulates active running time
//! across pause/resume cycles, emitting a [`TimeEntry`] on stop.
//! Pluggable [`Clock`] trait allows deterministic testing via
//! [`ManualClock`].

pub mod engine;
pub mod types;

pub use engine::{
    widget_contributions, Clock, ManualClock, Stopwatch, StopwatchOptions, SystemClock,
};
pub use types::{TimeEntry, TimerHookSet, TimerPhase, TimerSnapshot};
