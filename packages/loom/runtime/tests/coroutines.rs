//! Runtime integration tests for SCENE / GENERATOR coroutines and
//! the tiered scheduler (spec §12.3, §12.4, §12.5).

use std::time::Instant;

use loom_runtime::coroutine::{Coroutine, CoroutineStatus, Tier};
use loom_runtime::ledger::{Event, Ledger};
use loom_runtime::scheduler::Scheduler;
use loom_runtime::{Bundle, World};

#[test]
fn bundle_indexes_top_level_scene_and_generator() {
    let bundle = Bundle::from_sources([(
        "main.loom",
        "\
SCENE investigate(character)
  approach
    -> examine

  examine
    return clue

GENERATOR Ambient
  tier: ambient

  loop
    yield bark from one | two

== opening

Hello.
",
    )]);
    assert!(
        bundle.scenes.contains_key("investigate"),
        "scenes: {:?}",
        bundle.scenes.keys().collect::<Vec<_>>()
    );
    assert!(bundle.generators.contains_key("Ambient"));
    let prog = bundle
        .scene_programs
        .get("investigate")
        .expect("scene lowered");
    assert!(prog.labels.contains_key("approach"));
    assert!(prog.labels.contains_key("examine"));
}

#[test]
fn scene_transitions_through_states_and_returns() {
    let bundle = Bundle::from_sources([(
        "main.loom",
        "\
SCENE investigate(character)
  approach
    -> examine

  examine
    -> confront

  confront
    return clue

== opening

Hello.
",
    )]);
    let program = bundle.scene_programs["investigate"].clone();
    let mut co = Coroutine::new(1, program, Tier::Focal, 1.0);
    let mut world = World::new();
    let mut ledger = Ledger::default();
    let mut value = None;
    for _ in 0..50 {
        match co.step(&mut world, &mut ledger) {
            CoroutineStatus::Returned { value: v } => {
                value = v;
                break;
            }
            CoroutineStatus::Waiting { .. } => panic!("unexpected wait"),
            _ => {}
        }
    }
    assert_eq!(value.map(|v| v.display()), Some("clue".into()));
    let advances: Vec<_> = ledger
        .events()
        .iter()
        .filter_map(|e| match e {
            Event::SceneAdvanced { state, .. } => Some(state.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(advances, vec!["examine", "confront"]);
}

#[test]
fn ambient_generator_yields_under_scheduler() {
    let bundle = Bundle::from_sources([(
        "main.loom",
        "\
GENERATOR HarborChorus
  tier: ambient

  yield bark from Quiet night. | Stars are out. | Tide's calm.

== opening

Hi.
",
    )]);
    let program = bundle.generator_programs["HarborChorus"].clone();
    let mut sched = Scheduler::new();
    let mut world = World::new();
    let mut ledger = Ledger::default();
    sched.spawn(Coroutine::new(1, program, Tier::Ambient, 0.3), &mut ledger);
    sched.tick(Instant::now(), &mut world, &mut ledger);
    assert!(
        ledger.events().iter().any(|e| matches!(
            e,
            Event::GeneratorYielded { generator, .. } if generator == "HarborChorus"
        )),
        "no yield: {:?}",
        ledger.events()
    );
}

#[test]
fn focal_scene_always_ticks_even_with_ambient_noise() {
    let bundle = Bundle::from_sources([(
        "main.loom",
        "\
SCENE entry()
  return done

GENERATOR Noise
  tier: ambient

  loop
    yield bark from a | b

== opening

Hi.
",
    )]);
    let mut sched = Scheduler::new();
    let mut world = World::new();
    let mut ledger = Ledger::default();
    // Several ambient noise generators.
    for i in 0..6 {
        sched.spawn(
            Coroutine::new(
                100 + i,
                bundle.generator_programs["Noise"].clone(),
                Tier::Ambient,
                0.1,
            ),
            &mut ledger,
        );
    }
    sched.spawn(
        Coroutine::new(1, bundle.scene_programs["entry"].clone(), Tier::Focal, 1.0),
        &mut ledger,
    );
    sched.tick(Instant::now(), &mut world, &mut ledger);
    assert!(
        ledger.events().iter().any(|e| matches!(
            e,
            Event::SceneCompleted { scene, .. } if scene == "entry"
        )),
        "focal coroutine did not return: {:?}",
        ledger.events()
    );
}

#[test]
fn guard_patrol_example_compiles_and_lowers() {
    let main = include_str!("../../examples/guard-patrol/main.loom");
    let bundle = Bundle::from_sources([("main.loom", main)]);
    assert!(
        bundle.project_diagnostics.is_empty(),
        "{:?}",
        bundle.project_diagnostics
    );
    assert!(bundle.scenes.contains_key("patrol"));
    assert!(bundle.scenes.contains_key("investigate"));
    assert!(bundle.generators.contains_key("HarborChorus"));
    let investigate = &bundle.scene_programs["investigate"];
    assert!(investigate.labels.contains_key("approach"));
    assert!(investigate.labels.contains_key("confront"));
}
