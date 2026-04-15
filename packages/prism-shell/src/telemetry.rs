//! First-paint telemetry.
//!
//! Phase 1 of the Slint migration plan (§11) calls for "first-paint
//! telemetry" so we can track boot cost as the shell grows panels in
//! Phases 2/3. The scaffold here is deliberately minimal: record the
//! instant [`Shell::new`](crate::Shell::new) was called, hand the
//! shell a shared `first_paint_at` slot, and let the Slint rendering
//! notifier push the first observed frame into it.
//!
//! The module is pure data + pure functions so it is testable without
//! a Slint platform backend. [`Shell::run`](crate::Shell::run) wires
//! the rendering-notifier hook — the actual Slint glue lives in
//! `app.rs` to keep the module test surface hermetic.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Shared boot timer. Cheap to clone (`Rc<Cell<_>>`) so the Slint
/// rendering-notifier closure can capture a handle without borrowing
/// the [`Shell`](crate::Shell).
#[derive(Debug, Clone)]
pub struct FirstPaint {
    boot_started_at: Instant,
    first_paint_at: Rc<Cell<Option<Instant>>>,
}

impl FirstPaint {
    /// Start a fresh timer. Callers should build this as early as
    /// possible inside `Shell::new` so the measured window actually
    /// reflects boot cost (store init + Slint compile-time codegen
    /// inclusion + window construction).
    pub fn start() -> Self {
        Self {
            boot_started_at: Instant::now(),
            first_paint_at: Rc::new(Cell::new(None)),
        }
    }

    /// Record the first observed paint. Idempotent — subsequent
    /// paints do not overwrite the recorded instant, so the metric
    /// stays pinned to the first frame.
    pub fn record_first_paint(&self) {
        if self.first_paint_at.get().is_none() {
            self.first_paint_at.set(Some(Instant::now()));
        }
    }

    /// The instant [`FirstPaint::start`] was called. Exposed so the
    /// Slint notifier and tests share a single source of truth.
    pub fn boot_started_at(&self) -> Instant {
        self.boot_started_at
    }

    /// Duration from boot start to the first observed paint. Returns
    /// `None` until [`FirstPaint::record_first_paint`] fires at least
    /// once.
    pub fn duration(&self) -> Option<Duration> {
        self.first_paint_at
            .get()
            .map(|t| t.saturating_duration_since(self.boot_started_at))
    }

    /// True once [`FirstPaint::record_first_paint`] has fired.
    pub fn is_recorded(&self) -> bool {
        self.first_paint_at.get().is_some()
    }
}

impl Default for FirstPaint {
    fn default() -> Self {
        Self::start()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn duration_is_none_until_first_paint_recorded() {
        let tel = FirstPaint::start();
        assert!(!tel.is_recorded());
        assert!(tel.duration().is_none());
    }

    #[test]
    fn record_first_paint_sets_a_positive_duration() {
        let tel = FirstPaint::start();
        thread::sleep(Duration::from_millis(2));
        tel.record_first_paint();
        assert!(tel.is_recorded());
        let d = tel.duration().expect("duration after record");
        assert!(d >= Duration::from_millis(2), "got {d:?}");
    }

    #[test]
    fn record_first_paint_is_idempotent() {
        let tel = FirstPaint::start();
        tel.record_first_paint();
        let first = tel.duration().unwrap();
        thread::sleep(Duration::from_millis(2));
        tel.record_first_paint();
        let second = tel.duration().unwrap();
        assert_eq!(
            first, second,
            "second record_first_paint must not overwrite the pinned instant"
        );
    }

    #[test]
    fn clones_share_state_so_the_slint_notifier_can_observe_from_a_closure() {
        let tel = FirstPaint::start();
        let clone = tel.clone();
        assert!(!clone.is_recorded());
        clone.record_first_paint();
        assert!(tel.is_recorded());
        assert_eq!(tel.duration(), clone.duration());
    }
}
