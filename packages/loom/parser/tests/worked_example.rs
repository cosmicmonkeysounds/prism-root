//! Parses the worked example from `docs/dev/loom-v3.html` §16 and
//! asserts structural invariants. This is the Phase-2 vertical slice:
//! the parser swallows a realistic multi-file project without
//! producing diagnostics, and the resulting AST exposes the shapes
//! later phases (resolver, playhead, reactive graph) will read.

use loom_parser::ast::{BodyItem, DeclarationKind, Divert, Item};
use loom_parser::parse;

const MAIN_LOOM: &str = "\
# Saltmere
entry: opening

let trusted = Wren.trusts.Player > 50

== opening
  cast: Wren, Player
  setting: Lighthouse

INT. LIGHTHOUSE - DAWN

```warn lx_dawn```

A bell rope swings in the gloom. Wren watches it from the doorway.

WREN
  (quietly)
  It hasn't rung in three days.

She turns the lantern up.

* Ring the bell.[ I shouldn't, but I do.]
  -> ringing
* Ask about the keepers.
  -> ask_about with topic: keepers, NPC: Wren
* Leave quietly.
  -> END
";

const WREN_LOOM: &str = "\
CHARACTER Wren is Keeper, Combatant
  stats:      Combat
  voice:      female_alto
  home:       Lighthouse
  reputation: 60
  hp:         80
";

const RINGING_LOOM: &str = "\
== ringing
  cast: Wren, Player
  setting: Lighthouse

The sound carries across the rocks.

WREN
  (stunned)
  You... rang it.
";

#[test]
fn main_file_parses_cleanly() {
    let (file, diags) = parse(MAIN_LOOM);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    assert_eq!(file.header.title.as_deref(), Some("Saltmere"));
    assert_eq!(
        file.header
            .properties
            .get("entry")
            .map(|v| v.value.as_str()),
        Some("opening")
    );

    // One top-level let + one beat.
    let mut saw_let = false;
    let mut beats = 0;
    for item in &file.items {
        match item {
            Item::LetBinding(l) => {
                saw_let = true;
                assert_eq!(l.name, "trusted");
            }
            Item::Beat(b) => {
                beats += 1;
                assert_eq!(b.name, "opening");
                assert_eq!(b.contract.get("cast").unwrap().value, "Wren, Player");
                assert_eq!(b.contract.get("setting").unwrap().value, "Lighthouse");
            }
            Item::Declaration(_) => panic!("main.loom has no declarations"),
        }
    }
    assert!(
        saw_let,
        "expected `let trusted = …` to land as an Item::LetBinding"
    );
    assert_eq!(beats, 1);

    let beat = file
        .items
        .iter()
        .find_map(|i| match i {
            Item::Beat(b) => Some(b),
            _ => None,
        })
        .unwrap();

    // Body should contain: scene heading, metadata fence, action,
    // dialogue, action, three choices.
    let mut kinds = Vec::new();
    for item in &beat.body {
        kinds.push(match item {
            BodyItem::SceneHeading(_) => "scene",
            BodyItem::Metadata(_) => "meta",
            BodyItem::Action(_) => "action",
            BodyItem::Dialogue(_) => "dialogue",
            BodyItem::Choice(_) => "choice",
            BodyItem::Divert(_) => "divert",
            BodyItem::Directive(_) => "directive",
            BodyItem::Conditional(_) => "conditional",
            BodyItem::DirectiveBlock(_) => "directive-block",
        });
    }
    assert_eq!(
        kinds,
        vec!["scene", "meta", "action", "dialogue", "action", "choice", "choice", "choice"],
        "beat body shape drifted"
    );

    // The first choice carries an Ink-style suppression.
    let choices: Vec<_> = beat
        .body
        .iter()
        .filter_map(|b| match b {
            BodyItem::Choice(c) => Some(c),
            _ => None,
        })
        .collect();
    assert_eq!(choices.len(), 3);
    assert_eq!(choices[0].text, "Ring the bell.");
    assert_eq!(
        choices[0].suppressed.as_deref(),
        Some(" I shouldn't, but I do.")
    );

    // The second choice parameterises the divert.
    match choices[1].body.first().unwrap() {
        BodyItem::Divert(Divert::To { target, params, .. }) => {
            assert_eq!(target.name, "ask_about");
            assert!(target.qualifier.is_none());
            assert_eq!(params.get("topic").map(String::as_str), Some("keepers"));
            assert_eq!(params.get("NPC").map(String::as_str), Some("Wren"));
        }
        other => panic!("expected parameterised divert, got {other:?}"),
    }

    // The third choice terminates the playhead.
    match choices[2].body.first().unwrap() {
        BodyItem::Divert(Divert::End { .. }) => {}
        other => panic!("expected -> END, got {other:?}"),
    }
}

#[test]
fn character_file_keeps_body_raw() {
    let (file, diags) = parse(WREN_LOOM);
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(file.items.len(), 1);
    let decl = match &file.items[0] {
        Item::Declaration(d) => d,
        other => panic!("expected declaration, got {other:?}"),
    };
    assert_eq!(decl.kind, DeclarationKind::Character);
    assert_eq!(decl.name, "Wren");
    assert_eq!(decl.mixin, vec!["Keeper", "Combatant"]);
    // 5 body lines — `stats`, `voice`, `home`, `reputation`, `hp`.
    assert_eq!(decl.body.len(), 5);
    assert!(decl.body[0].text.starts_with("stats:"));
    assert!(decl.body[4].text.starts_with("hp:"));
}

#[test]
fn ringing_beat_dialogue_block() {
    let (file, diags) = parse(RINGING_LOOM);
    assert!(diags.is_empty(), "{diags:?}");
    let beat = match &file.items[0] {
        Item::Beat(b) => b,
        _ => panic!(),
    };
    assert_eq!(beat.name, "ringing");
    let dialogue = beat
        .body
        .iter()
        .find_map(|b| match b {
            BodyItem::Dialogue(d) => Some(d),
            _ => None,
        })
        .unwrap();
    assert_eq!(dialogue.speaker, "WREN");
    assert_eq!(dialogue.parenthetical.as_deref(), Some("stunned"));
    assert_eq!(dialogue.lines.len(), 1);
}
