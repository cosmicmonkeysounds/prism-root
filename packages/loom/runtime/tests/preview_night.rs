//! Spec v3 §13 — end-to-end load + play of the preview-night example.
//! Bundles ROLE / PERSON / ROSTER declarations and a prologue beat
//! that loads the roster and casts Jamie in as Wren. Asserts the
//! bundle materialised the new registries, the constructor argument
//! lifted Wren's strength, and the directives wrote the expected
//! envelopes + world bindings.

use std::path::PathBuf;
use std::sync::Arc;

use loom_runtime::{Bundle, Event, Playhead, Step, Value};

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples")
        .join("preview-night")
}

#[test]
fn preview_night_loads_persons_rosters_and_runs_cast_directives() {
    let bundle = Bundle::load(example_root()).expect("load preview-night");
    assert!(
        bundle.project_diagnostics.is_empty(),
        "project diagnostics: {:?}",
        bundle.project_diagnostics
    );

    // Registries populated.
    assert!(bundle.persons.contains_key("jamie_lee"));
    assert!(bundle.persons.contains_key("raja_park"));
    assert!(bundle.persons.contains_key("kim_ho"));
    assert!(bundle.rosters.contains_key("preview_night_05_28"));
    // ROLE materialises into the same character registry as CHARACTER.
    assert!(bundle.characters.contains_key("Wren"));

    // Constructor arg lifted Wren's strength from 10 → 12.
    let wren = bundle.characters.get("Wren").unwrap();
    let stats = wren
        .stats
        .as_ref()
        .expect("Wren has the Combat profile attached");
    assert_eq!(stats.attributes.get("strength").copied(), Some(12.0));

    // PERSON fields round-trip.
    let jamie = bundle.persons.get("jamie_lee").unwrap();
    assert_eq!(jamie.display_name.as_deref(), Some("Jamie Lee"));
    assert_eq!(jamie.pronouns.as_deref(), Some("they/them"));
    assert_eq!(jamie.content_tolerance, vec!["no_strobe"]);

    // ROSTER picked up the cast block.
    let roster = bundle.rosters.get("preview_night_05_28").unwrap();
    assert_eq!(roster.cast.len(), 3);
    assert_eq!(roster.swings.len(), 2);

    // Play the prologue — should fire <load_roster:> and <cast:>.
    let mut p = Playhead::new(Arc::new(bundle)).expect("playhead");
    for _ in 0..32 {
        match p.step().expect("step") {
            Step::Ended => break,
            _ => {}
        }
    }

    // World reflects cast + roster.
    let world = p.world();
    match world.get("Wren.player") {
        Value::String(s) => assert_eq!(s, "jamie_lee"),
        other => panic!("Wren.player = {:?}", other),
    }
    match world.get("jamie_lee.role") {
        Value::String(s) => assert_eq!(s, "Wren"),
        other => panic!("jamie_lee.role = {:?}", other),
    }
    match world.get("Roster.active") {
        Value::String(s) => assert_eq!(s, "preview_night_05_28"),
        other => panic!("Roster.active = {:?}", other),
    }

    // Ledger received the right envelopes.
    let events = p.ledger().events();
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::RosterLoaded { roster } if roster == "preview_night_05_28")));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::CastBound { person, role }
            if person == "jamie_lee" && role == "Wren")));
}
