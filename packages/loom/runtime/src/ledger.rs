//! Append-only event ledger.
//!
//! The ledger is the canonical record of everything the playhead
//! emits — the same stream that drives the live-performance scope
//! filters (spec §13), the `since(…)` / `visits(…)` query helpers
//! (spec §12.2), and the booth's transcript view (spec §13.4).
//!
//! Phase-3 keeps it deliberately small: one envelope per visible
//! step. Knowledge / disposition / stat-change envelopes land when
//! Simulacra and Meridian come online.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::bundle::BeatRef;
use crate::expr::{CallArg, ExprError, Value};

/// One event written by the playhead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// The playhead entered a beat.
    BeatEntered {
        beat: String,
        file: PathBuf,
        #[serde(skip)]
        reference: Option<BeatRef>,
    },
    /// A Fountain-style scene heading was reached.
    Scene { text: String },
    /// A flush-left prose paragraph was reached.
    Action { text: String },
    /// A speaker delivered one line. Parenthetical performer
    /// direction is carried alongside for stage / booth display.
    Dialogue {
        speaker: String,
        parenthetical: Option<String>,
        text: String,
    },
    /// A production-metadata fence was reached. The runtime does
    /// nothing with it at production (spec §15) but stages /
    /// debuggers / linters may.
    Metadata { text: String },
    /// The playhead is offering a set of choices and waiting for
    /// `Playhead::choose`.
    ChoicePrompted { options: Vec<ChoiceOption> },
    /// A choice was taken — recorded after `Playhead::choose`.
    ChoiceTaken { index: usize, text: String },
    /// A `->` divert resolved and was followed.
    Diverted { target: String, beat: String },
    /// A `-> (…) ->` tunnel call entered.
    Tunneled { target: String, beat: String },
    /// A `<-` tunnel return popped back to the caller.
    Returned,
    /// A `-> END` (or otherwise exhausted) playhead halted.
    Ended,
    /// A directive call resolved against the registry. The generic
    /// envelope — `sfx`, `cue`, `pause`, `anchor` reach the consumer
    /// through this variant. `set` and `fire` use the more specific
    /// `WorldSet` / `Fired` variants below so consumers don't have to
    /// re-parse the call.
    Directive {
        kind: String,
        positional: Vec<String>,
        named: Vec<(String, String)>,
    },
    /// `<set: path OP rhs>` resolved and applied to the World scope.
    WorldSet { path: String, value: String },
    /// `<fire: name, ...>` pushed a named event onto the ledger.
    Fired {
        name: String,
        payload: Vec<(String, String)>,
    },
    /// One arm of a `<if:>/<else if:>/<else>` chain was selected.
    /// `condition` is `None` for the trailing `<else>` arm.
    ConditionalArm { condition: Option<String> },
    /// A top-level `let` binding was (re-)evaluated.
    LetEvaluated { name: String, value: String },
    /// A `spawn` / `run` constructed a new coroutine (spec §12.3).
    SceneSpawned {
        scene: String,
        coroutine: u64,
        tier: String,
    },
    /// One coroutine step advanced a SCENE through a state
    /// transition (`-> state`).
    SceneAdvanced {
        scene: String,
        coroutine: u64,
        state: String,
    },
    /// A SCENE returned and the coroutine completed (spec §12.3).
    SceneCompleted {
        scene: String,
        coroutine: u64,
        value: String,
    },
    /// A GENERATOR or SCENE coroutine yielded ambient content
    /// (`yield bark from list`, `yield with_chance(p) body`).
    GeneratorYielded {
        generator: String,
        coroutine: u64,
        text: String,
    },
    /// A coroutine is parked on a `wait` until its predicate clears
    /// or its duration elapses (spec §12.3).
    CoroutineWaiting {
        coroutine: u64,
        reason: String,
    },
    /// A live participant joined the show (spec §13.1).
    ParticipantJoined { id: String },
    /// A participant was removed from the stage (booth-side retire).
    ParticipantRetired { id: String },
    /// A participant entered a LOCATION (spec §13.1).
    ParticipantEnteredLocation { id: String, location: String },
    /// A participant was enrolled into a COHORT (spec §13.1).
    CohortEnrolled { id: String, cohort: String },
    /// An improv beat started — the playhead is now holding on the
    /// controller until a signal advances it or it times out.
    ImprovBeatStarted { handle: u64 },
    /// An improv beat advanced — either a signal satisfied the
    /// declared quorum or the duration elapsed (`reason` describes
    /// which).
    ImprovBeatAdvanced { handle: u64, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub index: usize,
    pub text: String,
    pub sticky: bool,
}

/// An ordered log of [`Event`]s. Plain `Vec`-backed; mutations only
/// through `push`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    events: Vec<Event>,
}

impl Ledger {
    pub fn push(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Index of the most recent occurrence of an event matching
    /// `predicate`. Returned as the count-from-the-end so callers
    /// can compute "did X happen recently". Used by the §12.2
    /// `since(…)` query primitive once it lands.
    pub fn last_matching<F: Fn(&Event) -> bool>(&self, predicate: F) -> Option<usize> {
        self.events.iter().rposition(predicate)
    }

    /// Count of `BeatEntered` events naming `beat`. Backs the
    /// `visits(name)` ledger query (spec §12.2).
    pub fn beat_visit_count(&self, beat: &str) -> usize {
        self.events
            .iter()
            .filter(|e| matches!(e, Event::BeatEntered { beat: b, .. } if b == beat))
            .count()
    }

    /// `true` once `name` has appeared as either a `BeatEntered`,
    /// a fired `<anchor: name>` directive, or a `<fire: name>`
    /// envelope. Backs the `played(name)` ledger query (spec §12.2).
    pub fn played(&self, name: &str) -> bool {
        self.events.iter().any(|e| event_names(e, name))
    }

    /// Steps elapsed since the most recent event named `name`
    /// (as recognised by [`event_names`]). `None` if it never
    /// happened. Returns the ledger's "logical time" — number of
    /// envelopes between the match and the current end — rather
    /// than wall-clock seconds. Wall-clock backing lands when the
    /// clock subsystem comes online.
    pub fn since(&self, name: &str) -> Option<usize> {
        for (idx, event) in self.events.iter().enumerate().rev() {
            if event_names(event, name) {
                return Some(self.events.len() - idx - 1);
            }
        }
        None
    }
}

fn event_names(event: &Event, name: &str) -> bool {
    match event {
        Event::BeatEntered { beat, .. } => beat == name,
        Event::Fired { name: n, .. } => n == name,
        Event::Directive {
            kind, positional, ..
        } if kind == "anchor" => positional.first().map(|s| s.as_str()) == Some(name),
        _ => false,
    }
}

/// Resolve a query call from an expression — `played(name)`,
/// `visits(name)`, `since(name)`. Bare identifiers (`played(intro)`)
/// are accepted as the literal name via [`CallArg::as_name`]; quoted
/// strings (`played("intro")`) also work.
pub fn call_query(ledger: &Ledger, name: &str, args: &[CallArg<'_>]) -> Result<Value, ExprError> {
    let first_name = || args.first().map(|a| a.as_name()).unwrap_or_default();
    match name {
        "played" => Ok(Value::Bool(ledger.played(&first_name()))),
        "visits" => Ok(Value::Number(ledger.beat_visit_count(&first_name()) as f64)),
        "since" => Ok(match ledger.since(&first_name()) {
            Some(steps) => Value::Number(steps as f64),
            None => Value::Null,
        }),
        other => Err(ExprError::UnknownFunction(other.into())),
    }
}
