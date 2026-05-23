//! End-to-end runtime test — drives a small Harbor scene through
//! both branches and asserts the ledger state at the end.
//!
//! Pulls together everything Phase 1 ships: parse → validate →
//! compile → boot → walk frames → resolve choices → fall through.

use prism_loom_runtime::{Frame, LedgerEntry, Show};

/// Source built to exercise both branches cleanly without source-
/// order fall-through surprises: every branch terminates with an
/// explicit `-> ending` so the playhead can't drift into the other
/// branch's section.
const HARBOR_SOURCE: &str = "# harbor_greeting \"The Harbor\"

cast WREN
  .label \"Wren the Fisher\"
  .voice female_mezzo

-- start

WREN
  The bell went silent three days ago.

  * help me
    -> investigate
  * leave
    WREN
      Suit yourself.
    -> ending

-- investigate

WREN
  Maren is gone. Thanks for staying.

-> ending

-- ending
";

fn collect_dialogue_lines(frames: &[Frame]) -> Vec<String> {
    frames
        .iter()
        .flat_map(|f| match f {
            Frame::Dialogue { lines, .. } => lines.clone(),
            _ => Vec::new(),
        })
        .collect()
}

fn drive_to_end(show: &mut Show) {
    // Step until the playhead reports End; defensive cap protects
    // against a regression that would otherwise hang the test.
    for _ in 0..200 {
        if show.step().is_none() {
            break;
        }
    }
}

#[test]
fn help_branch_lands_in_investigate_and_ends_cleanly() {
    let mut show = Show::load(HARBOR_SOURCE).expect("show should load");

    let pre_choice = show.play_until_park();
    assert!(matches!(pre_choice.last(), Some(Frame::Choices(_))));
    assert!(show.is_awaiting_choice());

    show.choose(0);
    let post_choice = show.play_until_park();
    let lines = collect_dialogue_lines(&post_choice);
    assert!(
        lines.iter().any(|l| l.contains("Maren")),
        "expected Maren line in {lines:?}"
    );

    // Drive the rest of the show.
    drive_to_end(&mut show);
    assert!(show.is_at_end());

    assert!(show.ledger().played("start"));
    assert!(show.ledger().played("investigate"));
    assert!(show.ledger().played("ending"));
}

#[test]
fn leave_branch_renders_inline_body_and_ends() {
    let mut show = Show::load(HARBOR_SOURCE).expect("show should load");
    let _ = show.play_until_park();
    show.choose(1);
    let post_choice = show.play_until_park();
    let lines = collect_dialogue_lines(&post_choice);
    assert!(
        lines.iter().any(|l| l.contains("Suit yourself")),
        "expected leave-branch dialogue in {lines:?}"
    );
    drive_to_end(&mut show);
    assert!(show.is_at_end());
    assert!(show.ledger().played("start"));
    assert!(show.ledger().played("ending"));
    // The help-branch section should NOT have been visited.
    assert!(!show.ledger().played("investigate"));
}

#[test]
fn ledger_records_visited_chose_in_order() {
    let mut show = Show::load(HARBOR_SOURCE).expect("show should load");
    let _ = show.play_until_park();
    show.choose(0);
    drive_to_end(&mut show);

    let entries = show.ledger().entries();
    // Help-branch expected order:
    //   Visited(start), Chose(start, "help me"), Visited(investigate),
    //   Visited(ending).
    let visit_path: Vec<&str> = entries
        .iter()
        .filter_map(|e| match e {
            LedgerEntry::Visited { section, .. } => Some(section.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(visit_path, vec!["start", "investigate", "ending"]);

    let chose_count = entries
        .iter()
        .filter(|e| matches!(e, LedgerEntry::Chose { .. }))
        .count();
    assert_eq!(chose_count, 1);
}

#[test]
fn bundle_round_trips_through_postcard() {
    // The runtime's bundle format is the design's "postcard wire" —
    // load → compile → encode → decode produces an equivalent bundle.
    let show = Show::load(HARBOR_SOURCE).expect("show should load");
    let bytes = postcard::to_allocvec(show.bundle()).expect("encode");
    let decoded: prism_loom_runtime::LoomDatabase = postcard::from_bytes(&bytes).expect("decode");
    assert_eq!(decoded.documents.len(), 1);
    assert_eq!(decoded.documents[0].id, "harbor_greeting");
    assert_eq!(decoded.documents[0].sections.len(), 3);
}
