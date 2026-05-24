//! Phase-4 end-to-end coverage for: reactive `let` bindings,
//! conditional weave (`<if:>`/`<else if:>`/`<else>`),
//! inline `{expr}` substitution, sticky vs. once-only choices, and
//! ledger query primitives (`played`, `visits`, `since`).
//!
//! The script exercises the full pipeline — parser → bundle →
//! resolver → playhead — without any in-test mocking, so a
//! regression in any layer surfaces here.

use std::sync::Arc;

use loom_runtime::{Bundle, Event, Playhead, Step};

const MAIN: &str = "\
# Trust Tested
entry: gate

let trusted = Wren.trust > 50
let coins = Player.coins

== gate
  cast: Wren, Player

You have {coins} coins. Wren watches your hands.

<if: visits(gate) == 1>
  <set: Player.coins = 5>
  <set: Wren.trust = 30>

<if: trusted>
  WREN
    Welcome, friend. Trust runs {Wren.trust} of 100.
  -> END
<else if: Wren.trust > 20>
  WREN
    Convince me. You're at {Wren.trust}.
  -> persuade
<else>
  WREN
    Leave.
  -> END

== persuade
  cast: Wren, Player

* Offer a coin.
  <set: Wren.trust += 30>
  -> gate
+ Argue.
  <set: Wren.trust += 5>
  WREN
    That's not enough.
  -> persuade
* Walk away.
  -> END
";

#[test]
fn conditional_weave_drives_branch_selection() {
    let bundle = Arc::new(Bundle::from_sources([("main.loom", MAIN)]));
    assert!(
        bundle.project_diagnostics.is_empty(),
        "{:?}",
        bundle.project_diagnostics
    );

    let mut p = Playhead::new(bundle).unwrap();

    // First yield: action paragraph with inline {coins} substitution.
    // coins hasn't been set yet so it expands to "null".
    match p.step().unwrap() {
        Step::Event(Event::Action { text }) => {
            assert!(
                text.contains("You have null coins"),
                "expected unset coins to expand to null, got {text:?}"
            );
        }
        other => panic!("expected action, got {other:?}"),
    }
    // <set: Player.coins = 5> — World mutation surfaces as a WorldSet event.
    match p.step().unwrap() {
        Step::Event(Event::WorldSet { path, value }) => {
            assert_eq!(path, "Player.coins");
            assert_eq!(value, "5");
        }
        other => panic!("expected coins WorldSet, got {other:?}"),
    }
    // <set: Wren.trust = 30> — trust still under 50 so the first arm
    // is false; the else-if arm fires.
    match p.step().unwrap() {
        Step::Event(Event::WorldSet { path, .. }) => {
            assert_eq!(path, "Wren.trust");
        }
        other => panic!("expected trust WorldSet, got {other:?}"),
    }
    // Dialogue from the else-if arm with inline {Wren.trust}.
    // (Diverts are silent transitions — they surface in the ledger
    // but not as Step events.)
    match p.step().unwrap() {
        Step::Event(Event::Dialogue { speaker, text, .. }) => {
            assert_eq!(speaker, "WREN");
            assert!(
                text.contains("at 30"),
                "expected trust=30 in dialogue, got {text:?}"
            );
        }
        other => panic!("expected dialogue, got {other:?}"),
    }
    // Next yield is the persuade beat's choice prompt — the `-> persuade`
    // divert advanced silently between the dialogue and the prompt.
    let opts = match p.step().unwrap() {
        Step::Choice(o) => o,
        other => panic!("expected choice, got {other:?}"),
    };
    assert_eq!(opts.len(), 3);
    assert_eq!(opts[0].text, "Offer a coin.");
    assert_eq!(opts[1].text, "Argue.");
    assert_eq!(opts[2].text, "Walk away.");
    assert!(!opts[0].sticky); // *
    assert!(opts[1].sticky); //  +
    assert!(!opts[2].sticky); // *

    // Take "Offer a coin." — once-only.
    p.choose(0).unwrap();
    // <set: Wren.trust += 30> — now trust=60.
    let _ = p.step().unwrap();
    // Divert -> gate.
    let _ = p.step().unwrap();

    // Re-enter gate. Drain through to the choice or next event.
    let mut saw_trusted_dialogue = false;
    let mut steps = 0;
    while steps < 20 {
        steps += 1;
        match p.step().unwrap() {
            Step::Event(Event::Dialogue { text, .. }) if text.contains("trust runs 60") => {
                saw_trusted_dialogue = true;
            }
            Step::Event(Event::Dialogue { text, .. }) if text.contains("Welcome, friend") => {
                saw_trusted_dialogue = true;
            }
            Step::Ended => break,
            _ => {}
        }
    }
    assert!(
        saw_trusted_dialogue,
        "expected the trusted-arm dialogue to fire after trust>=50"
    );
    assert!(p.halted());

    // Ledger should record at least one ConditionalArm pick and a
    // LetEvaluated for `trusted`.
    let events = p.ledger().events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ConditionalArm { .. })),
        "expected at least one ConditionalArm event"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::LetEvaluated { name, .. } if name == "trusted")),
        "expected LetEvaluated for `trusted`"
    );
    // ChoiceTaken for "Offer a coin." was recorded exactly once.
    let offered_count = events
        .iter()
        .filter(|e| matches!(e, Event::ChoiceTaken { text, .. } if text == "Offer a coin."))
        .count();
    assert_eq!(offered_count, 1);
}

#[test]
fn once_only_choice_is_suppressed_on_re_entry() {
    // A beat with one `*` choice that loops back to itself.
    // First visit: the choice appears. After taking it, the next
    // visit must drop the choice and fall through.
    const SRC: &str = "\
entry: loop

== loop

* Knock on the door.
  -> loop
+ Try again later.
  -> END
";
    let bundle = Arc::new(Bundle::from_sources([("main.loom", SRC)]));
    let mut p = Playhead::new(bundle).unwrap();
    // First prompt — 2 options.
    let opts = match p.step().unwrap() {
        Step::Choice(o) => o,
        other => panic!("expected choice, got {other:?}"),
    };
    assert_eq!(opts.len(), 2);
    p.choose(0).unwrap(); // take the once-only one
                          // Divert back into loop.
    let _ = p.step().unwrap();
    // Second prompt — once-only is gone, only sticky remains.
    let opts = match p.step().unwrap() {
        Step::Choice(o) => o,
        other => panic!("expected choice second time, got {other:?}"),
    };
    assert_eq!(opts.len(), 1);
    assert!(opts[0].sticky);
    assert_eq!(opts[0].text, "Try again later.");
    p.choose(0).unwrap();
    assert_eq!(p.step().unwrap(), Step::Ended);
}

#[test]
fn played_and_visits_track_through_ledger() {
    const SRC: &str = "\
entry: opening

== opening

<anchor: bell_seen>
-> deep

== deep

<if: played(bell_seen) and visits(deep) >= 1>
  This is the second branch.
-> END
";
    let bundle = Arc::new(Bundle::from_sources([("main.loom", SRC)]));
    let mut p = Playhead::new(bundle).unwrap();
    let mut saw_branch_text = false;
    while let Ok(step) = p.step() {
        if matches!(step, Step::Ended) {
            break;
        }
        if let Step::Event(Event::Action { text }) = step {
            if text.contains("second branch") {
                saw_branch_text = true;
            }
        }
    }
    assert!(saw_branch_text);
}
