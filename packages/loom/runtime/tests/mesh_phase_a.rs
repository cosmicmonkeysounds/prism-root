//! Phase-A invariants for the Mesh substrate
//! (loom-editor.html §3, §9, §11.1):
//!
//! 1. Every ledger envelope has metadata: the implicit main track id
//!    and a cause-edge chained to the previous envelope on that track.
//! 2. Every beat-visit is bracketed by `CellEntered` / `CellExited`.
//! 3. `Mesh::cells_for_track` folds the ledger into a monotonic list
//!    of `Cell`s matching the beat sequence the playhead walked.
//! 4. The single Scripted track on `TrackId::MAIN` is registered.

use std::sync::Arc;

use loom_runtime::{
    Bundle, CellKind, Driver, Event, Mesh, Step, TrackId, TrackIdentity,
};

const MAIN: &str = "\
# Phase A
entry: opening

== opening
A bell rope swings.

WREN
  It hasn't rung in three days.

-> ringing
";

const RINGING: &str = "\
== ringing
The sound carries.

WREN
  You... rang it.

-> END
";

fn play_to_end(mesh: &mut Mesh) {
    for _ in 0..64 {
        match mesh.step().expect("step") {
            (_, Step::Ended) => return,
            _ => {}
        }
    }
    panic!("mesh did not halt within 64 steps");
}

#[test]
fn mesh_has_main_track_registered() {
    let bundle = Arc::new(Bundle::from_sources([
        ("main.loom", MAIN),
        ("beats/ringing.loom", RINGING),
    ]));
    let mesh = Mesh::new(bundle).expect("mesh");
    let main = mesh.track(TrackId::MAIN).expect("MAIN track");
    assert!(matches!(main.identity, TrackIdentity::Main));
    assert!(matches!(main.driver, Driver::Scripted));
    // Phase B: the Mesh seeds extra rows from the bundle (Booth +
    // characters). MAIN is one of them, not the only one.
    assert!(mesh
        .tracks()
        .any(|t| matches!(t.identity, TrackIdentity::Booth)));
}

#[test]
fn every_envelope_has_metadata_on_main_track() {
    let bundle = Arc::new(Bundle::from_sources([
        ("main.loom", MAIN),
        ("beats/ringing.loom", RINGING),
    ]));
    let mut mesh = Mesh::new(bundle).expect("mesh");
    play_to_end(&mut mesh);

    let events = mesh.ledger().events();
    let meta = mesh.ledger().meta();
    assert_eq!(events.len(), meta.len(), "events and meta length match");
    assert!(!events.is_empty(), "playhead produced events");

    // Track id is MAIN for every envelope in Phase A.
    for m in meta {
        assert_eq!(m.track, TrackId::MAIN);
    }
    // The first envelope on a track is a root (no cause); every later
    // envelope chains backward.
    assert_eq!(meta[0].cause, None);
    for (i, m) in meta.iter().enumerate().skip(1) {
        let cause = m.cause.expect("non-root envelope has cause");
        assert!((cause as usize) < i, "cause must precede envelope");
    }
}

#[test]
fn beat_visits_are_bracketed_by_cell_envelopes() {
    let bundle = Arc::new(Bundle::from_sources([
        ("main.loom", MAIN),
        ("beats/ringing.loom", RINGING),
    ]));
    let mut mesh = Mesh::new(bundle).expect("mesh");
    play_to_end(&mut mesh);

    let entered = mesh
        .ledger()
        .events()
        .iter()
        .filter(|e| {
            matches!(
                e,
                Event::CellEntered {
                    kind: CellKind::BeatVisit,
                    ..
                }
            )
        })
        .count();
    let exited = mesh
        .ledger()
        .events()
        .iter()
        .filter(|e| matches!(e, Event::CellExited { .. }))
        .count();
    assert!(entered >= 2, "at least opening + ringing beats entered");
    assert_eq!(entered, exited, "every cell opens and closes");
}

#[test]
fn cells_for_track_folds_into_monotonic_cell_sequence() {
    let bundle = Arc::new(Bundle::from_sources([
        ("main.loom", MAIN),
        ("beats/ringing.loom", RINGING),
    ]));
    let mut mesh = Mesh::new(bundle).expect("mesh");
    play_to_end(&mut mesh);

    let cells = mesh.cells_for_track(TrackId::MAIN);
    assert!(!cells.is_empty());
    // Monotonic by `start` — cells may nest (a divert opens a child
    // cell on top of its parent), but their entry points always move
    // forward in ledger order.
    let mut prev_start = 0;
    for cell in &cells {
        assert!(cell.start >= prev_start, "cell starts grow forward");
        prev_start = cell.start;
    }
    // Every cell is closed once the show ended.
    let open: Vec<_> = cells.iter().filter(|c| c.is_open()).collect();
    assert!(
        open.is_empty(),
        "open cells remain: {:#?}\n\n--- envelopes ---\n{:#?}",
        open,
        mesh.ledger().events()
    );
    // Bundle refs include the beats we walked through.
    let refs: Vec<&str> = cells.iter().map(|c| c.bundle_ref.as_str()).collect();
    assert!(refs.contains(&"opening"));
    assert!(refs.contains(&"ringing"));
}

#[test]
fn register_track_allocates_new_id_without_disturbing_main() {
    let bundle = Arc::new(Bundle::from_sources([("main.loom", MAIN)]));
    let mut mesh = Mesh::new(bundle).expect("mesh");
    let new = mesh.register_track(
        TrackIdentity::Person("jamie_lee".into()),
        Driver::Participant {
            person: "jamie_lee".into(),
        },
    );
    assert_ne!(new, TrackId::MAIN);
    assert_eq!(mesh.track(new).unwrap().identity.label(), "jamie_lee");
    assert!(matches!(mesh.track(TrackId::MAIN).unwrap().identity, TrackIdentity::Main));
}
