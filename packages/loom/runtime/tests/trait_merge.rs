//! TRAIT mixin merge — spec §9. A child CHARACTER's `is X, Y` clause
//! pulls property + disposition + knowledge + goal + generator + hook
//! defaults from each named TRAIT or parent CHARACTER, with the child's
//! own declarations winning on every slot it spells out.

use loom_runtime::{Bundle, ProjectDiagnostic};

#[test]
fn guard_inherits_humanoid_combatant_patrolling_talkative() {
    let src = "
TRAIT Humanoid
  voice: neutral

TRAIT Combatant
  hp: 50
  attack: 8

TRAIT Patrolling
  route: dock_loop

TRAIT Talkative
  chattiness: high

CHARACTER Guard is Humanoid, Combatant, Patrolling, Talkative

== opening
A bell rings.
";
    let bundle = Bundle::from_sources([("main.loom", src)]);
    let guard = bundle
        .characters
        .get("Guard")
        .expect("Guard should compile");
    // Inherited slots flow through.
    assert_eq!(
        guard.properties.get("voice").map(String::as_str),
        Some("neutral"),
        "voice from Humanoid"
    );
    assert_eq!(
        guard.properties.get("hp").map(String::as_str),
        Some("50"),
        "hp from Combatant"
    );
    assert_eq!(
        guard.properties.get("attack").map(String::as_str),
        Some("8"),
        "attack from Combatant"
    );
    assert_eq!(
        guard.properties.get("route").map(String::as_str),
        Some("dock_loop"),
        "route from Patrolling"
    );
    assert_eq!(
        guard.properties.get("chattiness").map(String::as_str),
        Some("high"),
        "chattiness from Talkative"
    );
    // Standalone TRAITs are not exposed as live characters.
    assert!(!bundle.characters.contains_key("Humanoid"));
    assert!(!bundle.characters.contains_key("Combatant"));
    // No ambiguous-slot complaints — every property came from exactly
    // one parent.
    assert!(
        bundle
            .project_diagnostics
            .iter()
            .all(|d| !matches!(d, ProjectDiagnostic::AmbiguousSlot { .. })),
        "unexpected diagnostics: {:?}",
        bundle.project_diagnostics
    );
}

#[test]
fn ambiguous_slot_between_two_parents_diagnoses() {
    let src = "
TRAIT Loud
  voice: bellow

TRAIT Quiet
  voice: whisper

CHARACTER Mixed is Loud, Quiet

== opening
Hush.
";
    let bundle = Bundle::from_sources([("main.loom", src)]);
    let ambiguous: Vec<_> = bundle
        .project_diagnostics
        .iter()
        .filter_map(|d| match d {
            ProjectDiagnostic::AmbiguousSlot { character, prop } => {
                Some((character.clone(), prop.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        ambiguous,
        vec![("Mixed".to_string(), "voice".to_string())],
        "expected one ambiguous-slot diagnostic for `voice`"
    );
}

#[test]
fn child_override_silences_ambiguity_and_wins() {
    let src = "
TRAIT Loud
  voice: bellow

TRAIT Quiet
  voice: whisper

CHARACTER Overrider is Loud, Quiet
  voice: speaks

== opening
Hush.
";
    let bundle = Bundle::from_sources([("main.loom", src)]);
    let overrider = bundle.characters.get("Overrider").unwrap();
    assert_eq!(
        overrider.properties.get("voice").map(String::as_str),
        Some("speaks"),
        "child override wins"
    );
    // Child override resolves the conflict — no diagnostic should fire.
    assert!(
        bundle
            .project_diagnostics
            .iter()
            .all(|d| !matches!(d, ProjectDiagnostic::AmbiguousSlot { .. })),
        "child override should suppress the AmbiguousSlot diagnostic"
    );
}
