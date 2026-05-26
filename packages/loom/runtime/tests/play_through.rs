//! Plays the §16 worked example end-to-end through the Phase-3
//! engine: project loader → resolver → playhead → ledger. Confirms
//! the multi-file divert from `main.loom :: opening` into
//! `beats/ringing.loom :: ringing` resolves, dialogue inside the
//! ringing beat is emitted in order, and the playhead halts cleanly
//! at the final `-> END`.

use std::sync::Arc;

use loom_runtime::{Bundle, Event, Playhead, Step};

const MAIN: &str = "\
# Saltmere
entry: opening

== opening
  cast: Wren, Player
  setting: Lighthouse

A bell rope swings in the gloom.

WREN
  (quietly)
  It hasn't rung in three days.

* Ring the bell.
  -> ringing
* Leave quietly.
  -> END
";

const RINGING: &str = "\
== ringing
  cast: Wren, Player
  setting: Lighthouse

The sound carries across the rocks.

WREN
  (stunned)
  You... rang it.

* So it begins.
  -> END
";

#[test]
fn project_plays_opening_to_ringing_to_end() {
    let bundle = Arc::new(Bundle::from_sources([
        ("main.loom", MAIN),
        ("beats/ringing.loom", RINGING),
    ]));
    assert!(
        bundle.project_diagnostics.is_empty(),
        "{:?}",
        bundle.project_diagnostics
    );

    let mut p = Playhead::new(bundle).unwrap();

    // 1. Action paragraph from opening.
    match p.step().unwrap() {
        Step::Event(Event::Action { text }) => {
            assert!(text.starts_with("A bell rope swings"));
        }
        other => panic!("expected Action, got {other:?}"),
    }
    // 2. Wren's quiet line.
    match p.step().unwrap() {
        Step::Event(Event::Dialogue {
            speaker,
            speakers: _,
            parenthetical,
            text,
        }) => {
            assert_eq!(speaker, "WREN");
            assert_eq!(parenthetical.as_deref(), Some("quietly"));
            assert_eq!(text, "It hasn't rung in three days.");
        }
        other => panic!("expected Dialogue, got {other:?}"),
    }
    // 3. Choice prompt with two options.
    match p.step().unwrap() {
        Step::Choice(opts) => {
            assert_eq!(opts.len(), 2);
            assert_eq!(opts[0].text, "Ring the bell.");
            assert_eq!(opts[1].text, "Leave quietly.");
        }
        other => panic!("expected Choice, got {other:?}"),
    }
    // 4. Take "Ring the bell." — playhead diverts into the ringing
    //    beat.
    p.choose(0).unwrap();
    match p.step().unwrap() {
        Step::Event(Event::Action { text }) => {
            assert_eq!(text, "The sound carries across the rocks.");
        }
        other => panic!("expected ringing's first action, got {other:?}"),
    }
    // 5. Wren stunned dialogue.
    match p.step().unwrap() {
        Step::Event(Event::Dialogue {
            speaker,
            speakers: _,
            parenthetical,
            text,
        }) => {
            assert_eq!(speaker, "WREN");
            assert_eq!(parenthetical.as_deref(), Some("stunned"));
            assert_eq!(text, "You... rang it.");
        }
        other => panic!("expected ringing's dialogue, got {other:?}"),
    }
    // 6. Ringing's terminal choice.
    match p.step().unwrap() {
        Step::Choice(opts) => {
            assert_eq!(opts.len(), 1);
            assert_eq!(opts[0].text, "So it begins.");
        }
        other => panic!("expected terminal choice, got {other:?}"),
    }
    p.choose(0).unwrap();
    assert_eq!(p.step().unwrap(), Step::Ended);
    assert!(p.halted());

    // Ledger summary: count event kinds we care about.
    let events = p.ledger().events();
    let dialogue_count = events
        .iter()
        .filter(|e| matches!(e, Event::Dialogue { .. }))
        .count();
    let beat_entered_count = events
        .iter()
        .filter(|e| matches!(e, Event::BeatEntered { .. }))
        .count();
    let diverted_count = events
        .iter()
        .filter(|e| matches!(e, Event::Diverted { .. }))
        .count();
    let choice_taken_count = events
        .iter()
        .filter(|e| matches!(e, Event::ChoiceTaken { .. }))
        .count();
    assert_eq!(beat_entered_count, 2, "should have entered 2 beats");
    assert_eq!(diverted_count, 1, "one cross-file divert");
    assert_eq!(dialogue_count, 2, "two dialogue lines");
    assert_eq!(choice_taken_count, 2, "two choices were taken");
    assert!(matches!(events.last(), Some(Event::Ended)));
}

#[test]
fn leave_quietly_branch_halts_immediately() {
    let bundle = Arc::new(Bundle::from_sources([
        ("main.loom", MAIN),
        ("beats/ringing.loom", RINGING),
    ]));
    let mut p = Playhead::new(bundle).unwrap();
    // Advance to the choice (action + dialogue + prompt).
    p.step().unwrap();
    p.step().unwrap();
    p.step().unwrap();
    p.choose(1).unwrap(); // "Leave quietly." → -> END
    assert_eq!(p.step().unwrap(), Step::Ended);
}
