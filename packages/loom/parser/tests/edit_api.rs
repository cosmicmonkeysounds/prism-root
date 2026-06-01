//! Tests for the span-preserving structural edit API
//! (`loom_parser::edit`). The contract: structural edits rewrite the
//! source as minimal byte-range splices, leaving every untouched line
//! byte-identical (so collaborative Loro merges stay clean).

use loom_parser::{
    apply_edits, insert_beat, move_beat, move_body_item, parse, remove_beat, set_beat_property,
    Anchor, Item,
};

const SAMPLE: &str = r#"# Saltmere
entry: opening

CHARACTER Wren is Keeper
  hp: 80

== opening
  cast: Wren, Player
  setting: Lighthouse

A bell rope swings. // first beat comment

WREN
  It hasn't rung.

-> ringing

== ringing
  cast: Wren, Player

The sound carries.

-> END
"#;

fn beat_names(source: &str) -> Vec<String> {
    let (file, _) = parse(source);
    file.items
        .iter()
        .filter_map(|it| match it {
            Item::Beat(b) => Some(b.name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn set_existing_property_touches_only_that_line() {
    let (file, _) = parse(SAMPLE);
    let edits = set_beat_property(SAMPLE, &file, "opening", "setting", "Cliffside").unwrap();
    let out = apply_edits(SAMPLE, &edits).unwrap();

    // The single strongest minimal-diff guarantee: the result is the
    // original with exactly one line changed.
    assert_eq!(
        out,
        SAMPLE.replace("  setting: Lighthouse", "  setting: Cliffside")
    );
    // Re-parse confirms the new value took.
    let (file2, diags) = parse(&out);
    assert_eq!(diags.len(), parse(SAMPLE).1.len());
    let opening = file2
        .items
        .iter()
        .find_map(|it| match it {
            Item::Beat(b) if b.name == "opening" => Some(b),
            _ => None,
        })
        .unwrap();
    assert_eq!(opening.contract.get("setting").unwrap().value, "Cliffside");
}

#[test]
fn set_same_value_is_a_noop() {
    let (file, _) = parse(SAMPLE);
    let edits = set_beat_property(SAMPLE, &file, "opening", "setting", "Lighthouse").unwrap();
    assert!(edits.is_empty());
    assert_eq!(apply_edits(SAMPLE, &edits).unwrap(), SAMPLE);
}

#[test]
fn set_missing_property_inserts_a_contract_line() {
    let (file, _) = parse(SAMPLE);
    // `ringing` has only a `cast:` line — `setting:` must be inserted.
    let edits = set_beat_property(SAMPLE, &file, "ringing", "setting", "Shore").unwrap();
    let out = apply_edits(SAMPLE, &edits).unwrap();

    // Inserted directly under the existing cast line, same indent.
    assert!(out.contains("== ringing\n  cast: Wren, Player\n  setting: Shore\n"));
    // Untouched regions survive, including the trailing comment.
    assert!(out.contains("// first beat comment"));
    assert!(out.contains("  setting: Lighthouse"));

    let (file2, diags) = parse(&out);
    assert_eq!(diags.len(), parse(SAMPLE).1.len());
    let ringing = file2
        .items
        .iter()
        .find_map(|it| match it {
            Item::Beat(b) if b.name == "ringing" => Some(b),
            _ => None,
        })
        .unwrap();
    assert_eq!(ringing.contract.get("setting").unwrap().value, "Shore");
}

#[test]
fn move_beat_to_end_reorders_and_preserves_untouched() {
    assert_eq!(beat_names(SAMPLE), vec!["opening", "ringing"]);

    let (file, _) = parse(SAMPLE);
    let edits = move_beat(SAMPLE, &file, "opening", Anchor::After("ringing".to_string())).unwrap();
    let out = apply_edits(SAMPLE, &edits).unwrap();

    // Order flipped.
    assert_eq!(beat_names(&out), vec!["ringing", "opening"]);
    // No new parse diagnostics introduced by the move.
    assert_eq!(parse(&out).1.len(), parse(SAMPLE).1.len());

    // The character declaration is byte-identical.
    assert!(out.contains("CHARACTER Wren is Keeper\n  hp: 80"));
    // The (unmoved) `ringing` body is byte-identical.
    assert!(out.contains("== ringing\n  cast: Wren, Player\n\nThe sound carries.\n\n-> END"));
    // The moved beat carried its trailing comment along.
    assert!(out.contains("// first beat comment"));
    // ringing now precedes opening in the text.
    assert!(out.find("The sound carries.").unwrap() < out.find("A bell rope swings.").unwrap());
}

#[test]
fn move_beat_before_matches_after_anchor() {
    let (file, _) = parse(SAMPLE);
    let edits = move_beat(SAMPLE, &file, "ringing", Anchor::Before("opening".to_string())).unwrap();
    let out = apply_edits(SAMPLE, &edits).unwrap();
    assert_eq!(beat_names(&out), vec!["ringing", "opening"]);
    assert_eq!(parse(&out).1.len(), parse(SAMPLE).1.len());
}

#[test]
fn move_to_current_position_is_a_noop() {
    let (file, _) = parse(SAMPLE);
    // `opening` is already immediately before `ringing`.
    let edits = move_beat(SAMPLE, &file, "opening", Anchor::Before("ringing".to_string())).unwrap();
    assert!(edits.is_empty());
    assert_eq!(apply_edits(SAMPLE, &edits).unwrap(), SAMPLE);
}

#[test]
fn unknown_beat_is_an_error() {
    let (file, _) = parse(SAMPLE);
    assert!(set_beat_property(SAMPLE, &file, "nope", "cast", "X").is_err());
    assert!(move_beat(SAMPLE, &file, "nope", Anchor::End).is_err());
    assert!(move_beat(SAMPLE, &file, "opening", Anchor::Before("nope".to_string())).is_err());
}

#[test]
fn empty_edits_roundtrip_identity() {
    assert_eq!(apply_edits(SAMPLE, &[]).unwrap(), SAMPLE);
}

#[test]
fn insert_beat_at_end_appends() {
    let (file, _) = parse(SAMPLE);
    let edits = insert_beat(SAMPLE, &file, "interlude", Anchor::End).unwrap();
    let out = apply_edits(SAMPLE, &edits).unwrap();
    assert_eq!(beat_names(&out), vec!["opening", "ringing", "interlude"]);
    assert_eq!(parse(&out).1.len(), parse(SAMPLE).1.len());
    // The pre-existing beats are byte-identical.
    assert!(out.contains("== ringing\n  cast: Wren, Player\n\nThe sound carries.\n\n-> END"));
    assert!(out.contains("== interlude"));
}

#[test]
fn insert_beat_before_anchor() {
    let (file, _) = parse(SAMPLE);
    let edits =
        insert_beat(SAMPLE, &file, "prologue", Anchor::Before("opening".to_string())).unwrap();
    let out = apply_edits(SAMPLE, &edits).unwrap();
    assert_eq!(beat_names(&out), vec!["prologue", "opening", "ringing"]);
}

#[test]
fn remove_beat_deletes_block() {
    let (file, _) = parse(SAMPLE);
    let edits = remove_beat(SAMPLE, &file, "opening").unwrap();
    let out = apply_edits(SAMPLE, &edits).unwrap();
    assert_eq!(beat_names(&out), vec!["ringing"]);
    // opening's body (and its trailing comment) is gone; ringing + the
    // character declaration are byte-identical.
    assert!(!out.contains("first beat comment"));
    assert!(out.contains("== ringing\n  cast: Wren, Player"));
    assert!(out.contains("CHARACTER Wren is Keeper\n  hp: 80"));
    assert_eq!(parse(&out).1.len(), parse(SAMPLE).1.len());
}

#[test]
fn move_body_item_reorders_within_beat() {
    let (file, _) = parse(SAMPLE);
    // opening.body = [Action, Dialogue, Divert]; move the Action (0) to
    // the end (2) — it should land after the `-> ringing` divert.
    let edits = move_body_item(SAMPLE, &file, "opening", 0, 2).unwrap();
    let out = apply_edits(SAMPLE, &edits).unwrap();
    assert_eq!(parse(&out).1.len(), parse(SAMPLE).1.len());
    assert_eq!(beat_names(&out), vec!["opening", "ringing"]);
    // The action (with its comment) survived and now follows the divert.
    assert!(out.contains("first beat comment"));
    assert!(out.find("-> ringing").unwrap() < out.find("A bell rope swings.").unwrap());
}
