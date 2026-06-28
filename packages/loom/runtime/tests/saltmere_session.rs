//! Repro: play the bundled Saltmere example (mirrors the editor's
//! `lib/example-project.ts`) through the same `PlaySession` the wasm
//! engine drives, to localise a "Start play" crash seen in the browser.

use loom_runtime::session::PlaySession;

const SALTMERE: &str = "# Saltmere
entry: opening
tags: tutorial, lighthouse

CHARACTER Wren is Keeper, Combatant
  voice: female_alto
  hp: 80

let trusted = Wren.trusts.Player > 50

== opening
  cast: Wren, Player
  setting: Lighthouse

INT. LIGHTHOUSE - DAWN

A bell rope swings in the gloom.

WREN
  (quietly)
  It hasn't rung in three days.

<if: trusted>
  WREN
    I knew you'd come.

<sfx: distant_thunder>

* Ring the bell.
  -> ringing
* Leave quietly.[ but you wonder.]
  -> END

== ringing
  cast: Wren, Player

The sound carries across the rocks.

<cue: rope_creak>

WREN | FISHER
  (stunned)
  You... rang it.

-> END
";

#[test]
fn saltmere_example_plays_and_routes_dialogue() {
    let mut session = PlaySession::new(
        "saltmere".into(),
        vec![("main.loom".into(), SALTMERE.into())],
        "main.loom".into(),
    )
    .expect("session builds + advances to first pause");

    // Take the first choice ("Ring the bell." → ringing) and play out.
    session.choose(session.primary().to_string().as_str(), 0).expect("choose 0");

    let view = session.snapshot_view();
    let head = view.heads.iter().find(|h| h.id == view.primary).expect("primary head");
    // The multi-speaker WREN | FISHER line must ride Wren's track row.
    let wren = head.tracks.iter().find(|t| t.label == "Wren").expect("Wren track");
    let dialogue_on_wren = head
        .transcript
        .iter()
        .zip(head.meta.iter())
        .any(|(e, m)| format!("{e:?}").contains("rang it") && m.track.0 == wren.id);
    assert!(dialogue_on_wren, "WREN | FISHER line should land on the Wren track");
}
