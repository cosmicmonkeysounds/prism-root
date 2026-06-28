//! Phase-B invariants (loom-editor.html §11.2):
//!
//! 1. `Mesh::new` auto-seeds tracks for every CHARACTER / PERSON /
//!    top-level GENERATOR plus a Booth track.
//! 2. Cast directives push their envelopes onto the Booth track, not
//!    onto the main scripted track.
//! 3. Participant join / location / cohort / retire / recast events
//!    push onto the participating PERSON's track.
//! 4. The single Scripted track on `TrackId::MAIN` is registered.

use std::path::PathBuf;
use std::sync::Arc;

use loom_runtime::{
    ledger::BOOTH_TRACK_NAME, Bundle, Event, Ledger, LiveStage, Mesh, Step, TrackId,
    TrackIdentity,
};

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples")
        .join("preview-night")
}

fn play_to_end(mesh: &mut Mesh) {
    for _ in 0..64 {
        if let (_, Step::Ended) = mesh.step().expect("step") {
            return;
        }
    }
    panic!("mesh did not halt within 64 steps");
}

#[test]
fn mesh_new_seeds_roles_persons_generators_and_booth() {
    let bundle = Arc::new(Bundle::load(example_root()).expect("load preview-night"));
    let mesh = Mesh::new(bundle).expect("mesh");
    let identities: Vec<_> = mesh.tracks().map(|t| t.identity.clone()).collect();
    // Main is always there.
    assert!(identities.iter().any(|i| matches!(i, TrackIdentity::Main)));
    // Booth is seeded eagerly.
    assert!(identities.iter().any(|i| matches!(i, TrackIdentity::Booth)));
    // Wren is a ROLE so it should have its own row.
    assert!(identities
        .iter()
        .any(|i| matches!(i, TrackIdentity::Role(n) if n == "Wren")));
    // jamie_lee is a PERSON declared in the example.
    assert!(identities
        .iter()
        .any(|i| matches!(i, TrackIdentity::Person(n) if n == "jamie_lee")));
}

#[test]
fn cast_envelopes_land_on_the_booth_track() {
    let bundle = Arc::new(Bundle::load(example_root()).expect("load preview-night"));
    let mut mesh = Mesh::new(bundle).expect("mesh");
    play_to_end(&mut mesh);

    let booth_id = mesh
        .ledger()
        .track_for(BOOTH_TRACK_NAME)
        .expect("booth track registered");
    let bound_on_booth = mesh
        .ledger()
        .iter_with_meta()
        .find(|(_, e, m)| {
            matches!(e, Event::CastBound { .. }) && m.track == booth_id
        });
    assert!(
        bound_on_booth.is_some(),
        "CastBound envelope was not attributed to the Booth track"
    );

    let roster_on_booth = mesh
        .ledger()
        .iter_with_meta()
        .find(|(_, e, m)| {
            matches!(e, Event::RosterLoaded { .. }) && m.track == booth_id
        });
    assert!(roster_on_booth.is_some(), "RosterLoaded not on Booth");
}

#[test]
fn participant_envelopes_land_on_the_persons_track() {
    let mut ledger = Ledger::default();
    let person_track = TrackId(7);
    ledger.register_track("jamie_lee", person_track);

    let mut stage = LiveStage::new();
    stage.participant_joins("jamie_lee", &mut ledger);
    stage
        .participant_enters("jamie_lee", "BellTower", &mut ledger)
        .expect("enter");
    stage
        .enroll("jamie_lee", "Initiates", &mut ledger)
        .expect("enroll");

    let pairs: Vec<_> = ledger.iter_with_meta().collect();
    assert!(
        pairs
            .iter()
            .any(|(_, e, m)| matches!(e, Event::ParticipantJoined { .. }) && m.track == person_track),
        "ParticipantJoined not on jamie_lee's track"
    );
    assert!(
        pairs
            .iter()
            .any(|(_, e, m)| matches!(e, Event::ParticipantEnteredLocation { .. })
                && m.track == person_track),
        "ParticipantEnteredLocation not on jamie_lee's track"
    );
    assert!(
        pairs
            .iter()
            .any(|(_, e, m)| matches!(e, Event::CohortEnrolled { .. })
                && m.track == person_track),
        "CohortEnrolled not on jamie_lee's track"
    );
}

#[test]
fn dialogue_lands_on_the_speakers_character_track() {
    // A spoken line attributes to its speaker's row, case-insensitively
    // (the cue is screenplay ALL-CAPS `VEX`; the track is the `Vex`
    // CHARACTER). A multi-speaker cue rides on the first speaker. A
    // line by an undeclared role (`CYBORG`) and the narrative spine
    // stay on `Main`.
    const SRC: &str = "\
entry: opening

CHARACTER Vex
CHARACTER Praxis

== opening

The lights drop.

VEX | PRAXIS
  Pick a side.

CYBORG
  Beep.

-> END
";
    let bundle = Arc::new(Bundle::from_sources([("main.loom", SRC)]));
    let mut mesh = Mesh::new(bundle).expect("mesh");
    play_to_end(&mut mesh);

    let vex = mesh.ledger().track_for("Vex").expect("Vex track seeded");
    let main = TrackId::MAIN;

    let on = |want: TrackId, pred: &dyn Fn(&Event) -> bool| {
        mesh.ledger()
            .iter_with_meta()
            .any(|(_, e, m)| pred(e) && m.track == want)
    };

    // `VEX | PRAXIS` → Vex's row.
    assert!(
        on(vex, &|e| matches!(e, Event::Dialogue { speaker, .. } if speaker.contains("VEX"))),
        "VEX | PRAXIS dialogue did not land on the Vex track"
    );
    // The undeclared `CYBORG` cue has no row → stays on Main.
    assert!(
        on(main, &|e| matches!(e, Event::Dialogue { speaker, .. } if speaker == "CYBORG")),
        "undeclared CYBORG dialogue should fall back to Main"
    );
    // Narration stays on the spine.
    assert!(
        on(main, &|e| matches!(e, Event::Action { .. })),
        "action/narration should stay on Main"
    );
    // No dialogue leaked onto Main from a declared speaker.
    assert!(
        !on(main, &|e| matches!(e, Event::Dialogue { speaker, .. } if speaker.contains("VEX"))),
        "Vex dialogue must not also appear on Main"
    );
}

#[test]
fn cells_for_main_track_still_track_the_scripted_beats() {
    // Phase-A invariant still holds: the scripted spine sits on MAIN
    // even though other rows now carry their own envelopes.
    let bundle = Arc::new(Bundle::load(example_root()).expect("load preview-night"));
    let mut mesh = Mesh::new(bundle).expect("mesh");
    play_to_end(&mut mesh);
    let cells = mesh.cells_for_track(TrackId::MAIN);
    assert!(!cells.is_empty(), "main track has at least one cell");
    assert!(cells.iter().any(|c| c.bundle_ref == "prologue"));
}
