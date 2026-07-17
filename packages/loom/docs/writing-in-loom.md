# Writing in Loom

*A complete guide for writers — from your very first line to a live,
interactive show.*

---

## Before you start

**You do not need to know how to program.** You do not even need to have
written a screenplay. If you can write a conversation between two people,
you can write in Loom. This guide starts there and builds up slowly.

**What Loom is.** Loom looks like a screenplay. Underneath, it is also a
little machine that *plays* your story — it remembers what the audience
chose, tracks how characters feel, branches when you want it to branch,
and (when you're ready) runs a live event with real people in a real
room. You write one file; it serves the reader, the director, the
performers, and the runtime all at once.

**What a `.loom` file is.** A plain text file whose name ends in
`.loom`. You can write it in the Loom editor, or in any text editor. A
**project** is just a folder of these files.

**How to read this guide.** Each part adds one new idea and shows it
working. Read in order the first time. Later, use it as a reference —
the [cheat sheet](#appendix-a--cheat-sheet) at the end lists every symbol
on one page.

A quick map of the journey:

| Part | You'll learn to… |
|---|---|
| [1](#part-1--your-first-scene) | Write dialogue, action, and a scene |
| [2](#part-2--branching-giving-the-audience-a-choice) | Offer choices and branch the story |
| [3](#part-3--memory-variables-and-conditions) | Remember things and react to them |
| [4](#part-4--variety-making-lines-feel-alive) | Vary lines so nothing feels canned |
| [5](#part-5--characters) | Give characters feelings, memory, and goals |
| [6](#part-6--traits-writing-less-by-reusing-shapes) | Reuse behaviour so you write each idea once |
| [7](#part-7--numbers-stats-pools-and-progression) | Add stats, health, levels, skill trees |
| [8](#part-8--directives-the--toolbox) | Trigger sound, light, and effects |
| [9](#part-9--organising-a-project) | Split a big story across many files |
| [10](#part-10--live--interactive-shows) | Run a live event with a real audience |

Let's write something.

---

## Part 1 — Your first scene

### 1.1 The two rules of a screenplay

If you've never written a script, here are the only two rules you need:

1. **A name on its own line is a person speaking.** The lines *underneath*
   it (pushed in a little) are what they say.
2. **A paragraph on its own is stage action** — something that happens,
   or something we see.

That's it. Here is a tiny scene:

```loom
WREN
  It hasn't rung in three days.

A bell rope swings in the gloom.
```

`WREN` is the speaker. "It hasn't rung in three days." is her line. The
sentence about the bell rope is action — nobody says it; it just happens.

> **Capitalisation and indentation carry meaning in Loom.** A speaker's
> name is written in CAPITALS. Their dialogue is *indented* (pushed to the
> right) underneath. Getting the indentation right is how Loom knows who's
> talking — so be consistent. Two spaces per step is the convention used
> throughout this guide. (Use spaces, not tabs.)

### 1.2 The smallest complete file

A real file needs a **title** and at least one **beat**. A *beat* is a
chunk of story — think of it as a scene or a moment. You open a beat with
two equals signs, `==`, and a name:

```loom
# The Lighthouse

== opening

WREN
  It hasn't rung in three days.

A bell rope swings in the gloom.
```

- `# The Lighthouse` — the title. (The `#` just marks the title line.)
- `== opening` — begins a beat named `opening`. Everything after it,
  until the next `==`, belongs to this beat.

This is a complete, playable Loom story. Short, but complete.

### 1.3 Parentheticals — a hint to the performer

If you want to tell the actor *how* to say a line — quietly, angrily, with
a laugh — put it in parentheses on its own indented line, above the words:

```loom
WREN
  (quietly)
  It hasn't rung in three days.
```

`(quietly)` is a **parenthetical**. It's guidance for whoever performs
the line; it isn't spoken aloud.

### 1.4 Where are we? Scene headings

Screenplays often announce a location in a bold line like `INT. LIGHTHOUSE
- DAWN` ("INT." means *interior*, a scene indoors; "EXT." is *exterior*).
Loom recognises these:

```loom
== opening

INT. LIGHTHOUSE - DAWN

A bell rope swings in the gloom.

WREN
  (quietly)
  It hasn't rung in three days.
```

A scene heading is a nice signpost for readers and directors. Good to
know: in Loom, the *beat* (`==`) is what actually organises the story —
the scene heading is just a label inside it.

### 1.5 Who's in the scene, and where

Right under a beat's `==` line, you can note who's present and the
setting. These indented `key: value` lines are the beat's **contract**:

```loom
== opening
  cast: Wren, Player
  setting: Lighthouse

INT. LIGHTHOUSE - DAWN

WREN
  (quietly)
  It hasn't rung in three days.
```

- `cast:` — the characters in this beat. (`Player` is the audience/reader.)
- `setting:` — where it takes place.

You don't *have* to write these yet, but they become important later
(especially for live shows), so it's a good habit.

**You now know enough to write a linear scene.** Next: letting the
audience change what happens.

---

## Part 2 — Branching: giving the audience a choice

### 2.1 A choice

Put a `*` at the start of a line to offer the audience a choice:

```loom
WREN
  (quietly)
  It hasn't rung in three days.

* Ring the bell.
* Leave quietly.
```

The reader sees two options — *Ring the bell.* / *Leave quietly.* — and
picks one. Whatever is indented **under** a choice happens when they pick
it:

```loom
* Ring the bell.
  The sound carries across the rocks.
* Leave quietly.
  You slip out before she can turn around.
```

### 2.2 Going somewhere: diverts

A story with more than one moment needs a way to move between beats. That's
a **divert**, written `->` ("go to"):

```loom
== opening

WREN
  It hasn't rung in three days.

* Ring the bell.
  -> ringing
* Leave quietly.
  -> END

== ringing

The sound carries across the rocks.

WREN
  (stunned)
  You... rang it.

-> END
```

- `-> ringing` jumps to the beat named `ringing`.
- `-> END` ends the story.

So *Ring the bell.* sends us to the `ringing` beat; *Leave quietly.* ends
things. Beats can be in any order in the file — a divert finds its target
by name.

### 2.3 The entry point

When a project has several beats, Loom needs to know which one starts. Name
it at the top with `entry:`:

```loom
# The Lighthouse
entry: opening

== opening
  ...
```

If you don't write `entry:`, the first beat in the file is the start.

### 2.4 Once vs. sticky choices

- `*` is a **once-only** choice — after the audience picks it, it's gone.
- `+` is a **sticky** choice — it stays available, so they can pick it
  again on a later visit.

```loom
+ Ask about the bell.
  -> ask_bell
* Storm out.
  -> END
```

Use `+` for things like "Ask another question" that should remain on the
menu, and `*` for one-time decisions.

### 2.5 Hidden text on a choice

Sometimes the *button* should read one way and the *story* another. Wrap
the extra words in square brackets `[ ]` and they show up only after the
choice is taken, not on the button:

```loom
* Leave quietly.[ But you wonder what you're walking away from.]
  -> END
```

The audience sees the button **Leave quietly.** After they click it, the
narration reads: *Leave quietly. But you wonder what you're walking away
from.*

### 2.6 Nesting choices

Anything under a choice can include *more* choices, dialogue, action, and
diverts — as deep as you like:

```loom
* Confront her.
  WREN
    You shouldn't have come.
  * Apologise.
    -> makeup
  * Hold your ground.
    -> standoff
* Say nothing.
  -> END
```

**You can now write a branching story.** Next: making it remember things.

---

## Part 3 — Memory: variables and conditions

A branching story is good; a story that *remembers* is better. Loom keeps
track of numbers, facts, and what the audience has done.

### 3.1 Dropping a value into text

Curly braces `{ }` insert a value into a line. Whatever's inside is worked
out and printed:

```loom
WREN
  You have {coins} coins left.
```

If `coins` is 3, the audience reads: *You have 3 coins left.*

### 3.2 Changing a value: `<set: ...>`

The angle brackets `< >` are how you tell the runtime to *do* something.
The most common is `set`:

```loom
<set: coins = 10>
<set: coins += 5>
<set: coins -= 2>
```

- `=` gives a value.
- `+=` adds to it; `-=` subtracts.

So after those three lines, `coins` is 13.

You'll also use `set` to record facts:

```loom
<set: rang_the_bell = true>
```

### 3.3 Reacting to values: `<if:>`

Show something only when a condition holds, using `<if:>`. You can add
`<else if:>` for more cases and `<else>` for "otherwise":

```loom
<if: coins > 5>
  WREN
    Keep your coins. You'll need them.
<else if: coins > 0>
  WREN
    That won't get you far.
<else>
  WREN
    Broke, then. Figures.
```

Everything indented under an arm plays only when that arm's condition is
true.

The words you can use in a condition:

| You write | Means |
|---|---|
| `>`  `<`  `>=`  `<=` | greater / less than (or equal) |
| `==` | is equal to |
| `!=` | is not equal to |
| `and` | both must be true |
| `or` | either can be true |
| `not` | flips true/false |

```loom
<if: coins > 5 and not rang_the_bell>
  ...
```

### 3.4 Living values: `let`

A `let` gives a name to a *formula*. It's not a one-time calculation — it
stays true to its definition and updates itself whenever the pieces
change:

```loom
let trusted = Wren.trusts.Player > 50
```

Now `trusted` is always up to date, and you can use it as a tidy shorthand
anywhere:

```loom
<if: trusted>
  WREN
    I knew you'd come.
```

Put a `let` at the top of your file (near the title) to make it available
everywhere, or inside a beat to keep it local to that beat.

### 3.5 What has the audience already done?

Loom keeps a running record — a **ledger** — of everything that's
happened. A few questions you can ask it, right inside a condition:

| You write | Answers |
|---|---|
| `played(opening)` | Have we ever played the `opening` beat? |
| `visits(opening)` | How many times? (a number) |
| `since(bell_rung)` | How long since that happened? |

```loom
<if: visits(opening) == 1>
  WREN
    First time here, I see.

<if: since(bell_rung) < 30s>
  The echo hasn't faded yet.
```

(`30s` means 30 seconds. You can write `s` for seconds, `m` for minutes.)

**Your story can now remember and react.** Next: keeping it from sounding
repetitive.

---

## Part 4 — Variety: making lines feel alive

A line the audience hears twice shouldn't read identically both times.
Loom has small tools for this.

### 4.1 Cycles and shuffles

`<cycle: ... | ... | ...>` steps through options in order, one per visit.
`<shuffle: ...>` picks one at random. Separate options with `|`:

```loom
FISHER
  <cycle: Quiet night. | Stars are out. | Tide's calm.>

DOCKHAND
  <shuffle: Storm's close. | Sky's wrong. | Time to tie down.>
```

The first time the fisher speaks he says "Quiet night."; next time, "Stars
are out."; and so on. The dockhand's line is random each time.

### 4.2 First time, next time, finally

`<each visit>` lets you write a beat that changes as it's revisited:

```loom
<each visit>
  first
    WREN
      Who are you?
  then
    WREN
      You again.
  finally
    WREN
      I'm tired of your questions.
```

- `first` — plays on visit 1.
- `then` — plays on visits after that.
- `finally` — plays once you've settled on the last variation.

### 4.3 Before and after a turning point

`<after: condition>` swaps a beat's content once something becomes true.
Pair it with `<otherwise>` for the "before" version:

```loom
SELF
  <after: guest.captured>
    I've been in here since the old forums. Don't end up like me.
  <otherwise>
    A flickering figure mouths something you can't quite read.
```

Before `guest.captured` is true, the audience gets the flickering figure.
After, they get the warning — permanently.

### 4.4 Choosing by value: `<match:>`

When a value has several possible states, `<match:>` picks the matching
arm:

```loom
<match: weather>
  storm
    The rain comes sideways.
  fog
    You can't see the harbour wall.
  clear
    Gulls wheel over a flat sea.
```

**Your lines can now vary naturally.** Next: real characters.

---

## Part 5 — Characters

So far, speakers have just been names. Loom lets you make them into real
**characters** — with feelings, memory, and goals — so the story can react
to *them*, not just to what the audience clicked.

### 5.1 Declaring a character

Write `CHARACTER`, a name, and (indented) some properties:

```loom
CHARACTER Wren
  voice: female_alto
  home: Lighthouse
  hp: 80
```

Properties are just `key: value` facts about them. You choose the keys.

### 5.2 How they feel about you: disposition

Characters can hold feelings toward others on a scale. The built-in ones
are `trusts`, `respects`, and `fears`. Write the feeling, who it's about,
and the value `N of M` (N out of a maximum of M):

```loom
CHARACTER Wren
  trusts Player: 30 of 100
  respects Player: 50 of 100
  fears Player: 0 of 100
```

Wren starts trusting the Player 30 out of 100. You nudge these with `set`:

```loom
<set: Wren.trusts.Player += 20>
```

### 5.3 Reacting to feelings: `reacts`

A `reacts` line gives a name to a mood the character falls into when a
feeling crosses a line:

```loom
CHARACTER Wren
  trusts Player: 30 of 100
  reacts trust > 60 -> warm
  reacts fear > 40 -> guarded
```

When trust climbs past 60, Wren becomes `warm`; when fear passes 40, she's
`guarded`. (You then write beats or lines that check her mood.)

### 5.4 What they know: `knows`

A character can carry a little sheet of facts — their **knowledge**. List
them under `knows:`, each with a type and a starting value:

```loom
CHARACTER Wren
  knows:
    met_player: bool = false
    saw_the_keeper: bool = false
```

- `bool` means true/false.
- `= false` is the starting value.

Update knowledge like any other value:

```loom
<set: Wren.knows.met_player = true>
```

Knowledge can be more than yes/no. A fact with several stages is written
with `|` between the options:

```loom
CHARACTER Wren
  knows:
    bell_origin: unknown | suspects | confirmed = unknown
```

Now `Wren.knows.bell_origin` moves through `unknown` → `suspects` →
`confirmed` as your story reveals things.

### 5.5 Hooks: "when X happens, do Y"

A **hook** starts with `on` and fires automatically when something
happens. It's how a character responds to the world without you wiring it
up by hand at every call site:

```loom
CHARACTER Wren
  on meeting Player
    <set: Wren.knows.met_player = true>
    -> greet

  on trust passes 80
    -> reveal_secret
```

- `on meeting Player` — fires the first time Wren meets the Player.
- `on trust passes 80` — fires when trust crosses 80 going up.

Some hooks you'll reach for:

| Hook | Fires when… |
|---|---|
| `on meeting X` | the character first meets X |
| `on trust passes N` | a feeling crosses N upward |
| `on trust drops below N` | …crosses N downward |
| `on enters Lighthouse` | they enter a location |
| `on every 60s` | on a repeating timer |

### 5.6 Goals

A **goal** is something a character is trying to achieve. Loom tracks it as
a little state machine — it becomes active, then completes or fails on the
conditions you set:

```loom
CHARACTER Wren
  goal find_keeper
    priority: 0.8
    active when: true
    completes when: Wren.knows.saw_the_keeper
```

- `priority` — how much it matters (0 to 1), when goals compete.
- `active when` — the condition under which she's pursuing it.
- `completes when` — the condition that satisfies it.

You can also add `fails when:` and `on complete:` / `on fail:` follow-ups.

**Putting it together:**

```loom
CHARACTER Wren
  hp: 80
  trusts Player: 30 of 100
  reacts trust > 60 -> warm

  knows:
    saw_the_keeper: bool = false

  goal find_keeper
    priority: 0.8
    active when: true
    completes when: Wren.knows.saw_the_keeper

  on trust passes 80
    -> reveal_secret
```

**Your characters are now alive.** Next: how to stop repeating yourself.

---

## Part 6 — Traits: writing less by reusing shapes

Once you have a dozen characters, you'll notice they share behaviour. A
**trait** captures a shape once so every character can wear it. This is the
single biggest lever for keeping a large story manageable — so it's worth
learning well.

### 6.1 A trait is a reusable bundle

Write `TRAIT` exactly like `CHARACTER`, but describe a *role* rather than a
specific person:

```loom
TRAIT Keeper
  home: Lighthouse
  voice: solemn

TRAIT Combatant
  hp: 100
```

A trait on its own does nothing — it's a template waiting to be worn.

### 6.2 Wearing traits: `is`

Give a character traits with `is`, separating several with commas:

```loom
CHARACTER Wren is Keeper, Combatant
  hp: 80
```

Wren now has everything from `Keeper` and `Combatant`. When a trait and the
character set the same property, the character wins — here Wren's `hp: 80`
overrides `Combatant`'s `hp: 100`.

`is` also expresses *"is a kind of"*: `CHARACTER GoblinKing is Goblin` makes
a king that starts from everything a goblin is.

### 6.3 Traits with a setting: parameters

A trait becomes far more useful when it can be *pointed at* something. Put a
parameter in parentheses after the trait's name, and refer to it inside as
`self.<parameter>`:

```loom
TRAIT Scanner(beat)
  on scan guest
    -> self.beat
```

`Scanner` says: "when someone scans me, jump to *the beat I was told
about*." Each character fills in the blank when they wear it:

```loom
CHARACTER Crawler is Scanner(crawler_report)
CHARACTER Paywall is Scanner(paywall)
```

Now scanning the Crawler goes to the `crawler_report` beat; scanning the
Paywall goes to `paywall`. One trait, written once, aims wherever you send
it.

`self` always means *"this character"* — the one wearing the trait right
now.

### 6.4 Combining traits into new traits

Traits can wear other traits, so you can build up vocabulary:

```loom
TRAIT Algo
  faction: TheAlgorithm

TRAIT AlgoScanner(beat) is Scanner(beat), Algo
```

`AlgoScanner` fuses "belongs to The Algorithm" with "scans to a beat." A
whole cast of villain props then collapses to one honest line each:

```loom
CHARACTER Crawler is AlgoScanner(crawler_report)
CHARACTER Captcha is AlgoScanner(captcha_gate)
CHARACTER Paywall is AlgoScanner(paywall)
```

### 6.5 Beats that belong to a character

A character can *own* a beat — written right inside them, so their scene
lives beside the behaviour that triggers it. Use `beat name(...)` and reach
it with `-> self.beat`:

```loom
CHARACTER Sysadmin is Algo
  on scan guest
    -> self.interrogation

  beat interrogation(guest)
    SELF
      Designation {guest.name}. Talk.
    * Name the Glitchers
      <set: guest.heat -= 10>
    * Say nothing
      <set: guest.karma += 20>
```

Two things to notice:

- **`SELF`** is a special speaker meaning "whoever owns this beat." Here it
  speaks as the Sysadmin, so you don't have to name them again.
- Because the beat is *owned*, two different characters can each have their
  own `interrogation` with no collision.

### 6.6 Shared templates with blanks: `slot` and `fill`

Sometimes several characters share the *structure* of a beat but differ in
the words. A trait can ship a beat with blanks — `slot:` lines — and each
character fills them in with a `fill` block:

```loom
TRAIT Gatekeeper
  beat confront(guest)
    SELF
      <if: guest.captured>
        slot: pitch
      <else>
        slot: dismissal

CHARACTER Bouncer is Gatekeeper
  fill pitch
    List's closed, {guest.name}. Name a Mod who'll vouch, or wait.
  fill dismissal
    Not on the list? Then you were never here.
```

Every gatekeeper gets the same shape (`confront`), but the Bouncer speaks
the Bouncer's words. Note the small spelling difference: `slot:` keeps its
colon (it's a labelled blank); `fill` doesn't (it opens a block).

### 6.7 Extending, not replacing: `super`

If a character overrides a trait's hook but wants to *keep* the original
behaviour and add to it, drop a bare `super` line where the inherited body
should run:

```loom
CHARACTER ChattyGuard is Guard
  on meeting Player
    super
    GUARD
      And try not to drip on the flagstones.
```

`super` runs everything `Guard` normally does on meeting the Player, *then*
adds the extra line.

To *remove* an inherited hook entirely, silence it with `: none`:

```loom
CHARACTER SilentGuard is Guard
  on meeting Player: none
```

**You can now build a large cast without repeating yourself.** The next
two parts are optional depending on your project — stats for game-like
numbers, directives for stagecraft.

---

## Part 7 — Numbers: stats, pools, and progression

*Skip this part if your story doesn't need game-style numbers.* If it does
— health, damage, levels, skill trees — Loom has a dedicated toolkit called
**stats**.

### 7.1 A stats sheet

Declare a `STATS` block with named numbers:

```loom
STATS Combat
  attribute strength = 10, range 1 to 30
  attribute agility  = 10, range 1 to 30
  stat damage = 8 + strength * 0.5
```

There are four kinds of number:

- **`attribute`** — a plain value you set, optionally clamped to a `range`.
- **`stat`** — a *computed* value: a formula that recalculates itself
  (here, damage rises with strength).
- **`pool`** — a spendable, refilling gauge (health, stamina, mana).
- **`axis`** — a value that *advances* along a track (like an XP level).

A pool and an axis:

```loom
STATS Combat
  attribute strength = 10, range 1 to 30

  pool health
    max: max_health
    regen: 2/s when not in_combat

  axis level
    mode: xp_curve
    curve: level * level * 50

  stat max_health = 50 + strength * 5
```

- The `health` pool refills 2 per second while out of combat, up to
  `max_health`.
- The `level` axis climbs an experience curve.

### 7.2 Giving a character stats

Attach a stats sheet to a character with `stats:`, filling in any starting
values:

```loom
CHARACTER Wren is Keeper
  stats: Combat(strength: 12)
  hp: 80
```

You can then read the numbers with a dot path — `Wren.strength`,
`Wren.damage`, `Wren.health` — and change them with `set`.

### 7.3 Skill trees

A `TREE` is a set of unlockable nodes, each of which can require others
first:

```loom
TREE WarriorPath
  node armsman_1
    cost: 1
    effect: stat(damage) += 5

  node armsman_2
    cost: 1
    requires: node(armsman_1)
    effect: stat(damage) += 5
```

`armsman_2` can't be taken until `armsman_1` is — that's what `requires`
enforces. Each node's `effect` changes the character's stats when unlocked.

**That's the numbers layer.** Reach for it only when your story is
game-like; a pure narrative piece never needs it.

---

## Part 8 — Directives: the `< >` toolbox

You've already met a few of the angle-bracket commands — `<set:>`,
`<if:>`, `<cycle:>`. These are **directives**: instructions to the
runtime. This part collects the ones a writer uses most.

A directive is `<name: arguments>`. Some take no arguments (`<pause>`);
some open an indented block.

### 8.1 Stagecraft directives

| Directive | Does |
|---|---|
| `<sfx: bell_toll>` | play a sound effect |
| `<cue: lx_dawn>` | fire a lighting/tech cue |
| `<pause>` | hold for a beat |
| `<flash: white, 200>` | a 200ms white flash |
| `<anchor: the_bell_rings>` | name this spot in the story (for tests and analytics) |

```loom
WREN
  (startled)
  Lightning?<flash: white, 200>

<sfx: distant_thunder>
```

Notice the flash sits *right at the end of the line* — a directive fires at
exactly the point it appears in the text.

### 8.2 Story directives

| Directive | Does |
|---|---|
| `<set: x = 5>` | change a value (Part 3) |
| `<fire: bell_acknowledged>` | announce a named event other hooks can listen for |
| `<goal: Wren/find_keeper complete>` | push a character's goal forward |

`<fire: ...>` is worth calling out: it lets one part of the story send a
signal that another part — a hook, a listening character — reacts to,
without them being directly wired together.

### 8.3 Block directives

Some directives wrap a chunk of story. `<broadcast: ...>` (which you'll
meet properly in Part 10) is one — it opens, and everything indented
beneath belongs to it:

```loom
<broadcast: location(Plaza)>
  NARRATOR
    Welcome to The Stack. Tonight, you choose a side.
```

The rule is consistent everywhere in Loom: **indentation shows what belongs
to what.** A directive with indented lines under it owns those lines.

---

## Part 9 — Organising a project

A short story fits in one file. A big one shouldn't. Here's how Loom
projects scale.

### 9.1 A project is a folder

Put your `.loom` files in a folder. One of them is `main.loom` — the front
door, where the title and `entry:` live:

```
the-lighthouse/
  main.loom          ← title, entry, shared characters
  beats/
    opening.loom
    ringing.loom
    endings.loom
  cast/
    wren.loom
```

### 9.2 Names find each other automatically

You never write file paths in your story. A divert just names its target,
and Loom finds it *anywhere in the project* — even in another file, another
folder:

```loom
-> ringing
```

The same goes for characters: declare `Wren` in `cast/wren.loom` and speak
as `WREN` in any beat. This "flat" naming keeps your writing clean; you
move files around without rewriting diverts.

### 9.3 When two beats share a name

If you deliberately have two beats named `ringing` in different folders, be
specific with a `/`:

```loom
-> Lighthouse/ringing
```

And to jump to a specific spot inside a file, use `#`:

```loom
-> cast/wren#backstory
```

### 9.4 Notes to yourself: comments

Anything after `//` is a **comment** — a note for you, ignored by the
runtime and invisible to the audience. For a longer note, wrap it in `/*
... */`:

```loom
// rough order: bell, beat, lantern up, line
WREN
  It hasn't rung in three days. // pick up the pace here

/*
  Blocking sketch from rehearsal 04-12:
  Wren crosses to the lantern on "three."
*/
```

### 9.5 Notes for the production team: fences

A **fence** — a block wrapped in triple backticks — holds information for
the director, stage manager, or crew. The runtime ignores it, but it stays
visible in the prompt book:

````loom
WREN
  It hasn't rung in three days.
  ```blocking: cross to the lantern on "three"```

```note
This scene ran long at the table read. Consider cutting the lantern beat.
```
````

Comments are for *you*; fences are for the *team*. Both leave the story
itself untouched.

**You can now structure a project of any size.** The final part is where
Loom does something no screenplay can: run live.

---

## Part 10 — Live & interactive shows

This is Loom's reason for being. Everything so far — characters, choices,
memory — was building toward stories that play out *with a real audience,
in a real space, in real time*. This part is longer because it's the most
powerful; take it a section at a time.

### 10.1 The idea

In a live Loom show, the audience aren't just readers — they're
**participants**. They move between physical (or virtual) rooms, join
sides, get scanned, chat, and make choices, while performers run scripted
and improvised beats around them. Loom coordinates all of it and keeps the
story consistent for everyone.

The running example for this part is a show called *Escape the Internet*:
guests log on, pick a faction, and try not to get "captured" by a hidden
villain called The Algorithm.

### 10.2 The four kinds of "person"

Live shows separate ideas that a page play blurs together:

| You write | Means |
|---|---|
| `CHARACTER` (or `ROLE`) | a *part* in the script — Wren, the Sysadmin |
| `PERSON` | a *real human* who might perform — Jamie Lee |
| `ROSTER` | the plan for one night — who plays what |
| `<cast: ...>` | the live act of a person taking a part |

A **ROLE** is just another name for CHARACTER, used when you're thinking
about casting. A **PERSON** is a real individual:

```loom
PERSON jamie_lee
  display_name: Jamie Lee
  pronouns: they/them
  content_tolerance: [no_strobe]
```

A **ROSTER** is the line-up for a specific performance — who plays whom,
who covers (swings), and where people start:

```loom
ROSTER preview_night
  date: 2026-05-28T19:30
  capacity: 24

  cast
    Wren     := jamie_lee
    Initiate := any of [audience]

  swings
    Wren     := [raja_park, kim_ho]
```

`:=` assigns a person to a role. `any of [audience]` means "any walk-up can
fill this." At showtime you load it and bind people:

```loom
<load_roster: preview_night>
<cast: jamie_lee as Wren>
```

The point: your *script* never names a real person. The roster maps humans
to parts for one night, so the same script runs with a different cast
tomorrow.

### 10.3 Groups and places: cohorts and locations

A **COHORT** is a named group of participants; a **LOCATION** is a place
they can be:

```loom
COHORT Chatters
  label: The Chatters
  capacity: 24

COHORT Mods
  label: The Mods
  capacity: 24

LOCATION Plaza
  label: The Uplink Plaza
  ambient: neon-static
  capacity: 32
```

Participants flow between cohorts and locations as the night unfolds, and
your story can read those counts and memberships to decide what happens
next.

Every location is also a **room** where conversation happens — when a beat's
`setting:` is a location, its dialogue is heard by whoever is standing
there.

### 10.4 Speaking to a subset: broadcast

In a room full of people, you rarely address *everyone*. `<broadcast:
scope>` sends the lines inside it only to participants who match:

```loom
<broadcast: location(Plaza)>
  NARRATOR
    Welcome to The Stack. Tonight, you choose a side.

<broadcast: cohort(Chatters)>
  VEX
    Camp's open. Follow me if you've had enough of being deleted.
```

The scope atoms:

- `location(Plaza)` — everyone in the Plaza.
- `cohort(Chatters)` — everyone in the Chatters group.
- `participant(guest)` — one specific person.

Combine them with `and` (both) and `but` (except):

```loom
<broadcast: cohort(Singers) and location(BellTower)>
  <cue: private_choir>

<broadcast: location(BellTower) but participant(guest)>
  NARRATOR
    The others look up. You don't.
```

### 10.5 Improvised beats

Live performers don't read every word. An **improv beat** gives them a
direction and a *duration*, and hands control back to the story when a
signal arrives:

```loom
BELLKEEPER
  (improv duration: 45s, advance on: any [pedal, speech(let us begin), gesture(Bow)])
  (Greet warmly. Find out why they came. Don't mention the singing.)
  -> next_beat
```

Reading that parenthetical:

- **`duration: 45s`** — roughly how long the improv runs.
- **`advance on: any [...]`** — what ends it. Here *any* of: a foot
  `pedal`, someone saying "let us begin" (`speech`), or a `Bow` gesture.
- The second parenthetical is the *direction* to the performer — what to
  play. It isn't spoken verbatim.

`advance on` can require more than one signal: `all [...]` waits for every
listed signal; `quorum(8) [...]` waits until 8 people have given one.

### 10.6 Text chat rooms: spaces and channels

Many live Loom shows include a chat layer. Declare **SPACE**s (sidebar
sections) and **CHANNEL**s (individual rooms):

```loom
SPACE Forums
  label: The Forums

  CHANNEL general
    kind: open
    label: # general

  CHANNEL backroom
    kind: private
    label: # the-backroom
    invite: members

CHANNEL mod_lounge
  space: Forums
  kind: faction
  faction: Mods
  label: # mod-lounge
```

Channel `kind` controls who sees and posts:

- `open` — everyone.
- `faction` — only that faction's members.
- `private` / `group` / `dm` — only invited members.

Extra knobs let a channel be read-only (`type: announcement`),
rate-limited (`slow: 3s`), or disappearing (`ephemeral: 30s`).

### 10.7 The game verbs: hooks that drive a live show

Live shows lean heavily on **hooks** (Part 5.5) — `on <event>` blocks that
fire as participants act. This is where a show's rules live. From *Escape
the Internet*:

```loom
CHARACTER ModBot is Algo
  on scan guest
    <capture: guest into Internet>

CHARACTER TheAdmin is Algo
  captures: 0 to 100 = 0
  on captured guest
    <broadcast: doomed to participant(guest)>
    <set: self.captures += 1>
    <if: self.captures >= 2>
      <reveal: TheAlgorithm>
```

Read it as plain English: when the ModBot scans a guest, capture them; each
time The Admin captures someone, tally it, and once it's captured two, blow
the villain's cover with `<reveal:>`.

A few live directives you'll see:

| Directive | Does |
|---|---|
| `<capture: guest into Internet>` | move a participant into a "captured" state/room |
| `<reveal: TheAlgorithm>` | expose a hidden faction to everyone |
| `<escape: guest>` | free a captured participant |
| `<respond: text>` | send a private reply back to whoever acted |
| `<enroll: jamie_lee → Initiates>` | add a participant to a cohort |

### 10.8 The audience as a character: `as Participant` and `self`

In a live show, `self` and the special speaker `SELF` (Part 6.5) do a lot
of work: they let one beat serve many participants at once, each seeing
themselves in it.

```loom
on participant joins
  <enroll: participant → Initiates>
  -> orientation as participant

== orientation
  cast: self

<if: self.content_tolerance contains no_strobe>
  -> gentle_intro as self
-> standard_intro as self
```

`as participant` runs the same beat *for each person* who joins, with
`self` bound to that person — so it can check *their* preferences and route
*them* individually, all from one piece of writing.

### 10.9 Background life: scenes and generators

A living world needs things happening even when the audience isn't looking.
Two tools spin up background activity:

A **GENERATOR** yields ambient content on a loop:

```loom
GENERATOR HarborChorus
  tier: ambient
  priority: 0.3

  loop
    yield bark from Quiet night. | Stars are out. | Tide's calm.
```

A **SCENE** is a longer scripted sequence a character runs on its own, with
named stages and waits:

```loom
SCENE patrol(character)
  loop
    wait until character.spotted_intruder
    -> investigate

SCENE investigate(character)
  approach
    -> examine
  examine
    wait until character.deduction > 60
    -> confront
  confront
    return clue
```

- `loop` repeats; `wait until` pauses for a condition.
- Bare names inside a SCENE (`approach`, `examine`, `confront`) are its
  stages.
- `tier:` sets how often it runs — `focal` (constant attention),
  `active` (frequent), or `ambient` (occasional, cheap background life).

You don't need these for your first live show — but they're how a Loom
world keeps breathing on its own.

**You've now seen the whole language.** Write small, run it, and add one
idea at a time.

---

## Appendix A — Cheat sheet

**Structure**

| Symbol | Meaning |
|---|---|
| `# Title` | project / file title |
| `entry: name` | which beat starts the story |
| `== name` | begins a beat |
| `== name(param)` | a beat that takes a value |
| `cast:` / `setting:` | who's in a beat / where it is |
| `//` … , `/* … */` | comments (for you) |
| triple-backtick fence | notes for the production team |

**Dialogue & prose**

| Form | Meaning |
|---|---|
| `NAME` then indented line | speaker + their dialogue |
| `(quietly)` | parenthetical — a hint to the performer |
| flush-left paragraph | action / narration |
| `INT. PLACE - TIME` | scene heading |
| `SELF` | speaker = whoever owns this beat |
| `A \| B` | two possible speakers |

**Choices & flow**

| Form | Meaning |
|---|---|
| `* text` | once-only choice |
| `+ text` | sticky choice |
| `text[ hidden]` | words shown only after the choice |
| `-> beat` | go to a beat |
| `-> self.beat` | go to a beat this character owns |
| `-> folder/beat` , `-> file#knot` | disambiguated jumps |
| `-> END` | end the story |

**Values & logic**

| Form | Meaning |
|---|---|
| `{expr}` | insert a value into text |
| `<set: x = 5>` , `+= -=` | change a value |
| `<if:>` `<else if:>` `<else>` | conditional |
| `let name = formula` | a live, self-updating value |
| `and` `or` `not` `== != > < >= <=` | condition operators |
| `visits(b)` `played(b)` `since(e)` | ask the ledger |

**Variety**

| Form | Meaning |
|---|---|
| `<cycle: a \| b \| c>` | step through in order |
| `<shuffle: a \| b \| c>` | pick at random |
| `<each visit>` + `first`/`then`/`finally` | change on revisit |
| `<after: cond>` + `<otherwise>` | swap content at a turning point |
| `<match: value>` | pick an arm by value |

**Characters & traits**

| Form | Meaning |
|---|---|
| `CHARACTER Name` | declare a character |
| `is X, Y` | wear traits / inherit |
| `trusts P: 30 of 100` | disposition |
| `reacts trust > 60 -> mood` | mood on a threshold |
| `knows:` | a character's facts |
| `goal name` | something they're pursuing |
| `on <event>` | a hook — do this when that happens |
| `TRAIT Name(param)` | reusable shape with a blank |
| `self.field` | "this character's" field |
| `beat name()` inside a character | an owned beat |
| `slot:` / `fill` | template blanks / filling them |
| `super` , `on X: none` | extend / silence an inherited hook |

**Stats**

| Form | Meaning |
|---|---|
| `STATS Name` | a stats sheet |
| `attribute` / `stat` / `pool` / `axis` | a set value / formula / gauge / track |
| `TREE Name` + `node` | an unlockable skill tree |
| `stats: Combat(strength: 12)` | attach stats to a character |

**Live shows**

| Form | Meaning |
|---|---|
| `PERSON` / `ROSTER` / `<cast: p as R>` | real people, the night's plan, binding |
| `COHORT` / `LOCATION` | a group / a place (and a chat room) |
| `<broadcast: scope>` | send lines to a subset |
| `location(X)` `cohort(X)` `participant(X)` , `and` / `but` | broadcast scopes |
| `(improv duration: 45s, advance on: any […])` | an improvised beat |
| `SPACE` / `CHANNEL` | chat sections / rooms |
| `<capture:>` `<reveal:>` `<escape:>` `<respond:>` `<enroll:>` | live game verbs |
| `as participant` / `self` | run one beat per participant |
| `GENERATOR` / `SCENE` , `loop` / `wait until` / `tier:` | background life |

---

## Appendix B — A complete little story

This is a whole, self-contained Loom file. Read it top to bottom; you know
every piece now.

```loom
# Saltmere
entry: opening

// Wren warms to you as you earn her trust; cross 80 and she opens up.
CHARACTER Wren is Keeper
  hp: 80
  trusts Player: 30 of 100
  reacts trust > 60 -> warm

  knows:
    saw_the_keeper: bool = false

  on trust passes 80
    -> reveal_secret

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
* Earn her trust first.
  <set: Wren.trusts.Player += 60>
  WREN
    ...Alright. Maybe you do understand.
* Leave quietly.[ But you wonder what you're walking away from.]
  -> END

== ringing
  cast: Wren, Player

The sound carries across the rocks.

<cue: rope_creak>

WREN
  (stunned)
  You... rang it.

-> END

== reveal_secret
  cast: Wren, Player

WREN
  (whispering)
  The bell rings for the keeper alone. I was the last one before you.

-> END
```

---

## Appendix C — Common mistakes

- **Indentation must be consistent.** A dialogue line has to be indented
  under its speaker; a choice's content under the choice. Use spaces (two
  per step is the norm), never tabs. Most "it didn't work" moments are an
  indentation slip.
- **Speakers are CAPITALISED.** `WREN` is a speaker; `Wren` is the
  character you declared. Loom tells them apart by case.
- **`=` changes a value; `==` compares.** `<set: x = 5>` assigns.
  `<if: x == 5>` tests. Swapping them is a classic slip.
- **A divert needs a real target.** `-> ringng` (a typo) has nowhere to
  go. Check beat names match.
- **`*` disappears, `+` stays.** If a choice you expected is gone on a
  second visit, it was probably a `*`.
- **`SELF` needs an owner.** It only works in a beat a character *owns*
  (Part 6.5) or one reached through that character; in a plain beat, name
  the speaker.
- **Traits don't act alone.** A `TRAIT` is a template — nothing happens
  until a `CHARACTER` wears it with `is`.

---

*Write a little, run it, add one idea. That loop — not this guide — is how
you'll actually learn Loom. Welcome to the weave.*
