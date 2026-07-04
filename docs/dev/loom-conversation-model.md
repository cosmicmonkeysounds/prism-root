# Loom — the unified conversation model

*2026-07-02. Status: implemented in the TS stack (`core` + `editor` + `play` mirrors).*

## The problem

Loom grew two parallel conversation systems that never met:

1. **Story output.** Beats and hooks emit `dialogue` (a speaker's line) and
   `action` (speakerless narration) sim events. `composeGuestMessages` routed
   `dialogue` into a per-speaker DM thread — and **dropped `action` entirely**.
   A beat's `setting:` header was parsed but never read, so narrative had no
   concept of *where* it was happening.
2. **Chatrooms.** Authored `SPACE`/`CHANNEL` declarations, derived lobby /
   faction / DM channels, membership, post policy, threads — a full
   server-authoritative chat model that the story couldn't reach.

Concrete failure: `escape-the-internet`'s entry beat `doors_open` sets
`setting: Party` and has the `NARRATOR` deliver the opening word block. In the
editor's Sim mode the beat lit up on the story map — but the Narrator's words
appeared **nowhere** in the conversation surface. Three stacked causes:

- The event server never played `entry:` at all (`openDoors` didn't fire it).
- Unbound dialogue (`NARRATOR` with no `guest`/`self` person in scope) got
  `audience: []` — addressed to nobody, visible to nobody.
- Speakerless prose became an `action` event, which chat composition silently
  discarded (`default: break`).

Separately, the editor cockpit was operator-god-view only: no way to see a
specific guest's or performer's room set, and the composer could speak only as
cast members or "Operator" — never as a persona, never as the Narrator.

## Principles

1. **The room list is the one conversation surface.** Every guest-visible line
   the engine produces — scripted, reactive, or typed — lands in exactly one
   channel. Nothing is "log-only".
2. **Locations are rooms.** A `LOCATION` is already a first-class sim entity
   with occupancy; it now derives a `loc:<Location>` channel the same way a
   `FACTION` derives `faction:<F>`. The stage itself is a channel.
3. **The Narrator is a voice, not a ghost.** Speakerless prose is delivered
   `from: "Narrator"`, `kind: "narration"` — styled distinctly, addressable in
   the composer like any other voice.
4. **Message audience ≠ room visibility.** A message's `audience` records who
   it was *addressed to* (and gates guest history). A channel's visibility
   answers who may *open the room*. Story narration is stage voice — addressed
   to `"all"` — while typed chat in a location room is scoped to whoever is
   *present* when it is said.
5. **Perspective is a lens, not a mode.** The cockpit stays one surface; a
   "view as" selector re-filters rooms + messages through the same visibility
   rules the play app applies, and the composer defaults to that identity.

## Routing (the contract)

Where a sim event lands, in order of specificity:

| Event | Room | `from` / `kind` | Audience |
|---|---|---|---|
| `dialogue`, subject bound | `dm:<SPEAKER>` (unchanged) | speaker / `line` | `[subject]` |
| `dialogue`, no subject | `loc:<setting>` if the beat's setting is a known location, else lobby | speaker / `line` | `"all"` |
| `action` (narration, scene headings) | `loc:<setting>` else lobby | `"Narrator"` / `narration` | `"all"` |
| `respond` (device readout) | lobby | `""` / `narration` | `[to]` (or `"all"` if unbound) |
| `chat` typed into `loc:<L>` | that room | sender / `line` | occupants at send time + sender |
| `broadcast` / `ambient` / system notices | unchanged (lobby, faction mirrors, cue-routed channels) | | |

**Setting propagation.** The executor threads the *current setting* through
its frame stack: entering a beat reads the `setting:` contract line; control
-flow children (dialogue blocks, `<if:>` arms, choice bodies) inherit it; a
`-> beat` divert switches to the target's own setting **or inherits the
caller's** when the target declares none (a sub-beat continues in the same
place). Hook bodies run with no setting (→ lobby) unless they divert into a
beat that has one. `dialogue`/`action` events now carry `setting` + `beat`,
and `beatEntered` carries `setting` — so chat can route by place and the
cockpit can link any story line back to its beat on the story map.

**Entry beat.** `EventRuntime.openDoors()` fires `model.entry` through the
journaled `fireBeat` mutation on first open, so a live event opens exactly
like a rehearsal and the replay stays deterministic. (The editor's Sim mode
already did this.)

**Location channels.** `loc:<Location>` is a derived channel kind
(`"location"`): visible to everyone (the stage feed is part of the show),
listed in every view (operator, guest, performer), `member`/`canPost` = "is
the viewer currently there" (operator surfaces bypass, as they already do for
authored rooms). Sidebar order: lobby, factions, locations, authored, DMs.

## The cockpit (Sim ⌘3 / Run ⌘4)

- **Perspective selector** in the rail: Operator (god view, default), any
  guest/persona, any character. A non-operator perspective filters the room
  list to what that identity can see and the feed to `visibleTo(m, id)`;
  the composer defaults to speaking as them.
- **Act as anyone.** The composer's "post as" picker is grouped: Story
  (Operator, Narrator) · Guests (roster) · Cast (characters). Guest speech
  goes through the same journaled `say` path as the play app, so a rehearsal
  transcript replays deterministically.
- **Kind-aware rendering.** `narration` renders as an italic stage block,
  `system` as a dim notice, `signal` with an accent; `line`s group
  consecutive-sender runs like the play app.
- **Decisions in the room.** When the active perspective has a pending choice,
  the choice docks as a tray above the composer. Sim answers it against the
  local engine; Run answers it through the journaled `/api/mod/choose`
  (the mod snapshot carries `ModView.choices`), indistinguishable from the
  guest's own tap on replay.

## Non-goals / later

- Presence-gated *visibility* of location rooms (see who's in the room before
  entering) — visibility is deliberately open in this slice.
- The Rust mirror (parser/runtime crates) — explicitly deprioritized.
