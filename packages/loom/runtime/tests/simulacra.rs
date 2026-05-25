//! End-to-end Simulacra + Meridian integration (spec §10, §11).
//!
//! Loads the `wren-simulacra` example project from disk, walks the
//! playhead through the `earn` branch, and asserts that the
//! disposition mutation, stat publication, and goal lifecycle all
//! land as expected on the world snapshot the playhead exposes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use loom_runtime::{Bundle, Playhead, SetOp, Step, Value};

fn example_root() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest)
        .join("..")
        .join("examples")
        .join("wren-simulacra")
}

#[test]
fn wren_example_bundle_compiles_simulacra_and_stats() {
    let bundle = Bundle::load(example_root()).expect("load wren-simulacra");
    assert!(
        bundle.project_diagnostics.is_empty(),
        "project diagnostics: {:?}",
        bundle.project_diagnostics
    );
    assert!(bundle.characters.contains_key("Wren"));
    assert!(bundle.stats_profiles.contains_key("Combat"));
    assert!(bundle.trees.contains_key("WarriorPath"));

    let wren = bundle.characters.get("Wren").unwrap();
    assert_eq!(wren.disposition.len(), 1);
    assert!(wren.knowledge.contains_key("met_player"));
    let stats = wren.stats.as_ref().expect("Wren includes the Combat profile");
    assert_eq!(stats.attributes.get("strength"), Some(&10.0));
}

#[test]
fn earn_branch_routes_disposition_through_character_and_fires_hook() {
    let bundle = Arc::new(Bundle::load(example_root()).unwrap());
    let mut p = Playhead::new(bundle).unwrap();

    // 1. Opening dialogue.
    let step = p.step().unwrap();
    assert!(matches!(step, Step::Event(_)));

    // 2. Choice prompt.
    match p.step().unwrap() {
        Step::Choice(opts) => assert_eq!(opts.len(), 2),
        other => panic!("expected choice, got {other:?}"),
    }
    p.choose(0).unwrap(); // Earn trust.

    // 3. The `<set: Wren.trusts.Player += 60>` fires.
    let _ = p.step().unwrap();

    // World snapshot now reflects the mutation: 30 + 60 = 90.
    assert_eq!(
        p.world().get("Wren.trusts.Player"),
        Value::Number(90.0),
        "disposition should be republished after <set:>"
    );
    // The `<set:>` dispatch routes through `CharacterState`, which
    // refreshes its own `reacts` clauses; `trust > 60 -> warm` should
    // now be asserted.
    let wren = p.character("Wren").unwrap();
    assert!(wren.has_tag("warm"));

    // Stat surface: max_health = 50 + strength * 5 = 100.
    assert_eq!(p.world().get("Wren.health.max"), Value::Number(100.0));

    // Drain to end.
    loop {
        match p.step().unwrap() {
            Step::Ended => break,
            Step::Choice(_) => p.choose(0).unwrap(),
            _ => {}
        }
    }
}

#[test]
fn warrior_tree_unlock_respects_prereq() {
    let bundle = Bundle::load(example_root()).unwrap();
    let tree = bundle.trees.get("WarriorPath").unwrap();
    let mut unlocked: BTreeMap<String, bool> = BTreeMap::new();
    let world = loom_runtime::World::new();
    assert!(tree.can_unlock("armsman_1", &unlocked, &world));
    assert!(!tree.can_unlock("armsman_2", &unlocked, &world));
    unlocked.insert("armsman_1".into(), true);
    assert!(tree.can_unlock("armsman_2", &unlocked, &world));
}

#[test]
fn route_set_through_character_updates_world_and_state() {
    let bundle = Arc::new(Bundle::load(example_root()).unwrap());
    let mut p = Playhead::new(bundle).unwrap();
    let path: Vec<String> = vec!["Wren".into(), "trusts".into(), "Player".into()];
    let consumed = p.route_set_through_character(&path, &Value::Number(40.0), SetOp::Add);
    assert!(consumed, "character store should claim the path");
    assert_eq!(p.world().get("Wren.trusts.Player"), Value::Number(70.0));
}
