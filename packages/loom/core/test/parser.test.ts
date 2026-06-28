import { describe, expect, it } from "vitest";
import { parse, Code } from "../src/parser/index.ts";
import type { Beat, BodyItem, Declaration, DialogueBlock, Divert, Item } from "../src/parser/index.ts";

function beatOf(item: Item): Beat {
  if (item.kind !== "beat") throw new Error("expected beat");
  return item.value;
}
function declOf(item: Item): Declaration {
  if (item.kind !== "declaration") throw new Error("expected declaration");
  return item.value;
}
function dialogueOf(item: BodyItem): DialogueBlock {
  if (item.kind !== "dialogue") throw new Error("expected dialogue");
  return item.value;
}
function divertOf(item: BodyItem): Divert {
  if (item.kind !== "divert") throw new Error("expected divert");
  return item.value;
}

describe("parser::parse", () => {
  it("collects header title and properties", () => {
    const [file, diags] = parse("# Saltmere\nentry: opening\n");
    expect(diags).toHaveLength(0);
    expect(file.header.title).toBe("Saltmere");
    expect(file.header.properties.get("entry")?.value).toBe("opening");
  });

  it("parses a beat contract then body", () => {
    const src = `== opening
  cast: Wren, Player
  setting: Lighthouse

A bell rope swings in the gloom.

WREN
  (quietly)
  It hasn't rung in three days.

* Ring the bell.
  -> ringing
* Leave quietly.
  -> END
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    expect(file.items).toHaveLength(1);
    const beat = beatOf(file.items[0]!);
    expect(beat.name).toBe("opening");
    expect(beat.contract.get("cast")!.value).toBe("Wren, Player");
    expect(beat.contract.get("setting")!.value).toBe("Lighthouse");
    expect(beat.body).toHaveLength(4);
    expect(beat.body[0]!.kind).toBe("action");
    const dialogue = dialogueOf(beat.body[1]!);
    expect(dialogue.speaker).toBe("WREN");
    expect(dialogue.parenthetical).toBe("quietly");
    expect(dialogue.lines).toHaveLength(1);
    const choiceItem = beat.body[2]!;
    if (choiceItem.kind !== "choice") throw new Error("expected choice");
    expect(choiceItem.value.text).toBe("Ring the bell.");
    expect(choiceItem.value.body).toHaveLength(1);
    const divert = divertOf(choiceItem.value.body[0]!);
    if (divert.kind !== "to") throw new Error("expected To divert");
    expect(divert.target.name).toBe("ringing");
    expect(divert.target.qualifier).toBeNull();
    expect(divert.target.knot).toBeNull();
  });

  it("treats wrapped dialogue as one line", () => {
    const src = `== opening
  NARRATOR
    Welcome to The Stack. Tonight, you choose a side. Tonight, you
    find out what the sides are.
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const beat = beatOf(file.items[0]!);
    const dialogue = dialogueOf(beat.body[0]!);
    expect(dialogue.lines).toHaveLength(1);
    const ln = dialogue.lines[0]!;
    expect(ln.kind).toBe("text");
    if (ln.kind === "text") {
      expect(ln.value.value).toBe(
        "Welcome to The Stack. Tonight, you choose a side. Tonight, you find out what the sides are.",
      );
    }
  });

  it("splits dialogue on a blank line", () => {
    const src = `== opening
  NARRATOR
    First beat of the speech.

    Second beat after a pause.
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const beat = beatOf(file.items[0]!);
    const dialogue = dialogueOf(beat.body[0]!);
    expect(dialogue.lines).toHaveLength(2);
  });

  it("handles ink-style suppression", () => {
    const [file] = parse('== opening\n* "Yes."[ I said firmly.]\n  -> END\n');
    const beat = beatOf(file.items[0]!);
    const choiceItem = beat.body[0]!;
    if (choiceItem.kind !== "choice") throw new Error("expected choice");
    expect(choiceItem.value.text).toBe('"Yes."');
    expect(choiceItem.value.suppressed).toBe(" I said firmly.");
  });

  it("parses a divert with parameters and qualifier", () => {
    const [file] = parse(
      "== opening\n* Ask.\n  -> Lighthouse/ringing with topic: bell, NPC: Wren\n",
    );
    const beat = beatOf(file.items[0]!);
    const choiceItem = beat.body[0]!;
    if (choiceItem.kind !== "choice") throw new Error("expected choice");
    const divert = divertOf(choiceItem.value.body[0]!);
    if (divert.kind !== "to") throw new Error("expected To divert");
    expect(divert.target.qualifier).toBe("Lighthouse");
    expect(divert.target.name).toBe("ringing");
    expect(divert.params.get("topic")).toBe("bell");
    expect(divert.params.get("NPC")).toBe("Wren");
  });

  it("keeps the declaration body raw", () => {
    const src = `CHARACTER Wren is Keeper, Combatant
  voice: female_alto
  hp: 80
  on meeting Player
    -> introduce
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const decl = declOf(file.items[0]!);
    expect(decl.kind).toBe("character");
    expect(decl.name).toBe("Wren");
    expect(decl.mixin).toEqual(["Keeper", "Combatant"]);
    expect(decl.body.length).toBeGreaterThanOrEqual(4);
  });

  it("treats a let binding as a top-level item", () => {
    const [file] = parse("let trusted = Wren.trusts.Player > 50\n");
    const item = file.items[0]!;
    expect(item.kind).toBe("letBinding");
    if (item.kind === "letBinding") {
      expect(item.value.name).toBe("trusted");
      expect(item.value.expression).toBe("Wren.trusts.Player > 50");
    }
  });

  it("groups conditional arms", () => {
    const src = `== opening

<if: trust > 50>
  WREN
    You may pass.
<else if: trust > 20>
  WREN
    Maybe later.
<else>
  WREN
    Leave. Now.
  -> END
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const beat = beatOf(file.items[0]!);
    const cond = beat.body[0]!;
    if (cond.kind !== "conditional") throw new Error("expected conditional");
    expect(cond.value.arms).toHaveLength(3);
    expect(cond.value.arms[0]!.condition).toBe("trust > 50");
    expect(cond.value.arms[1]!.condition).toBe("trust > 20");
    expect(cond.value.arms[2]!.condition).toBeNull();
    expect(cond.value.arms[2]!.body).toHaveLength(2);
  });

  it("collects an indented directive block body", () => {
    const [file] = parse(
      "== opening\n\n<broadcast: location(BellTower)>\n  WREN\n    Listen.\n",
    );
    const beat = beatOf(file.items[0]!);
    const block = beat.body[0]!;
    if (block.kind !== "directiveBlock") throw new Error("expected directive block");
    expect(block.value.directive.raw.startsWith("broadcast")).toBe(true);
    expect(block.value.body).toHaveLength(1);
  });

  it("keeps comments invisible to the parser", () => {
    const src = `// rough draft — pickup pace on the bell line
== opening
  cast: Wren, Player  // production: confirm with director

/* blocking sketch:
   Wren is upstage left at the rope.
   Player enters from SR on the bell.
*/

WREN
  (quietly)
  It hasn't rung in three days. // confirm pickup on \`rang\`
  -> ringing
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const beat = beatOf(file.items[0]!);
    expect(beat.name).toBe("opening");
    expect(beat.contract.get("cast")!.value).toBe("Wren, Player");
    const dialogue = dialogueOf(beat.body[0]!);
    expect(dialogue.speaker).toBe("WREN");
    const ln = dialogue.lines[0]!;
    if (ln.kind === "text") expect(ln.value.value).toBe("It hasn't rung in three days.");
    else throw new Error("expected text");
  });

  it("surfaces an unterminated block comment in parse", () => {
    const [, diags] = parse("/* never closed\n== opening\n");
    expect(diags.some((d) => d.code === Code.L1007UnterminatedBlockComment)).toBe(true);
  });

  it("lowers character disposition and knowledge", () => {
    const src = `CHARACTER Wren is Keeper
  voice: female_alto
  hp: 80
  trusts Player: 30 of 100
  respects Player: 50 of 100 mirror Player.respects.Wren
  reacts trust > 60 -> warm
  knows:
    met_player: bool = false
    bell_origin: unknown | suspects | confirmed = unknown
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const decl = declOf(file.items[0]!);
    const body = decl.character!;
    expect(body.properties.get("voice")!.value).toBe("female_alto");
    expect(body.disposition).toHaveLength(2);
    expect(body.disposition[0]!.verb).toBe("trusts");
    expect(body.disposition[0]!.target).toBe("Player");
    expect(body.disposition[0]!.current).toBe(30);
    expect(body.disposition[0]!.max).toBe(100);
    expect(body.disposition[1]!.mirror).toBe("Player.respects.Wren");
    expect(body.reacts).toHaveLength(1);
    expect(body.reacts[0]!.tag).toBe("warm");
    expect(body.knowledge).toHaveLength(2);
    expect(body.knowledge[0]!.name).toBe("met_player");
    expect(body.knowledge[0]!.default).toBe("false");
  });

  it("lowers a goal and threshold hook", () => {
    const src = `CHARACTER Wren
  goal find_keeper
    priority: 0.8
    active when: Time.hour > 6
    completes when: Wren.knows.saw_the_keeper
    drives: search_routine
  on trust passes 80
    -> reveal_secret as Wren
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const body = declOf(file.items[0]!).character!;
    expect(body.goals).toHaveLength(1);
    const g = body.goals[0]!;
    expect(g.name).toBe("find_keeper");
    expect(g.priority).toBe(0.8);
    expect(g.activeWhen).toBe("Time.hour > 6");
    expect(g.drives).toBe("search_routine");
    expect(body.hooks).toHaveLength(1);
    expect(body.hooks[0]!.event).toBe("trust passes 80");
    expect(body.hooks[0]!.body.length).toBeGreaterThan(0);
  });

  it("lowers STATS primitives", () => {
    const src = `STATS Combat
  attribute strength = 10, range 1 to 30
  axis level
    mode: xp_curve
    curve: level * level * 50
  pool health
    max: max_health
    regen: 2/s
  stat max_health = 50 + strength * 5
  stat damage = 8 + strength * 0.5
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const s = declOf(file.items[0]!).stats!;
    expect(s.attributes).toHaveLength(1);
    expect(s.attributes[0]!.name).toBe("strength");
    expect(s.attributes[0]!.default).toBe(10);
    expect(s.attributes[0]!.min).toBe(1);
    expect(s.attributes[0]!.max).toBe(30);
    expect(s.axes).toHaveLength(1);
    expect(s.axes[0]!.mode).toBe("xp_curve");
    expect(s.pools).toHaveLength(1);
    expect(s.pools[0]!.max).toBe("max_health");
    expect(s.stats).toHaveLength(2);
    expect(s.stats[1]!.name).toBe("damage");
  });

  it("lowers TREE nodes", () => {
    const src = `TREE WarriorPath
  node armsman_1
    cost: skill_points: 1
    requires: axis(one_handed) >= 20
    effect: stat(damage) += 5
  node armsman_2
    requires: node(armsman_1)
    effect: stat(damage) += 5
    effect: ability PowerAttack
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const t = declOf(file.items[0]!).tree!;
    expect(t.nodes).toHaveLength(2);
    expect(t.nodes[0]!.name).toBe("armsman_1");
    expect(t.nodes[1]!.effects).toHaveLength(2);
  });

  it("emits a diagnostic for an axis without mode", () => {
    const [, diags] = parse("STATS Combat\n  axis level\n    curve: x\n");
    expect(diags.some((d) => d.code === Code.L1110AxisMissingMode)).toBe(true);
  });

  it("parses an axis milestones list", () => {
    const src = `STATS Combat
  axis level
    mode: milestone
    milestones: tutorial, novice, adept, expert, master
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const axis = declOf(file.items[0]!).stats!.axes[0]!;
    expect(axis.mode).toBe("milestone");
    expect(axis.milestones).toEqual(["tutorial", "novice", "adept", "expert", "master"]);
  });

  it("lowers ITEM inherits and typed properties", () => {
    const src = `ITEM LootBag
  contents: list of ITEM = []
  gold:     int          = 0

ITEM goblin_pouch is LootBag
  contents: [rusty_dagger]
  gold:     3
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const bag = declOf(file.items[0]!).item!;
    expect(bag.inherits).toHaveLength(0);
    expect(bag.properties).toHaveLength(2);
    expect(bag.properties[0]!.name).toBe("contents");
    expect(bag.properties[0]!.slotType?.kind).toBe("listOf");
    expect(bag.properties[1]!.name).toBe("gold");
    expect(bag.properties[1]!.default).toBe("0");
    const pouch = declOf(file.items[1]!).item!;
    expect(pouch.inherits).toEqual(["LootBag"]);
  });

  it("lowers FACTION typed properties", () => {
    const src = `FACTION KeepersGuild
  members:    list of CHARACTER = []
  reputation: 0 to 100 = 50
  ledger:     map of CHARACTER to int = {}
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const f = declOf(file.items[0]!).faction!;
    expect(f.properties).toHaveLength(3);
    expect(f.properties[0]!.name).toBe("members");
    expect(f.properties[0]!.slotType?.kind).toBe("listOf");
    const range = f.properties[1]!.slotType;
    expect(range).toEqual({ kind: "range", lo: 0, hi: 100, default: 50 });
    expect(f.properties[2]!.slotType?.kind).toBe("mapOf");
  });

  it("captures any-shaped character slots", () => {
    const src = `CHARACTER Keeper
  voice: any
  home:  any of LOCATION
  reputation: 0 to 100 = 50
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const body = declOf(file.items[0]!).character!;
    const names = body.typedProperties.map((p) => p.name);
    expect(names).toContain("voice");
    expect(names).toContain("home");
    expect(names).toContain("reputation");
    const voice = body.typedProperties.find((p) => p.name === "voice")!;
    expect(voice.slotType?.kind).toBe("any");
    const home = body.typedProperties.find((p) => p.name === "home")!;
    expect(home.slotType).toEqual({ kind: "anyOf", name: "LOCATION" });
  });

  it("lowers a cohort capacity and label", () => {
    const src = `COHORT Initiates
  label: The Initiates
  capacity: 24
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const body = declOf(file.items[0]!).cohort!;
    expect(body.label).toBe("The Initiates");
    expect(body.capacity).toBe(24);
  });

  it("emits a diagnostic for a cohort without capacity", () => {
    const [, diags] = parse("COHORT Singers\n  label: The Singers\n");
    expect(diags.some((d) => d.code === Code.L1142CohortNoCapacity)).toBe(true);
  });

  it("lowers location ambient/contains/capacity", () => {
    const src = `LOCATION BellTower
  label: The Bell Tower
  ambient: bell-loop
  capacity: 8
  contains: Nave, Belfry
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const body = declOf(file.items[0]!).location!;
    expect(body.ambient).toBe("bell-loop");
    expect(body.capacity).toBe(8);
    expect(body.contains).toEqual(["Nave", "Belfry"]);
  });

  it("attaches improv parentheticals to dialogue", () => {
    const src = `== opening

BELLKEEPER
  (improv duration: 45s, advance on: any [pedal, speech(anchor phrase), gesture(Bow)])
  (Greet warmly.)
  -> next_beat
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const beat = beatOf(file.items[0]!);
    const dialogue = dialogueOf(beat.body[0]!);
    const improv = dialogue.improv!;
    expect(improv.duration!.value).toBe(45);
    expect(improv.duration!.unit).toBe("seconds");
    expect(improv.advanceOn).toHaveLength(3);
    expect(improv.quorum).toEqual({ kind: "any" });
    expect(improv.advanceOn[0]).toEqual({ kind: "pedal" });
    expect(improv.advanceOn[1]).toEqual({ kind: "speech", anchor: "anchor phrase" });
    expect(dialogue.parenthetical).toBe("Greet warmly.");
  });

  it("keeps a multiline stage direction as one dialogue block", () => {
    const src = `== arrival

VEX | PRAXIS
  (improv duration: 60s, advance on: quorum(8) [speech(go), gesture(Cue)])
  (Argue about the city. Vex pitches reform; Praxis pitches order.
   Pull individual audience members into your camp by talking
   directly to them.)
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const beat = beatOf(file.items[0]!);
    expect(beat.body).toHaveLength(1);
    const dialogue = dialogueOf(beat.body[0]!);
    expect(dialogue.speakers).toEqual(["VEX", "PRAXIS"]);
    expect(dialogue.improv).not.toBeNull();
    expect(dialogue.parenthetical).toBe(
      "Argue about the city. Vex pitches reform; Praxis pitches order. Pull individual audience members into your camp by talking directly to them.",
    );
    expect(dialogue.lines).toHaveLength(0);
  });

  it("parses improv quorum(N)", () => {
    const src = `== opening

WREN
  (improv duration: 30s, advance on: quorum(2) [pedal, gesture(Bow)])
  -> END
`;
    const [file] = parse(src);
    const beat = beatOf(file.items[0]!);
    const dialogue = dialogueOf(beat.body[0]!);
    expect(dialogue.improv!.quorum).toEqual({ kind: "n", value: 2 });
  });

  it("emits a diagnostic for improv missing duration", () => {
    const src = `== opening

WREN
  (improv advance on: any [pedal])
  -> END
`;
    const [, diags] = parse(src);
    expect(diags.some((d) => d.code === Code.L1140ImprovMissingDuration)).toBe(true);
  });

  it("parses a scene with multiple states", () => {
    const src = `SCENE investigate(character)
  approach
    wait until character.at(Player.position)
    -> examine

  examine
    wait until character.deduction > 60
    -> confront

  confront
    return clue
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const decl = declOf(file.items[0]!);
    expect(decl.name).toBe("investigate");
    const scene = decl.scene!;
    expect(scene.params).toEqual(["character"]);
    expect(scene.states).toHaveLength(3);
    expect(scene.states[0]!.name).toBe("approach");
    expect(scene.states[2]!.name).toBe("confront");
  });

  it("parses a top-level generator with tier and priority", () => {
    const src = `GENERATOR HarborChorus
  tier:     ambient
  priority: 0.3

  loop
    wait random(20s, 60s)
    yield bark from Quiet night. | Stars are out. | Tide's calm.
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const gen = declOf(file.items[0]!).generator!;
    expect(gen.tier).toBe("ambient");
    expect(gen.priority).toBe(0.3);
    expect(gen.body.some((l) => l.text.trim().startsWith("yield bark from"))).toBe(true);
  });

  it("diagnoses an empty generator body", () => {
    const [, diags] = parse("GENERATOR Empty\n  tier: ambient\n");
    expect(diags.some((d) => d.code === Code.L1131GeneratorMissingBody)).toBe(true);
  });

  it("collects a metadata fence", () => {
    const [file] = parse("== opening\n```note\nThis felt long.\n```\n");
    const beat = beatOf(file.items[0]!);
    const meta = beat.body[0]!;
    if (meta.kind !== "metadata") throw new Error("expected metadata");
    expect(meta.value.value).toContain("note");
    expect(meta.value.value).toContain("This felt long.");
  });

  it("collects match arms", () => {
    const src = `== opening

<match: NPC.knows.bell_origin>
  confirmed
    NPC
      I know.
  suspects
    NPC
      A hunch.
  unknown
    NPC
      I do not know.
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const beat = beatOf(file.items[0]!);
    const m = beat.body[0]!;
    if (m.kind !== "match") throw new Error("expected match");
    expect(m.value.scrutinee).toBe("NPC.knows.bell_origin");
    expect(m.value.arms).toHaveLength(3);
    expect(m.value.arms[0]!.pattern).toBe("confirmed");
    expect(m.value.arms[2]!.pattern).toBe("unknown");
    expect(m.value.arms[0]!.body).toHaveLength(1);
  });

  it("parses each-visit with three arms", () => {
    const src = `== opening

<each visit>
  first
    First time prose.
  then
    Subsequent prose.
  finally
    After visits.
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const beat = beatOf(file.items[0]!);
    const each = beat.body[0]!;
    if (each.kind !== "eachVisit") throw new Error("expected each visit");
    expect(each.value.first).toHaveLength(1);
    expect(each.value.then).toHaveLength(1);
    expect(each.value.finally).toHaveLength(1);
  });

  it("parses an after/otherwise pair", () => {
    const src = `== opening

<after: bell_rung>
  WREN
    You rang it.
<otherwise>
  WREN
    Not yet.
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const beat = beatOf(file.items[0]!);
    const morph = beat.body[0]!;
    if (morph.kind !== "afterMorph") throw new Error("expected after-morph");
    expect(morph.value.condition).toBe("bell_rung");
    expect(morph.value.after).toHaveLength(1);
    expect(morph.value.otherwise).toHaveLength(1);
  });

  it("lands the beat param list on beat.params", () => {
    const [file, diags] = parse("== ask_about(topic, NPC)\n  cast: Wren\n\nDone.\n");
    expect(diags).toHaveLength(0);
    const beat = beatOf(file.items[0]!);
    expect(beat.name).toBe("ask_about");
    expect(beat.params).toEqual(["topic", "NPC"]);
  });

  it("captures a divert answer-slot fill", () => {
    const src = `== opening

* Ask.
  -> ask_about with topic: bell, NPC: Wren
    answer:
      WREN
        The bell rings when the keeper is in danger.
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const beat = beatOf(file.items[0]!);
    const choiceItem = beat.body[0]!;
    if (choiceItem.kind !== "choice") throw new Error("expected choice");
    const divert = divertOf(choiceItem.value.body[0]!);
    if (divert.kind !== "to") throw new Error("expected To divert");
    expect(divert.slots.size).toBe(1);
    const answer = divert.slots.get("answer")!;
    expect(answer).toHaveLength(1);
    expect(answer[0]!.kind).toBe("dialogue");
  });

  it("recognises a slot placeholder inside a beat body", () => {
    const src = `== ask_about(topic)
  cast: NPC, Player

slot: answer
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const beat = beatOf(file.items[0]!);
    const slot = beat.body[0]!;
    if (slot.kind !== "slotPlaceholder") throw new Error("expected slot placeholder");
    expect(slot.value.name).toBe("answer");
  });

  it("parses an inline-let directive", () => {
    const src = `== opening
<let: x = 42>

Done.
`;
    const [file, diags] = parse(src);
    expect(diags).toHaveLength(0);
    const beat = beatOf(file.items[0]!);
    const letItem = beat.body[0]!;
    if (letItem.kind !== "inlineLet") throw new Error("expected inline-let");
    expect(letItem.value.name).toBe("x");
    expect(letItem.value.expression).toBe("42");
  });
});
