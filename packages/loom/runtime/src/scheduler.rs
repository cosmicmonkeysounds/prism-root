//! Tiered scheduler (spec §12.5).
//!
//! Round-robin breaks at scale. Every coroutine the runtime knows
//! about runs in one of three tiers — `focal` (every frame, ~16ms),
//! `active` (every ~100ms), and `ambient` (every ~2s) — each with a
//! per-tick wall-clock budget.
//!
//! [`Scheduler::tick`] is the single entry point: it walks every
//! ready coroutine for the current tick (focal always; active /
//! ambient gated on their cadence), steps each one until it parks /
//! returns / its tier's budget runs out, and drops finished
//! coroutines. Focal-tier scenes are never deferred — when focal
//! would miss its budget the scheduler steals slack from `ambient`
//! and `active` before dropping a focal frame.

use std::time::{Duration, Instant};

use crate::coroutine::{Coroutine, CoroutineStatus, Tier};
use crate::expr::World;
use crate::ledger::{Event, Ledger};

/// Per-tier wall-clock budget per scheduler tick. Defaults follow
/// spec §12.5; tests / hosts can override via
/// [`Scheduler::with_budgets`].
#[derive(Clone, Copy, Debug)]
pub struct Budgets {
    pub focal: Duration,
    pub active: Duration,
    pub ambient: Duration,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            focal: Duration::from_millis(16),
            active: Duration::from_millis(100),
            ambient: Duration::from_secs(2),
        }
    }
}

/// Tick cadence per tier (how often the tier is considered for
/// stepping). Focal ticks every call; active / ambient tick when at
/// least their interval has elapsed since the last admission.
#[derive(Clone, Copy, Debug)]
pub struct Cadence {
    pub active: Duration,
    pub ambient: Duration,
}

impl Default for Cadence {
    fn default() -> Self {
        Self {
            active: Duration::from_millis(100),
            ambient: Duration::from_secs(2),
        }
    }
}

/// One ready coroutine handle. The scheduler owns the `Coroutine`
/// directly — there is no separate task / handle indirection.
#[derive(Debug)]
pub struct CoroutineHandle {
    pub coroutine: Coroutine,
    /// Wall clock at which a `WaitDuration` is allowed to resume.
    pub wake_at: Option<Instant>,
}

/// Three-tier coroutine scheduler.
#[derive(Default)]
pub struct Scheduler {
    pub focal: Vec<CoroutineHandle>,
    pub active: Vec<CoroutineHandle>,
    pub ambient: Vec<CoroutineHandle>,
    budgets: Budgets,
    cadence: Cadence,
    last_active: Option<Instant>,
    last_ambient: Option<Instant>,
    next_id: u64,
}

