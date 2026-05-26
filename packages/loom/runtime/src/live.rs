//! Live-performance layer (spec §13).
//!
//! Implements the runtime side of immersive-theatre Loom:
//! participants, cohorts, locations, broadcast scopes, and pluggable
//! improv advancement. The parser produces the shapes; this module
//! holds the in-memory stage and answers "who is this beat playing
//! to" / "did the improv beat advance".
//!
//! The pieces are deliberately small data types — a `Bundle` carries
//! one [`LiveStage`] and the playhead / directives mutate it through
//! the high-level methods here. Hooks for `on participant joins` /
//! `on participant enters X` re-fire by routing the ledger events
//! through the existing [`crate::simulacra::HookEvent`] surface.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

use loom_parser::ast::{AdvanceSignal, ImprovDirective, QuorumOp};

use crate::ledger::{Event, Ledger};

/// Stable identifier for a live audience member. Phase-1 keeps these
/// as opaque strings — the booth allocates one per ticketed entry.
pub type ParticipantId = String;

/// One live audience member (spec §13.1). Carries cohort + location
/// membership plus a free-form presence-state bag so directive
/// handlers can stash per-participant scratch data (current cue,
/// last-seen speech anchor, …).
#[derive(Clone, Debug, Default)]
pub struct Participant {
    pub id: ParticipantId,
    /// Logical join order — for `since(Participant, …)` queries.
    pub joined_at: u64,
    pub cohort_memberships: HashSet<String>,
    pub current_location: Option<String>,
    /// `key → value` per-participant slot. Strings keep it portable
    /// across the wasm boundary; the booth UI stringifies on the way
    /// in.
    pub presence_state: BTreeMap<String, String>,
}

/// Compiled [`loom_parser::ast::CohortBody`] (spec §13.1).
#[derive(Clone, Debug, Default)]
pub struct Cohort {
    pub name: String,
    pub label: Option<String>,
    pub capacity: Option<u32>,
    pub members: HashSet<ParticipantId>,
}

/// Compiled [`loom_parser::ast::LocationBody`] (spec §13.1).
#[derive(Clone, Debug, Default)]
pub struct Location {
    pub name: String,
    pub label: Option<String>,
    pub ambient: Option<String>,
    pub contains: Vec<String>,
    pub capacity: Option<u32>,
    pub occupants: HashSet<ParticipantId>,
}

/// One in-flight improv beat, tracked by [`ImprovController`].
#[derive(Clone, Debug)]
pub struct ImprovHandle {
    pub id: u64,
    pub started_at: Instant,
    pub duration: Option<Duration>,
    pub advance_on: Vec<AdvanceSignal>,
    pub quorum: QuorumOp,
    received: Vec<AdvanceSignal>,
}

impl ImprovHandle {
    /// `true` if `duration` has been declared and has elapsed.
    pub fn timed_out(&self, now: Instant) -> bool {
        self.duration
            .map(|d| now.duration_since(self.started_at) >= d)
            .unwrap_or(false)
    }

    /// Test whether `signal` matches one of the declared `advance_on`
    /// entries. Speech / gesture compare on their anchor / name; the
    /// `Pedal` variant matches unconditionally.
    pub fn matches(&self, signal: &AdvanceSignal) -> bool {
        self.advance_on
            .iter()
            .any(|s| signals_equivalent(s, signal))
    }
}

fn signals_equivalent(a: &AdvanceSignal, b: &AdvanceSignal) -> bool {
    match (a, b) {
        (AdvanceSignal::Pedal, AdvanceSignal::Pedal) => true,
        (AdvanceSignal::Speech { anchor: x }, AdvanceSignal::Speech { anchor: y }) => x == y,
        (AdvanceSignal::Gesture { name: x }, AdvanceSignal::Gesture { name: y }) => x == y,
        _ => false,
    }
}

/// Outcome of [`ImprovController::submit_signal`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImprovOutcome {
    /// Signal recorded but the beat's quorum is not yet satisfied.
    Pending,
    /// The beat's quorum is satisfied; the playhead should advance.
    Advanced,
    /// The signal didn't match any declared `advance_on` entry.
    Ignored,
}

/// Per-stage controller for in-flight improv beats (spec §13.3).
#[derive(Debug, Default)]
pub struct ImprovController {
    next_id: u64,
    handles: BTreeMap<u64, ImprovHandle>,
}

