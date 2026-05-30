//! Spec v3 §9.6 + §13 — class layer (init / method / constructor
//! calls) and the unified ROLE / PERSON / ROSTER / cast model. These
//! tests assert the parser surface; runtime semantics live in
//! `runtime/tests/cast_and_class.rs`.

use loom_parser::ast::{DeclarationKind, Item, RosterAssignment};
use loom_parser::parse;

#[test]
fn role_parses_as_character_alias_with_constructor_call() {
    let src = "\
STATS Combat
  attribute strength = 10
  init(strength)
    self.strength = strength
  method swing(target)
    return self.damage

ROLE Wren is Keeper
  stats: Combat(strength: 12)
  voice: female_alto
";
    let (file, diagnostics) = parse(src);
    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    let wren = file
        .items
        .iter()
        .find_map(|item| match item {
            Item::Declaration(d) if d.kind == DeclarationKind::Role => Some(d),
            _ => None,
        })
        .expect("ROLE Wren");
    let body = wren.character.as_ref().expect("ROLE body lowers via character");
    assert_eq!(body.stats_profile.as_deref(), Some("Combat"));
    let ctor = body.stats_ctor.as_ref().expect("structured constructor call");
    assert_eq!(ctor.class, "Combat");
    assert_eq!(ctor.args.get("strength").map(String::as_str), Some("12"));

    let stats_decl = file
        .items
        .iter()
        .find_map(|item| match item {
            Item::Declaration(d) if d.kind == DeclarationKind::Stats => Some(d),
            _ => None,
        })
        .expect("STATS Combat");
    let stats_body = stats_decl.stats.as_ref().expect("stats body");
    let init = stats_body.init.as_ref().expect("init body");
    assert_eq!(init.params.len(), 1);
    assert_eq!(init.params[0].name, "strength");
    assert_eq!(stats_body.methods.len(), 1);
    assert_eq!(stats_body.methods[0].name, "swing");
    assert_eq!(stats_body.methods[0].params.len(), 1);
}

#[test]
fn indented_stats_block_is_constructor_sugar() {
    let src = "\
CHARACTER Wren
  stats: Combat
    strength: 12
    agility: 14
";
    let (file, diagnostics) = parse(src);
    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    let wren = file
        .items
        .iter()
        .find_map(|item| match item {
            Item::Declaration(d) if d.kind == DeclarationKind::Character => Some(d),
            _ => None,
        })
        .expect("CHARACTER Wren");
    let body = wren.character.as_ref().unwrap();
    let ctor = body.stats_ctor.as_ref().expect("indented block lifts ctor");
    assert_eq!(ctor.class, "Combat");
    assert_eq!(ctor.args.get("strength").map(String::as_str), Some("12"));
    assert_eq!(ctor.args.get("agility").map(String::as_str), Some("14"));
}

#[test]
fn person_body_captures_personal_fields() {
    let src = "\
PERSON jamie_lee
  display_name: Jamie Lee
  pronouns: they/them
  device: jamie_phone_01
  content_tolerance: [no_strobe, no_loud_bells]
  accessibility: [step_free]
  notes: Veteran swing.
";
    let (file, diagnostics) = parse(src);
    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    let person = file
        .items
        .iter()
        .find_map(|item| match item {
            Item::Declaration(d) if d.kind == DeclarationKind::Person => d.person.as_ref(),
            _ => None,
        })
        .expect("PERSON body");
    assert_eq!(person.display_name.as_deref(), Some("Jamie Lee"));
    assert_eq!(person.pronouns.as_deref(), Some("they/them"));
    assert_eq!(person.device.as_deref(), Some("jamie_phone_01"));
    assert_eq!(person.content_tolerance, vec!["no_strobe", "no_loud_bells"]);
    assert_eq!(person.accessibility, vec!["step_free"]);
}

#[test]
fn roster_lifts_cast_swings_cohorts_locations() {
    let src = "\
ROSTER preview_night_05_28
  date: 2026-05-28T19:30
  capacity: 24

  cast
    Wren        := jamie_lee
    Bellkeeper  := raja_park
    Initiate    := any of [audience]
    Singer      := any of [pat_eberle, kim_ho, audience]

  swings
    Wren        := [raja_park, kim_ho]
    Bellkeeper  := [jamie_lee]

  cohorts
    Initiates   start with: [audience]

  locations
    BellTower   start with: [jamie_lee]
";
    let (file, diagnostics) = parse(src);
    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    let roster = file
        .items
        .iter()
        .find_map(|item| match item {
            Item::Declaration(d) if d.kind == DeclarationKind::Roster => d.roster.as_ref(),
            _ => None,
        })
        .expect("ROSTER body");
    assert_eq!(roster.date.as_deref(), Some("2026-05-28T19:30"));
    assert_eq!(roster.capacity, Some(24));

    let cast_lookup = |role: &str| {
        roster
            .cast
            .iter()
            .find(|c| c.role == role)
            .map(|c| c.assignment.clone())
    };
    match cast_lookup("Wren").unwrap() {
        RosterAssignment::Person(p) => assert_eq!(p, "jamie_lee"),
        other => panic!("Wren cast: {:?}", other),
    }
    match cast_lookup("Initiate").unwrap() {
        RosterAssignment::AnyOf(pool) => assert_eq!(pool, vec!["audience"]),
        other => panic!("Initiate cast: {:?}", other),
    }
    match cast_lookup("Singer").unwrap() {
        RosterAssignment::AnyOf(pool) => {
            assert_eq!(pool, vec!["pat_eberle", "kim_ho", "audience"])
        }
        other => panic!("Singer cast: {:?}", other),
    }

    let wren_swing = roster
        .swings
        .iter()
        .find(|s| s.role == "Wren")
        .expect("Wren swing");
    assert_eq!(wren_swing.fallbacks, vec!["raja_park", "kim_ho"]);

    let initiates_cohort = roster
        .cohorts
        .iter()
        .find(|c| c.cohort == "Initiates")
        .expect("Initiates cohort");
    assert_eq!(initiates_cohort.start_with, vec!["audience"]);

    let belltower = roster
        .locations
        .iter()
        .find(|l| l.location == "BellTower")
        .expect("BellTower start-with");
    assert_eq!(belltower.start_with, vec!["jamie_lee"]);
}