impl std::fmt::Debug for Scheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scheduler")
            .field("focal", &self.focal.len())
            .field("active", &self.active.len())
            .field("ambient", &self.ambient.len())
            .finish()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_budgets(mut self, budgets: Budgets) -> Self {
        self.budgets = budgets;
        self
    }

    pub fn with_cadence(mut self, cadence: Cadence) -> Self {
        self.cadence = cadence;
        self
    }

    /// Allocate a fresh coroutine id. Caller-owned; the scheduler
    /// also assigns one via [`Self::spawn`].
    pub fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Submit a coroutine into its declared tier. Emits a
    /// `SceneSpawned` envelope so observers see the lifecycle.
    pub fn spawn(&mut self, coroutine: Coroutine, ledger: &mut Ledger) {
        ledger.push(Event::SceneSpawned {
            scene: coroutine.program.name.clone(),
            coroutine: coroutine.id,
            tier: coroutine.tier.name().into(),
        });
        let handle = CoroutineHandle {
            coroutine,
            wake_at: None,
        };
        match handle.coroutine.tier {
            Tier::Focal => self.focal.push(handle),
            Tier::Active => self.active.push(handle),
            Tier::Ambient => self.ambient.push(handle),
        }
    }

    /// Run one scheduler tick. Walks every tier whose cadence has
    /// elapsed and gives each coroutine as many `step` calls as fit
    /// in the tier's budget. Focal always ticks; when focal would
    /// miss its budget it steals from ambient and active first
    /// (spec §12.5).
    pub fn tick(&mut self, now: Instant, world: &mut World, ledger: &mut Ledger) {
        let mut focal_taken = drive_tier(&mut self.focal, self.budgets.focal, now, world, ledger);
        // Budget stealing: if focal exhausted its own budget mid-list
        // (work remains), pull from active / ambient before yielding
        // the tick.
        if focal_taken >= self.budgets.focal && self.focal.iter().any(|h| !h.coroutine.is_done()) {
            let stolen_active = self.budgets.active / 2;
            let stolen_ambient = self.budgets.ambient / 4;
            focal_taken += drive_tier(&mut self.focal, stolen_active, now, world, ledger);
            focal_taken += drive_tier(&mut self.focal, stolen_ambient, now, world, ledger);
        }

        let due_active = self
            .last_active
            .map(|t| now.duration_since(t) >= self.cadence.active)
            .unwrap_or(true);
        if due_active {
            drive_tier(&mut self.active, self.budgets.active, now, world, ledger);
            self.last_active = Some(now);
        }
        let due_ambient = self
            .last_ambient
            .map(|t| now.duration_since(t) >= self.cadence.ambient)
            .unwrap_or(true);
        if due_ambient {
            drive_tier(&mut self.ambient, self.budgets.ambient, now, world, ledger);
            self.last_ambient = Some(now);
        }

        self.focal.retain(|h| !h.coroutine.is_done());
        self.active.retain(|h| !h.coroutine.is_done());
        self.ambient.retain(|h| !h.coroutine.is_done());
    }

    /// Total live coroutines across all tiers.
    pub fn len(&self) -> usize {
        self.focal.len() + self.active.len() + self.ambient.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Drive one tier's coroutine list for at most `budget` wall-clock
/// time. Returns the elapsed time actually spent in this slice.
fn drive_tier(
    queue: &mut [CoroutineHandle],
    budget: Duration,
    now: Instant,
    world: &mut World,
    ledger: &mut Ledger,
) -> Duration {
    let start = Instant::now();
    let mut i = 0;
    while i < queue.len() {
        if start.elapsed() >= budget {
            break;
        }
        let handle = &mut queue[i];
        if let Some(wake) = handle.wake_at {
            if now < wake {
                i += 1;
                continue;
            }
            handle.wake_at = None;
        }
        if handle.coroutine.is_done() {
            i += 1;
            continue;
        }
        match handle.coroutine.step(world, ledger) {
            CoroutineStatus::Running | CoroutineStatus::Yielded => {}
            CoroutineStatus::Waiting { until } => {
                // Wall-clock waits ("Nms") parse into a wake_at;
                // predicate waits stay re-pollable on the next tick.
                if let Some(ms) = until.strip_suffix("ms") {
                    if let Ok(n) = ms.trim().parse::<u64>() {
                        handle.wake_at = Some(now + Duration::from_millis(n));
                    }
                }
                i += 1;
                continue;
            }
            CoroutineStatus::Returned { .. } => {}
        }
        // Don't increment — let the same coroutine take another step
        // until it parks, finishes, or budget ends.
    }
    start.elapsed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coroutine::{Program, Step};

    fn never_ending_ambient(id: u64) -> Coroutine {
        let prog = Program {
            name: "noise".into(),
            steps: vec![
                Step::EmitLine {
                    text: "tick".into(),
                },
                Step::LoopHead { back_to: 0 },
            ],
            ..Program::default()
        };
        Coroutine::new(id, prog, Tier::Ambient, 0.1)
    }

    fn focal_one_shot(id: u64) -> Coroutine {
        let prog = Program {
            name: "scene".into(),
            steps: vec![
                Step::EmitLine {
                    text: "focal!".into(),
                },
                Step::Return { value: None },
            ],
            ..Program::default()
        };
        Coroutine::new(id, prog, Tier::Focal, 1.0)
    }

    #[test]
    fn focal_scene_ticks_even_when_ambient_is_loud() {
        let mut sched = Scheduler::new();
        let mut world = World::new();
        let mut ledger = Ledger::default();
        // Several ambient generators competing for budget.
        for i in 0..4 {
            sched.spawn(never_ending_ambient(100 + i), &mut ledger);
        }
        sched.spawn(focal_one_shot(1), &mut ledger);
        let now = Instant::now();
        sched.tick(now, &mut world, &mut ledger);
        // The focal coroutine must have completed.
        assert!(
            ledger.events().iter().any(|e| matches!(
                e,
                Event::SceneCompleted { scene, .. } if scene == "scene"
            )),
            "focal coroutine never returned: {:?}",
            ledger.events()
        );
    }

    #[test]
    fn ambient_generator_yields_when_ticked() {
        let mut sched = Scheduler::new();
        let mut world = World::new();
        let mut ledger = Ledger::default();
        let prog = Program {
            name: "Harbor".into(),
            steps: vec![Step::YieldBark {
                choices: vec!["Quiet night.".into(), "Stars are out.".into()],
            }],
            ..Program::default()
        };
        sched.spawn(Coroutine::new(1, prog, Tier::Ambient, 0.3), &mut ledger);
        sched.tick(Instant::now(), &mut world, &mut ledger);
        assert!(ledger.events().iter().any(|e| matches!(
            e,
            Event::GeneratorYielded { generator, .. } if generator == "Harbor"
        )));
    }
}