impl ImprovController {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a fresh improv beat from a parser [`ImprovDirective`].
    /// Returns the new handle's id.
    pub fn start(&mut self, directive: &ImprovDirective, now: Instant) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let duration = directive.duration.map(|d| d.to_std());
        let handle = ImprovHandle {
            id,
            started_at: now,
            duration,
            advance_on: directive.advance_on.clone(),
            quorum: directive.quorum,
            received: Vec::new(),
        };
        self.handles.insert(id, handle);
        id
    }

    /// Submit a signal toward `id`. The controller checks the
    /// declared `advance_on` semantics — `any` resolves on the first
    /// match, `all` waits for every declared signal, `quorum(N)`
    /// waits for N matching submissions.
    pub fn submit_signal(&mut self, id: u64, signal: AdvanceSignal) -> ImprovOutcome {
        let Some(handle) = self.handles.get_mut(&id) else {
            return ImprovOutcome::Ignored;
        };
        if !handle.matches(&signal) {
            return ImprovOutcome::Ignored;
        }
        handle.received.push(signal);
        let advanced = match handle.quorum {
            QuorumOp::Any => true,
            QuorumOp::All => handle.advance_on.iter().all(|need| {
                handle
                    .received
                    .iter()
                    .any(|got| signals_equivalent(got, need))
            }),
            QuorumOp::N(n) => handle.received.len() as u32 >= n,
        };
        if advanced {
            self.handles.remove(&id);
            ImprovOutcome::Advanced
        } else {
            ImprovOutcome::Pending
        }
    }

    /// Drain any handles whose `duration` has expired. Returns the
    /// ids of beats the caller should advance. The playhead calls
    /// this from its scheduler tick.
    pub fn drain_timeouts(&mut self, now: Instant) -> Vec<u64> {
        let expired: Vec<u64> = self
            .handles
            .iter()
            .filter(|(_, h)| h.timed_out(now))
            .map(|(id, _)| *id)
            .collect();
        for id in &expired {
            self.handles.remove(id);
        }
        expired
    }

    pub fn handle(&self, id: u64) -> Option<&ImprovHandle> {
        self.handles.get(&id)
    }

    pub fn len(&self) -> usize {
        self.handles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }
}

/// Algebraic broadcast-scope expression (spec §13.2). Evaluates to
/// the set of [`ParticipantId`] that should receive a `<broadcast>`
/// body.
#[derive(Clone, Debug)]
pub enum BroadcastScope {
    Participant(ParticipantId),
    Cohort(String),
    Location(String),
    /// `A and B` — intersection.
    And(Box<BroadcastScope>, Box<BroadcastScope>),
    /// `A but B` — set difference.
    But(Box<BroadcastScope>, Box<BroadcastScope>),
    /// Every participant currently on stage.
    All,
}

impl BroadcastScope {
    /// Evaluate the scope against `stage`'s current membership.
    pub fn evaluate(&self, stage: &LiveStage) -> HashSet<ParticipantId> {
        match self {
            Self::All => stage.participants.keys().cloned().collect(),
            Self::Participant(id) => {
                let mut out = HashSet::new();
                if stage.participants.contains_key(id) {
                    out.insert(id.clone());
                }
                out
            }
            Self::Cohort(name) => stage
                .cohorts
                .get(name)
                .map(|c| c.members.clone())
                .unwrap_or_default(),
            Self::Location(name) => stage
                .locations
                .get(name)
                .map(|l| l.occupants.clone())
                .unwrap_or_default(),
            Self::And(a, b) => a
                .evaluate(stage)
                .intersection(&b.evaluate(stage))
                .cloned()
                .collect(),
            Self::But(a, b) => a
                .evaluate(stage)
                .difference(&b.evaluate(stage))
                .cloned()
                .collect(),
        }
    }
}

/// The live-performance stage — all participants, cohorts, and
/// locations plus the [`ImprovController`] for in-flight beats.
///
/// `cohort_decls` / `location_decls` carry capacity + ambient
/// metadata from the parser; mutating them at runtime (`live-patch`
/// from the booth — spec §13.4) is allowed but rare.
#[derive(Debug, Default)]
pub struct LiveStage {
    pub participants: HashMap<ParticipantId, Participant>,
    pub cohorts: HashMap<String, Cohort>,
    pub locations: HashMap<String, Location>,
    pub improv: ImprovController,
    join_counter: u64,
}

