//! Spec v3 §9.6 + §13.4 — runtime side of the class layer and the
//! cast / roster directives. Verifies the constructor call actually
//! changes the StatsInstance, and that the cast/recast/promote/demote
//! builtins push the right ledger envelopes and dotted world entries.

use loom_runtime::directives::{self, Registry};
use loom_runtime::{Event, Ledger, StatsInstance, StatsProfile, Value, World};
use loom_parser::ast::{AttributeDecl, StatsBody, StatExprDecl};

fn combat_profile() -> StatsProfile {
    StatsProfile::from_body(
        "Combat",
        &StatsBody {
            attributes: vec![AttributeDecl {
                name: "strength".into(),
                default: 10.0,
                min: 1.0,
                max: 30.0,
                span: Default::default(),
            }],
            axes: Vec::new(),
            pools: Vec::new(),
            stats: vec![StatExprDecl {
                name: "damage".into(),
                expression: "8 + strength * 0.5".into(),
                span: Default::default(),
            }],
            init: None,
            methods: Vec::new(),
        },
    )
}

#[test]
fn constructor_args_override_attribute_defaults() {
    let profile = combat_profile();
    let world = World::new();
    let baseline = StatsInstance::from_profile(&profile, &world);
    assert_eq!(baseline.attributes.get("strength").copied(), Some(10.0));

    let inst = StatsInstance::from_profile_with_args(
        &profile,
        [("strength", "12")],
        &world,
    );
    assert_eq!(inst.attributes.get("strength").copied(), Some(12.0));

    // Stat expressions derived from strength should pick up the override.
    let damage = inst
        .evaluate_stat("damage", &world)
        .expect("damage evaluates")
        .as_number()
        .unwrap_or_default();
    assert!((damage - 14.0).abs() < f64::EPSILON, "damage = {damage}");
}

fn dispatch_raw(
    raw: &str,
    registry: &Registry,
    world: &mut World,
    ledger: &mut Ledger,
) {
    let call = directives::parse(raw).expect("directive parses");
    let _ = directives::dispatch(&call, registry, world, ledger).expect("dispatch succeeds");
}

#[test]
fn cast_binds_role_and_player_in_world_and_ledger() {
    let registry = Registry::with_builtins();
    let mut world = World::new();
    let mut ledger = Ledger::default();
    dispatch_raw(
        "cast: person: jamie_lee, role: Wren",
        &registry,
        &mut world,
        &mut ledger,
    );
    match world.get("Wren.player") {
        Value::String(s) => assert_eq!(s, "jamie_lee"),
        other => panic!("Wren.player = {:?}", other),
    }
    match world.get("jamie_lee.role") {
        Value::String(s) => assert_eq!(s, "Wren"),
        other => panic!("jamie_lee.role = {:?}", other),
    }
    let envelope = ledger
        .events()
        .iter()
        .find_map(|e| match e {
            Event::CastBound { person, role } => Some((person.clone(), role.clone())),
            _ => None,
        })
        .expect("CastBound envelope");
    assert_eq!(envelope, ("jamie_lee".into(), "Wren".into()));
}

#[test]
fn recast_swaps_player_and_emits_swap_envelope() {
    let registry = Registry::with_builtins();
    let mut world = World::new();
    let mut ledger = Ledger::default();
    dispatch_raw(
        "cast: person: jamie_lee, role: Wren",
        &registry,
        &mut world,
        &mut ledger,
    );
    dispatch_raw(
        "recast: role: Wren, to: kim_ho",
        &registry,
        &mut world,
        &mut ledger,
    );
    match world.get("Wren.player") {
        Value::String(s) => assert_eq!(s, "kim_ho"),
        other => panic!("Wren.player after recast = {:?}", other),
    }
    let swap = ledger
        .events()
        .iter()
        .find_map(|e| match e {
            Event::CastSwapped {
                role,
                old_player,
                new_player,
            } => Some((role.clone(), old_player.clone(), new_player.clone())),
            _ => None,
        })
        .expect("CastSwapped envelope");
    assert_eq!(swap.0, "Wren");
    assert_eq!(swap.1, "jamie_lee");
    assert_eq!(swap.2, "kim_ho");
}

#[test]
fn promote_emits_both_bound_and_promoted_envelopes() {
    let registry = Registry::with_builtins();
    let mut world = World::new();
    let mut ledger = Ledger::default();
    dispatch_raw(
        "promote: person: walkup_12, role: Initiate",
        &registry,
        &mut world,
        &mut ledger,
    );
    let saw_bound = ledger
        .events()
        .iter()
        .any(|e| matches!(e, Event::CastBound { .. }));
    let saw_promoted = ledger
        .events()
        .iter()
        .any(|e| matches!(e, Event::RolePromoted { .. }));
    assert!(saw_bound && saw_promoted);
}

#[test]
fn load_roster_records_active_roster_and_fires_envelope() {
    let registry = Registry::with_builtins();
    let mut world = World::new();
    let mut ledger = Ledger::default();
    dispatch_raw(
        "load_roster: preview_night_05_28",
        &registry,
        &mut world,
        &mut ledger,
    );
    match world.get("Roster.active") {
        Value::String(s) => assert_eq!(s, "preview_night_05_28"),
        other => panic!("Roster.active = {:?}", other),
    }
    let saw = ledger
        .events()
        .iter()
        .any(|e| matches!(e, Event::RosterLoaded { roster } if roster == "preview_night_05_28"));
    assert!(saw, "RosterLoaded envelope missing");
}
