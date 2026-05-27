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
    /// `speakers` carries the full split list — `DOCKHAND | FISHER` →
    /// `["DOCKHAND", "FISHER"]` — so live booth code can address all
    /// performers (spec §16). `speaker` is the joined-display fallback
    /// for single-performer consumers; empty `speakers` falls back to
    /// `vec![speaker.clone()]` semantically.
    Dialogue {
        speaker: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        speakers: Vec<String>,
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
    /// A `<set: Character.knows.field …>` mutation routed through the
    /// character store (spec §10.2). Carries the character + field
    /// separately so the booth and save layer can round-trip
    /// knowledge edits without re-parsing the dotted path.
    KnowledgeChanged {
        character: String,
        field: String,
        value: String,
    },
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
    CoroutineWaiting { coroutine: u64, reason: String },
    /// A live participant joined the show (spec §13.1).
    ParticipantJoined { id: String },
    /// A participant was removed from the stage (booth-side retire).
    ParticipantRetired { id: String },
    /// A participant was re-cast — every state previously attached to
    /// `old_id` now lives on `new_id` (spec §13.4 booth live-patch).
    ParticipantRecast { old_id: String, new_id: String },
    /// The booth skipped past the named beat (spec §13.4).
    BeatSkipped { beat: String },
    /// The bundle was hot-reloaded while the show was live; the
    /// playhead has just re-entered the new bundle's entry beat
    /// (spec §13.4).
    BundleReloaded,
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
    /// `<after: cond>` latched true on this beat — subsequent visits
    /// play the `after` arm without re-checking the condition (spec §14.2).
    AfterLatched { beat: String, anchor: String },
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
    /// `true` if `AfterLatched { beat, anchor }` has been recorded.
    pub fn after_latched(&self, beat: &str, anchor: &str) -> bool {
        self.events.iter().any(|e| {
            matches!(
                e,
                Event::AfterLatched { beat: b, anchor: a } if b == beat && a == anchor
            )
        })
    }

    pub fn since(&self, name: &str) -> Option<usize> {
        for (idx, event) in self.events.iter().enumerate().rev() {
            if event_names(event, name) {
                return Some(self.events.len() - idx - 1);
            }
        }
        None
    }

    /// Scoped form of [`Self::since`] — `since(Participant, bell_rung)`
    /// (spec §12.2). Walks the ledger newest-first and counts only
    /// events that originated in or are scoped to `scope` (a
    /// participant id today; the same predicate matches dialogue
    /// `speaker`, world writes on `<scope>.…`, and live-stage envelopes
    /// carrying `scope` as their id). Returns the number of events
    /// between the match and the end, or `None`.
    pub fn since_scoped(&self, scope: &str, name: &str) -> Option<usize> {
        for (idx, event) in self.events.iter().enumerate().rev() {
            if !event_in_scope(event, scope) {
                continue;
            }
            let matches = event_names(event, name)
                || match event {
                    Event::WorldSet { path, .. } => path
                        .rsplit_once('.')
                        .map(|(_, tail)| tail == name)
                        .unwrap_or(false),
                    _ => false,
                };
            if matches {
                return Some(self.events.len() - idx - 1);
            }
        }
        None
    }

    /// `last(target, speaker)` — the speaker of the most recent
    /// `Dialogue` envelope whose parenthetical / performer direction
    /// names `target` (spec §12.2). Returns `None` when nothing has
    /// addressed `target` yet.
    pub fn last_speaker_to(&self, target: &str) -> Option<&str> {
        for event in self.events.iter().rev() {
            if let Event::Dialogue {
                speaker,
                parenthetical,
                ..
            } = event
            {
                let mentions = parenthetical
                    .as_deref()
                    .map(|p| {
                        p.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                            .any(|w| w == target)
                    })
                    .unwrap_or(false);
                if mentions {
                    return Some(speaker.as_str());
                }
            }
        }
        None
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn last_speaker_to_finds_most_recent_dialogue_addressing_target() {
        let mut l = Ledger::default();
        l.push(Event::Dialogue {
            speaker: "Wren".into(),
            speakers: vec!["Wren".into()],
            parenthetical: Some("to Player".into()),
            text: "Hello.".into(),
        });
        l.push(Event::Dialogue {
            speaker: "Player".into(),
            speakers: vec!["Player".into()],
            parenthetical: Some("to Wren".into()),
            text: "Hi.".into(),
        });
        assert_eq!(l.last_speaker_to("Wren"), Some("Player"));
        assert_eq!(l.last_speaker_to("Player"), Some("Wren"));
        assert_eq!(l.last_speaker_to("Stranger"), None);
    }

    #[test]
    fn since_scoped_filters_to_participant_stream() {
        let mut l = Ledger::default();
        l.push(Event::Fired {
            name: "bell_rung".into(),
            payload: Vec::new(),
        });
        // A scoped (participant-specific) bell_rung — modelled today as
        // a world write on `<scope>.bell_rung`.
        l.push(Event::WorldSet {
            path: "p17.bell_rung".into(),
            value: "true".into(),
        });
        // Tail events so the scoped match is older.
        l.push(Event::Action {
            text: "filler".into(),
        });
        l.push(Event::Action {
            text: "filler".into(),
        });
        let unscoped = l.since("bell_rung").expect("unscoped fire visible");
        let scoped = l
            .since_scoped("p17", "bell_rung")
            .expect("scoped world-write visible");
        // Both queries find a match. Logical-time deltas are
        // count-from-end: the unscoped Fired is the oldest match (3
        // events later), the scoped WorldSet is more recent (2 events
        // later).
        assert_eq!(scoped, 2);
        assert_eq!(unscoped, 3);
    }
}

/// True when `event` is scoped to a participant / character id
/// (dialogue speaker, world writes on `<scope>.…`, live-stage
/// envelopes carrying `scope` as their id).
fn event_in_scope(event: &Event, scope: &str) -> bool {
    match event {
        Event::ParticipantJoined { id }
        | Event::ParticipantRetired { id }
        | Event::ParticipantEnteredLocation { id, .. }
        | Event::CohortEnrolled { id, .. } => id == scope,
        Event::ParticipantRecast { old_id, new_id } => old_id == scope || new_id == scope,
        Event::Dialogue { speaker, .. } => speaker == scope,
        Event::WorldSet { path, .. } => path
            .split_once('.')
            .map(|(head, _)| head == scope)
            .unwrap_or(false),
        Event::Fired { payload, .. } => payload.iter().any(|(_, v)| v == scope),
        _ => false,
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
    let nth_name = |n: usize| args.get(n).map(|a| a.as_name()).unwrap_or_default();
    match name {
        "played" => Ok(Value::Bool(ledger.played(&first_name()))),
        "visits" => Ok(Value::Number(ledger.beat_visit_count(&first_name()) as f64)),
        "since" => {
            // Scoped form `since(scope, name)` — when two args are
            // present, walk the scope-filtered stream (spec §12.2).
            if args.len() >= 2 {
                return Ok(match ledger.since_scoped(&first_name(), &nth_name(1)) {
                    Some(steps) => Value::Number(steps as f64),
                    None => Value::Null,
                });
            }
            Ok(match ledger.since(&first_name()) {
                Some(steps) => Value::Number(steps as f64),
                None => Value::Null,
            })
        }
        "last" => {
            // `last(target, field)` — today only `field == speaker` is
            // wired (spec §12.2). Other fields fall through to Null
            // until the ledger grows richer per-entity views.
            let target = first_name();
            let field = nth_name(1);
            if field == "speaker" {
                return Ok(match ledger.last_speaker_to(&target) {
                    Some(s) => Value::String(s.to_string()),
                    None => Value::Null,
                });
            }
            Ok(Value::Null)
        }
        other => Err(ExprError::UnknownFunction(other.into())),
    }
}