impl LiveStage {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the stage with the COHORT / LOCATION declarations parsed
    /// out of a [`crate::bundle::Bundle`]. Idempotent — calling twice
    /// preserves existing membership.
    pub fn seed_from_bundle(&mut self, bundle: &crate::bundle::Bundle) {
        for (name, decl) in &bundle.cohorts {
            self.cohorts.entry(name.clone()).or_insert_with(|| Cohort {
                name: name.clone(),
                label: decl.label.clone(),
                capacity: decl.capacity,
                members: HashSet::new(),
            });
        }
        for (name, decl) in &bundle.locations {
            self.locations
                .entry(name.clone())
                .or_insert_with(|| Location {
                    name: name.clone(),
                    label: decl.label.clone(),
                    ambient: decl.ambient.clone(),
                    contains: decl.contains.clone(),
                    capacity: decl.capacity,
                    occupants: HashSet::new(),
                });
        }
    }

    /// A new audience member has just been ticketed in. Fires the
    /// `ParticipantJoined` ledger event. The character-store side
    /// (re-firing `on participant joins`) is dispatched by the
    /// playhead through the existing simulacra `HookEvent` surface.
    pub fn participant_joins(
        &mut self,
        id: impl Into<ParticipantId>,
        ledger: &mut Ledger,
    ) -> ParticipantId {
        let id = id.into();
        let joined_at = self.join_counter;
        self.join_counter += 1;
        self.participants
            .entry(id.clone())
            .or_insert_with(|| Participant {
                id: id.clone(),
                joined_at,
                ..Participant::default()
            });
        ledger.push(Event::ParticipantJoined { id: id.clone() });
        id
    }

    /// Move a participant into `location`. Removes them from any
    /// previous location's occupant set. Fires the
    /// `ParticipantEnteredLocation` ledger event.
    pub fn participant_enters(
        &mut self,
        id: &str,
        location: &str,
        ledger: &mut Ledger,
    ) -> Result<(), LiveError> {
        let Some(participant) = self.participants.get_mut(id) else {
            return Err(LiveError::UnknownParticipant(id.to_string()));
        };
        if let Some(prev) = participant.current_location.clone() {
            if let Some(loc) = self.locations.get_mut(&prev) {
                loc.occupants.remove(id);
            }
        }
        participant.current_location = Some(location.to_string());
        let loc = self
            .locations
            .entry(location.to_string())
            .or_insert_with(|| Location {
                name: location.to_string(),
                ..Location::default()
            });
        loc.occupants.insert(id.to_string());
        ledger.push(Event::ParticipantEnteredLocation {
            id: id.to_string(),
            location: location.to_string(),
        });
        Ok(())
    }

    /// Enroll a participant into `cohort`. Idempotent. Fires
    /// `CohortEnrolled`.
    pub fn enroll(&mut self, id: &str, cohort: &str, ledger: &mut Ledger) -> Result<(), LiveError> {
        let Some(participant) = self.participants.get_mut(id) else {
            return Err(LiveError::UnknownParticipant(id.to_string()));
        };
        participant.cohort_memberships.insert(cohort.to_string());
        let cohort_entry = self
            .cohorts
            .entry(cohort.to_string())
            .or_insert_with(|| Cohort {
                name: cohort.to_string(),
                ..Cohort::default()
            });
        cohort_entry.members.insert(id.to_string());
        ledger.push(Event::CohortEnrolled {
            id: id.to_string(),
            cohort: cohort.to_string(),
        });
        Ok(())
    }

    /// Remove a participant entirely (spec §13.4 — the booth retires
    /// audience members who leave the venue).
    pub fn retire(&mut self, id: &str, ledger: &mut Ledger) -> bool {
        let Some(participant) = self.participants.remove(id) else {
            return false;
        };
        if let Some(prev) = participant.current_location {
            if let Some(loc) = self.locations.get_mut(&prev) {
                loc.occupants.remove(id);
            }
        }
        for c in &participant.cohort_memberships {
            if let Some(cohort) = self.cohorts.get_mut(c) {
                cohort.members.remove(id);
            }
        }
        ledger.push(Event::ParticipantRetired { id: id.to_string() });
        true
    }

