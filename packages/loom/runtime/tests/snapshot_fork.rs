//! Phase 1 of the Loom IDE redesign (docs/dev/loom-ide-redesign.md):
//! snapshot / restore / fork invariants on `Mesh` + `Playhead`.
//!
//! 1. `snapshot()` followed by further play and a `restore()` returns
//!    the mesh to byte-identical world + ledger state.
//! 2. A fork from a pre-choice snapshot lets the two heads diverge
//!    along different branches without contaminating each other's
//!    ledgers or worlds.

use std::sync::Arc;

use loom_runtime::{Bundle, Event, Mesh, Step};

const MAIN: &str = "\
# Fork
entry: opening

== opening
A bell rope swings.

WREN
  It hasn't rung in three days.

* Ring the bell.
  -> ringing
* Leave quietly.
  -> leaving
";

const RINGING: &str = "\
== ringing
The sound carries across the rocks.

WREN
  You rang it.

-> END
";

const LEAVING: &str = "\
== leaving
You slip away into the fog.

-> END
";

fn fresh_bundle() -> Arc<Bundle> {
    Arc::new(Bundle::from_sources([
        ("main.loom", MAIN),
        ("beats/ringing.loom", RINGING),
        ("beats/leaving.loom", LEAVING),
    ]))
}

fn fresh_mesh() -> Mesh {
    Mesh::new(fresh_bundle()).expect("mesh")
}

/// Step forward until we reach the first Choice / Awaiting / Ended.
fn drive_until_pause(mesh: &mut Mesh) -> Step {
    for _ in 0..128 {
        match mesh.step().expect("step") {
            (_, Step::Event(_)) => continue,
            (_, other) => return other,
        }
    }
    panic!("mesh never paused");
}

#[test]
fn restore_round_trips_world_and_ledger() {
    let mut mesh = fresh_mesh();
    let initial = drive_until_pause(&mut mesh);
    assert!(matches!(initial, Step::Choice(_)), "expect a choice");

    let snap = mesh.snapshot();
    let baseline_len = mesh.ledger().len();
    let baseline_events: Vec<Event> = mesh.ledger().events().to_vec();

    // Diverge: take a branch, run forward, then restore.
    mesh.choose(0).expect("choose ring");
    let _ = drive_until_pause(&mut mesh);
    assert!(mesh.ledger().len() > baseline_len, "branch added events");

    mesh.restore(&snap);
    assert_eq!(mesh.ledger().len(), baseline_len, "ledger truncated");
    assert_eq!(mesh.ledger().events(), baseline_events.as_slice(), "events identical");

    // After restore the playhead is sitting on the same choice — we
    // can take the *other* branch and play it through.
    mesh.choose(1).expect("choose leave");
    let post = drive_until_pause(&mut mesh);
    assert!(matches!(post, Step::Ended), "leaving branch ended");
}

#[test]
fn fork_diverges_independently() {
    let bundle = fresh_bundle();
    let mut main = Mesh::new(Arc::clone(&bundle)).expect("main mesh");
    let pause = drive_until_pause(&mut main);
    assert!(matches!(pause, Step::Choice(_)));

    // Snapshot at the choice, then fork a second head off the snapshot
    // (Mesh::fork() is the shortcut — it clones the live mesh).
    let mut alt = main.fork();

    // Two heads pick different branches.
    main.choose(0).expect("main chooses ring");
    let _ = drive_until_pause(&mut main);
    alt.choose(1).expect("alt chooses leave");
    let _ = drive_until_pause(&mut alt);

    let main_actions: Vec<String> = main
        .ledger()
        .events()
        .iter()
        .filter_map(|e| match e {
            Event::Action { text } => Some(text.clone()),
            _ => None,
        })
        .collect();
    let alt_actions: Vec<String> = alt
        .ledger()
        .events()
        .iter()
        .filter_map(|e| match e {
            Event::Action { text } => Some(text.clone()),
            _ => None,
        })
        .collect();

    assert!(
        main_actions.iter().any(|t| t.contains("sound carries")),
        "main head visited ringing beat"
    );
    assert!(
        alt_actions.iter().any(|t| t.contains("slip away")),
        "alt head visited leaving beat"
    );
    assert!(
        !main_actions.iter().any(|t| t.contains("slip away")),
        "main head did NOT visit leaving beat"
    );
    assert!(
        !alt_actions.iter().any(|t| t.contains("sound carries")),
        "alt head did NOT visit ringing beat"
    );
}

#[test]
fn mesh_fork_clones_live_state() {
    let mut a = fresh_mesh();
    let _ = drive_until_pause(&mut a);
    let len = a.ledger().len();
    let b = a.fork();
    assert_eq!(b.ledger().len(), len, "fork copies ledger");
    assert_eq!(
        b.ledger().events(),
        a.ledger().events(),
        "fork copies events"
    );
}
