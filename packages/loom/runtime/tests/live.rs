//! End-to-end Live-performance integration (spec §13).
//!
//! Loads the `bell-tower-live` example project from disk and checks
//! that COHORT / LOCATION declarations index into the bundle and the
//! [`LiveStage`] mediates broadcast / enroll / improv correctly.

use std::path::PathBuf;
use std::time::Instant;

use loom_parser::ast::{AdvanceSignal, ImprovDirective, ImprovDuration, ImprovDurationUnit, QuorumOp};
use loom_runtime::{
    parse_broadcast_scope, Bundle, ImprovOutcome, Ledger, LiveStage,
};

fn example_root() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest)
        .join("..")
        .join("examples")
        .join("bell-tower-live")
}

#[test]
fn bell_tower_indexes_cohorts_and_locations() {
    let bundle = Bundle::load(example_root()).expect("loads bell-tower-live");
    let parser_errs: Vec<_> = bundle
        .parser_diagnostics()
        .map(|(_, d)| d.clone())
        .collect();
    assert!(parser_errs.is_empty(), "{parser_errs:#?}");
    assert!(bundle.cohorts.contains_key("Initiates"));
    assert!(bundle.cohorts.contains_key("Singers"));
    assert_eq!(bundle.cohorts["Initiates"].capacity, Some(24));
    assert!(bundle.locations.contains_key("BellTower"));
    assert_eq!(
        bundle.locations["BellTower"].ambient.as_deref(),
        Some("bell-loop")
    );
}

#[test]
fn bell_tower_drives_a_live_stage_through_broadcast() {
    let bundle = Bundle::load(example_root()).expect("loads bell-tower-live");
    let mut stage = LiveStage::new();
    stage.seed_from_bundle(&bundle);
    let mut ledger = Ledger::default();
    for id in ["alice", "bob", "carol"] {
        stage.participant_joins(id, &mut ledger);
    }
    stage.enroll("alice", "Singers", &mut ledger).unwrap();
    stage.enroll("bob", "Singers", &mut ledger).unwrap();
    stage.participant_enters("alice", "BellTower", &mut ledger).unwrap();
    stage.participant_enters("carol", "BellTower", &mut ledger).unwrap();
    let scope =
        parse_broadcast_scope("cohort(Singers) and location(BellTower)").unwrap();
    let hits = scope.evaluate(&stage);
    assert_eq!(hits.len(), 1);
    assert!(hits.contains("alice"));
}

#[test]
fn improv_directive_resolves_on_pedal() {
    let dir = ImprovDirective {
        duration: Some(ImprovDuration {
            value: 45.0,
            unit: ImprovDurationUnit::Seconds,
        }),
        quorum: QuorumOp::Any,
        advance_on: vec![
            AdvanceSignal::Pedal,
            AdvanceSignal::Speech {
                anchor: "let us begin".into(),
            },
        ],
        span: loom_parser::source::Span::default(),
    };
    let mut stage = LiveStage::new();
    let id = stage.start_improv(&dir, Instant::now());
    let out = stage.improv.submit_signal(id, AdvanceSignal::Pedal);
    assert_eq!(out, ImprovOutcome::Advanced);
}
