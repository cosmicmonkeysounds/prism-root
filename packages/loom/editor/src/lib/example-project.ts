// A self-contained, always-available example `.loom` project. When the
// user hasn't opened a local folder (or the open folder has no `.loom`
// files), local play falls back to this so "Start play" works with zero
// setup. It mirrors `packages/loom/examples/saltmere/main.loom` — the
// spec's tutorial story — exercising a CHARACTER, a reactive `let`, an
// `<if:>` arm, `<sfx>` / `<cue>` directives, choices, diverts, and a
// multi-speaker cue.

export const EXAMPLE_LABEL = "Saltmere (example)";
export const EXAMPLE_PATH = "saltmere.loom";

export const EXAMPLE_SOURCE = `# Saltmere
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
`;