    /// Start an improv beat through the controller. The playhead
    /// holds on its return value until [`ImprovController::submit_signal`]
    /// resolves, or [`ImprovController::drain_timeouts`] expires it.
    pub fn start_improv(&mut self, directive: &ImprovDirective, now: Instant) -> u64 {
        // Note: the `ImprovBeatStarted` envelope is pushed by the
        // playhead because only it knows which beat the directive is
        // anchored to. We expose the controller-id here as the link.
        self.improv.start(directive, now)
    }
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum LiveError {
    #[error("unknown participant `{0}`")]
    UnknownParticipant(String),
    #[error("unknown cohort `{0}`")]
    UnknownCohort(String),
    #[error("unknown location `{0}`")]
    UnknownLocation(String),
    #[error("broadcast scope expression: {0}")]
    BadScope(String),
}

/// Parse a broadcast-scope expression (the body of `<broadcast: …>`).
/// Grammar: `expr := term ((and|but) term)*` where
/// `term := participant(ID) | cohort(NAME) | location(NAME) | all`.
pub fn parse_broadcast_scope(source: &str) -> Result<BroadcastScope, LiveError> {
    let mut parser = ScopeParser::new(source);
    let scope = parser.parse_expr()?;
    parser.expect_end()?;
    Ok(scope)
}

struct ScopeParser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> ScopeParser<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn rest(&self) -> &str {
        &self.src[self.pos..]
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.rest().chars().next() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn eat_word(&mut self, word: &str) -> bool {
        self.skip_ws();
        if self.rest().starts_with(word) {
            // Word boundary so `andrew` isn't read as `and`+`rew`.
            let after = self.rest()[word.len()..].chars().next();
            if after
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false)
            {
                return false;
            }
            self.pos += word.len();
            return true;
        }
        false
    }

    fn parse_expr(&mut self) -> Result<BroadcastScope, LiveError> {
        let mut left = self.parse_term()?;
        loop {
            self.skip_ws();
            if self.eat_word("and") {
                let right = self.parse_term()?;
                left = BroadcastScope::And(Box::new(left), Box::new(right));
            } else if self.eat_word("but") {
                let right = self.parse_term()?;
                left = BroadcastScope::But(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_term(&mut self) -> Result<BroadcastScope, LiveError> {
        self.skip_ws();
        if self.eat_word("all") {
            return Ok(BroadcastScope::All);
        }
        for (head, ctor) in [
            (
                "participant",
                BroadcastScope::Participant as fn(String) -> BroadcastScope,
            ),
            (
                "cohort",
                BroadcastScope::Cohort as fn(String) -> BroadcastScope,
            ),
            (
                "location",
                BroadcastScope::Location as fn(String) -> BroadcastScope,
            ),
        ] {
            if self.rest().starts_with(head) {
                let after_idx = head.len();
                if self.rest()[after_idx..].trim_start().starts_with('(') {
                    self.pos += after_idx;
                    self.skip_ws();
                    // consume '('
                    self.pos += 1;
                    let close = self.rest().find(')').ok_or_else(|| {
                        LiveError::BadScope(format!("`{head}(…)` is missing `)`"))
                    })?;
                    let inner = self.rest()[..close].trim().to_string();
                    self.pos += close + 1;
                    return Ok(ctor(inner));
                }
            }
        }
        Err(LiveError::BadScope(format!(
            "expected scope term, got `{}`",
            self.rest()
        )))
    }

    fn expect_end(&mut self) -> Result<(), LiveError> {
        self.skip_ws();
        if !self.rest().is_empty() {
            return Err(LiveError::BadScope(format!(
                "unexpected trailing input `{}`",
                self.rest()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_parser::ast::{AdvanceSignal, ImprovDirective, ImprovDuration, ImprovDurationUnit};

    fn improv(dur_s: f64, q: QuorumOp, signals: Vec<AdvanceSignal>) -> ImprovDirective {
        ImprovDirective {
            duration: Some(ImprovDuration {
                value: dur_s,
                unit: ImprovDurationUnit::Seconds,
            }),
            quorum: q,
            advance_on: signals,
            span: loom_parser::source::Span::default(),
        }
    }

    #[test]
    fn participant_joins_emits_event() {
        let mut stage = LiveStage::new();
        let mut ledger = Ledger::default();
        stage.participant_joins("A", &mut ledger);
        assert!(stage.participants.contains_key("A"));
        assert!(matches!(
            ledger.events().last(),
            Some(Event::ParticipantJoined { id }) if id == "A"
        ));
    }

    #[test]
    fn enters_moves_occupant_between_locations() {
        let mut stage = LiveStage::new();
        let mut ledger = Ledger::default();
        stage.participant_joins("A", &mut ledger);
        stage.participant_enters("A", "Nave", &mut ledger).unwrap();
        stage
            .participant_enters("A", "BellTower", &mut ledger)
            .unwrap();
        assert!(!stage.locations["Nave"].occupants.contains("A"));
        assert!(stage.locations["BellTower"].occupants.contains("A"));
        assert_eq!(
            stage.participants["A"].current_location.as_deref(),
            Some("BellTower")
        );
    }

    #[test]
    fn enroll_adds_membership_both_ways() {
        let mut stage = LiveStage::new();
        let mut ledger = Ledger::default();
        stage.participant_joins("A", &mut ledger);
        stage.enroll("A", "Initiates", &mut ledger).unwrap();
        assert!(stage.cohorts["Initiates"].members.contains("A"));
        assert!(stage.participants["A"]
            .cohort_memberships
            .contains("Initiates"));
    }

    #[test]
    fn broadcast_intersection_is_set_and() {
        let mut stage = LiveStage::new();
        let mut ledger = Ledger::default();
        for id in ["A", "B", "C"] {
            stage.participant_joins(id, &mut ledger);
        }
        stage.enroll("A", "Singers", &mut ledger).unwrap();
        stage.enroll("B", "Singers", &mut ledger).unwrap();
        stage
            .participant_enters("A", "BellTower", &mut ledger)
            .unwrap();
        stage
            .participant_enters("C", "BellTower", &mut ledger)
            .unwrap();
        let scope = parse_broadcast_scope("cohort(Singers) and location(BellTower)").unwrap();
        let hit = scope.evaluate(&stage);
        assert_eq!(hit, std::iter::once("A".to_string()).collect());
    }

    #[test]
    fn broadcast_but_is_set_difference() {
        let mut stage = LiveStage::new();
        let mut ledger = Ledger::default();
        stage.participant_joins("A", &mut ledger);
        stage.participant_joins("B", &mut ledger);
        stage
            .participant_enters("A", "BellTower", &mut ledger)
            .unwrap();
        stage
            .participant_enters("B", "BellTower", &mut ledger)
            .unwrap();
        let scope = parse_broadcast_scope("location(BellTower) but participant(B)").unwrap();
        let hit = scope.evaluate(&stage);
        assert_eq!(hit, std::iter::once("A".to_string()).collect());
    }

    #[test]
    fn improv_any_resolves_on_first_signal() {
        let dir = improv(
            45.0,
            QuorumOp::Any,
            vec![
                AdvanceSignal::Pedal,
                AdvanceSignal::Speech {
                    anchor: "ok".into(),
                },
            ],
        );
        let now = Instant::now();
        let mut ctrl = ImprovController::new();
        let id = ctrl.start(&dir, now);
        let out = ctrl.submit_signal(id, AdvanceSignal::Pedal);
        assert_eq!(out, ImprovOutcome::Advanced);
    }

    #[test]
    fn improv_quorum_n_waits_for_n_signals() {
        let dir = improv(
            45.0,
            QuorumOp::N(2),
            vec![
                AdvanceSignal::Pedal,
                AdvanceSignal::Gesture { name: "Bow".into() },
            ],
        );
        let mut ctrl = ImprovController::new();
        let id = ctrl.start(&dir, Instant::now());
        assert_eq!(
            ctrl.submit_signal(id, AdvanceSignal::Pedal),
            ImprovOutcome::Pending
        );
        assert_eq!(
            ctrl.submit_signal(id, AdvanceSignal::Gesture { name: "Bow".into() }),
            ImprovOutcome::Advanced
        );
    }

    #[test]
    fn improv_all_waits_for_every_declared_signal() {
        let dir = improv(
            45.0,
            QuorumOp::All,
            vec![
                AdvanceSignal::Pedal,
                AdvanceSignal::Gesture { name: "Bow".into() },
            ],
        );
        let mut ctrl = ImprovController::new();
        let id = ctrl.start(&dir, Instant::now());
        assert_eq!(
            ctrl.submit_signal(id, AdvanceSignal::Pedal),
            ImprovOutcome::Pending
        );
        // Re-firing pedal does not advance — All needs every distinct
        // declared signal.
        assert_eq!(
            ctrl.submit_signal(id, AdvanceSignal::Pedal),
            ImprovOutcome::Pending
        );
        assert_eq!(
            ctrl.submit_signal(id, AdvanceSignal::Gesture { name: "Bow".into() }),
            ImprovOutcome::Advanced
        );
    }

    #[test]
    fn improv_times_out_after_duration() {
        let dir = improv(0.01, QuorumOp::Any, vec![AdvanceSignal::Pedal]);
        let mut ctrl = ImprovController::new();
        let id = ctrl.start(&dir, Instant::now() - Duration::from_secs(1));
        let expired = ctrl.drain_timeouts(Instant::now());
        assert_eq!(expired, vec![id]);
        assert!(ctrl.is_empty());
    }
}
