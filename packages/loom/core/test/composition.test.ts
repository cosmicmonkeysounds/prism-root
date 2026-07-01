//! Slice 1 of the functional redesign (docs/dev/loom-functional-redesign.md):
//! the `is`-merge is now actually run on the sim compile path, ROLEs may mix
//! in traits (they are schemas, never "abstract"), and the `SELF`/`ME`
//! dialogue speaker resolves to whoever `self` is bound to.

import { describe, expect, it } from "vitest";
import { parse } from "../src/parser/index.ts";
import { Bundle, type LoomFileEntry } from "../src/runtime/index.ts";
import { compileModel } from "../src/runtime/sim/model.ts";
import { Sim } from "../src/runtime/sim/index.ts";
import type { SimEvent } from "../src/runtime/sim/index.ts";

function bundleOf(...sources: string[]): Bundle {
  const b = new Bundle();
  sources.forEach((src, i) => {
    const [file, diagnostics] = parse(src);
    const entry: LoomFileEntry = {
      path: `f${i}.loom`,
      stem: `f${i}`,
      qualifier: "",
      source: src,
      file,
      diagnostics,
    };
    b.files.push(entry);
  });
  return b;
}

function bundleFrom(...sources: string[]): Bundle {
  const b = bundleOf(...sources);
  b.rebuildSimulacra();
  return b;
}

/** Bundle after a full `compileModel` — surfaces model-time diagnostics. */
function compileBundle(...sources: string[]): Bundle {
  const b = bundleOf(...sources);
  compileModel(b);
  return b;
}

// A prop that wears a faction *badge* trait and, on scan, routes to a beat
// whose speaker is the universal `SELF` — the shape the redesign turns the
// repetitive scanner props into.
const SRC = `entry: start

FACTION TheAlgorithm
  ethos: control

LOCATION Party
  label: The Party

TRAIT Algo
  faction: TheAlgorithm

ROLE Guest
  faction: any of FACTION
  heat: 0 to 100 = 0

CHARACTER Crawler is Algo
  on scan guest
    -> report

== start
  setting: Party
  NARRATOR
    Doors open.

== report(guest)
  cast: Crawler, guest
  SELF
    Indexing. Flag raised.
  <set: guest.heat += 10>
`;

function dialogue(events: SimEvent[]): Array<{ speaker: string; text: string }> {
  return events
    .filter((e) => e.type === "dialogue")
    .map((e) => e as unknown as { speaker: string; text: string });
}

describe("Slice 1 — `is` wiring", () => {
  it("resolves a badge trait's faction onto the character at sim time", () => {
    // Before the wiring fix `mergedCharacters` was never populated on the sim
    // path, so `is Algo` was inert and Crawler's faction was null.
    const sim = Sim.fromSources(SRC);
    expect(sim.model.characters.get("Crawler")!.faction).toBe("TheAlgorithm");
    // The identity var the world seeds from the merged faction is present too.
    expect(sim.world.get("Crawler.faction")).toEqual({ kind: "string", value: "TheAlgorithm" });
  });

  it("still resolves the character's own hook after mixing in a trait", () => {
    const sim = Sim.fromSources(SRC);
    sim.createPerson("g1", "Ada");
    sim.scan("Crawler", "g1");
    // The `on scan guest -> report` hook fired and the divert body ran.
    expect(sim.world.get("g1.heat")).toEqual({ kind: "number", value: 10 });
  });
});

describe("Slice 1 — ROLE mixes in traits (never dropped as abstract)", () => {
  it("keeps a ROLE with an `any of` slot in mergedCharacters", () => {
    const b = bundleFrom(`ROLE Guest
  faction: any of FACTION
  heat: 0 to 100 = 0
`);
    expect(b.mergedCharacters.has("Guest")).toBe(true);
    expect(
      b.projectDiagnostics.some(
        (d) => d.kind === "requiredSlotUnfilled" && d.character === "Guest",
      ),
    ).toBe(false);
  });

  it("inherits a trait's hook onto a ROLE via `is`", () => {
    const b = bundleFrom(`TRAIT Pinged
  on betray
    <broadcast: agent_made to participant(self)>

ROLE Guest is Pinged
  faction: any of FACTION
  heat: 0 to 100 = 0
`);
    expect(b.mergedCharacters.has("Guest")).toBe(true);
    const guest = b.mergedCharacters.get("Guest")!;
    expect(guest.hooks.some((h) => h.event.trim() === "betray")).toBe(true);
  });

  it("a CHARACTER with an unfilled `any` slot is still flagged abstract", () => {
    // The role exemption must not weaken the check for real characters.
    const b = bundleFrom(`CHARACTER Keeper
  voice: any
`);
    expect(b.mergedCharacters.has("Keeper")).toBe(false);
    expect(
      b.projectDiagnostics.some(
        (d) => d.kind === "requiredSlotUnfilled" && d.character === "Keeper",
      ),
    ).toBe(true);
  });
});

describe("Slice 1 — SELF speaker", () => {
  it("attributes a SELF block to the scanning prop, upper-cased", () => {
    const sim = Sim.fromSources(SRC);
    sim.createPerson("g1", "Ada");
    const events = sim.scan("Crawler", "g1");
    const lines = dialogue(events);
    expect(lines).toHaveLength(1);
    expect(lines[0]!.speaker).toBe("CRAWLER");
    expect(lines[0]!.text).toBe("Indexing. Flag raised.");
  });

  it("upper-cases an underscored id to match explicit speaker casing", () => {
    const sim = Sim.fromSources(`FACTION F
  ethos: x

LOCATION Party
  label: P

ROLE Guest
  faction: any of FACTION

CHARACTER Cookie_Banner
  faction: F
  on scan guest
    -> consent

== consent(guest)
  cast: Cookie_Banner, guest
  SELF
    Do you accept all cookies?
`);
    sim.createPerson("g1", "Ada");
    const lines = dialogue(sim.scan("Cookie_Banner", "g1"));
    expect(lines[0]!.speaker).toBe("COOKIE_BANNER");
  });
});

describe("Slice 1 — rebuildSimulacra idempotency", () => {
  it("does not double-report abstractness diagnostics on re-run", () => {
    const b = bundleFrom(`CHARACTER Keeper
  voice: any
`);
    const first = b.projectDiagnostics.length;
    expect(first).toBe(1);
    b.rebuildSimulacra();
    expect(b.projectDiagnostics.length).toBe(first);
  });
});

// ── Slice 2: parameterized traits ────────────────────────────────────
// `TRAIT Scanner(beat)` + `is AlgoScanner(crawler_report)` collapses the
// repetitive scanner props to one line each.
const PARAM_SRC = `entry: start

FACTION TheAlgorithm
  ethos: control

LOCATION Party
  label: The Party

ROLE Guest
  faction: any of FACTION
  heat: 0 to 100 = 0

TRAIT Scanner(beat)
  on scan guest
    -> self.beat

TRAIT Algo
  faction: TheAlgorithm

TRAIT AlgoScanner(beat) is Scanner(beat), Algo

CHARACTER Crawler is AlgoScanner(crawler_report)
CHARACTER Captcha is AlgoScanner(captcha_gate)

== start
  setting: Party
  NARRATOR
    Doors.

== crawler_report(guest)
  cast: Crawler, guest
  SELF
    Indexing.
  <set: guest.heat += 40>

== captcha_gate(guest)
  cast: Captcha, guest
  SELF
    Prove you are human.
  <set: guest.heat += 5>
`;

describe("Slice 2 — parameterized traits", () => {
  it("collapses a scanner prop: forwards the beat through a badge+router trait", () => {
    const sim = Sim.fromSources(PARAM_SRC);
    // `is AlgoScanner(...)` forwards the beat through `Scanner(beat)` and
    // pulls the faction from `Algo` — all on one line.
    expect(sim.model.characters.get("Crawler")!.faction).toBe("TheAlgorithm");
    const hooks = sim.model.characters.get("Crawler")!.hooks;
    expect(hooks).toHaveLength(1);
    expect(hooks[0]!.verb).toBe("scan");

    sim.createPerson("g1", "Ada");
    const ev = sim.scan("Crawler", "g1");
    expect(sim.world.get("g1.heat")).toEqual({ kind: "number", value: 40 });
    const line = dialogue(ev)[0]!;
    expect(line.speaker).toBe("CRAWLER");
    expect(line.text).toBe("Indexing.");
  });

  it("routes two appliers of one trait to distinct beats (no cache corruption)", () => {
    const sim = Sim.fromSources(PARAM_SRC);
    sim.createPerson("g1", "A");
    sim.createPerson("g2", "B");
    sim.scan("Crawler", "g1");
    sim.scan("Captcha", "g2");
    // If the deep-clone were missing, the second applier would inherit the
    // first's substituted `-> crawler_report` and g2 would take +40.
    expect(sim.world.get("g1.heat")).toEqual({ kind: "number", value: 40 });
    expect(sim.world.get("g2.heat")).toEqual({ kind: "number", value: 5 });
  });

  it("substitutes an event param into the hook trigger (named args)", () => {
    const b = bundleFrom(`TRAIT Pinged(event, signal)
  on self.event
    <broadcast: self.signal to participant(self)>

CHARACTER Mole is Pinged(event: betray, signal: agent_made)
`);
    const mole = b.mergedCharacters.get("Mole")!;
    expect(mole.hooks).toHaveLength(1);
    expect(mole.hooks[0]!.event.trim()).toBe("betray");
    const body = mole.hooks[0]!.body.map((l) => l.text).join("\n");
    expect(body).toContain("agent_made");
    expect(body).not.toContain("self.signal");
    // Bare `self` (in `participant(self)`) must survive — only `self.<param>`
    // is a substitution site.
    expect(body).toContain("participant(self)");
  });

  it("keeps a multi-word arg value intact through the top-level comma split", () => {
    const b = bundleFrom(`TRAIT Pinged(event, signal)
  on self.event
    <broadcast: self.signal to participant(self)>

CHARACTER Sentinel is Pinged(event: enters Internet, signal: lockdown)
`);
    const s = b.mergedCharacters.get("Sentinel")!;
    // The comma inside the args is NOT a separator between applications, and
    // `enters Internet` survives as one value.
    expect(s.hooks[0]!.event.trim()).toBe("enters Internet");
  });

  it("reports an unfilled trait param", () => {
    const b = bundleFrom(`TRAIT Scanner(beat)
  on scan guest
    -> self.beat

CHARACTER Crawler is Scanner
`);
    expect(
      b.projectDiagnostics.some(
        (d) =>
          d.kind === "requiredParamUnfilled" &&
          d.character === "Crawler" &&
          d.param === "beat",
      ),
    ).toBe(true);
  });

  it("parses a multi-arg trait application as a single mixin entry", () => {
    const [file] = parse(`CHARACTER Sentinel is CellWatch(loc: Internet, signal: lockdown)\n`);
    const decl = file.items.find((i) => i.kind === "declaration")!;
    expect(decl.kind).toBe("declaration");
    if (decl.kind !== "declaration") throw new Error("unreachable");
    expect(decl.value.mixin).toEqual(["CellWatch(loc: Internet, signal: lockdown)"]);
  });

  it("carries trait params onto the lowered CharacterBody", () => {
    const [file] = parse(`TRAIT Scanner(beat)\n  on scan guest\n    -> self.beat\n`);
    const decl = file.items.find((i) => i.kind === "declaration")!;
    if (decl.kind !== "declaration") throw new Error("unreachable");
    expect(decl.value.name).toBe("Scanner");
    expect(decl.value.character!.params).toEqual(["beat"]);
  });
});

// ── Part II Slice A: class-owned beats + qualified diverts ────────────
const OWNED_SRC = `entry: start

FACTION F
  ethos: order

LOCATION Party
  label: The Party

ROLE Guest
  faction: any of FACTION
  heat: 0 to 100 = 0
  name: text?

CHARACTER Alpha
  faction: F
  on scan guest
    -> self.report
  beat report(guest)
    SELF
      Alpha here.
    <set: guest.heat += 1>

CHARACTER Beta
  faction: F
  on scan guest
    -> self.report
  beat report(guest)
    SELF
      Beta here.
    <set: guest.heat += 100>

CHARACTER Caller
  faction: F
  on scan guest
    -> Alpha.report

CHARACTER Ghost
  faction: F
  on scan guest
    -> self.missing

CHARACTER Funnel
  faction: F
  on scan guest
    -> lockdown

CHARACTER Oracle
  faction: F
  on scan guest
    <if: visits(self.prophecy) == 0>
      -> self.prophecy
    <else>
      -> self.riddle
  beat prophecy(guest)
    SELF
      First vision.
    <set: guest.heat += 1>
  beat riddle(guest)
    SELF
      A riddle.
    <set: guest.heat += 10>

== start
  setting: Party
  NARRATOR
    Go.

== lockdown(guest)
  <set: guest.heat += 999>
`;

describe("Slice A — owned beats", () => {
  it("namespaces same-named owned beats under Owner.name (no collision)", () => {
    const sim = Sim.fromSources(OWNED_SRC);
    expect(sim.model.beats.has("Alpha.report")).toBe(true);
    expect(sim.model.beats.has("Beta.report")).toBe(true);

    sim.createPerson("g1", "Ada");
    sim.createPerson("g2", "Bo");
    const a = sim.scan("Alpha", "g1");
    const b = sim.scan("Beta", "g2");
    expect(sim.world.get("g1.heat")).toEqual({ kind: "number", value: 1 });
    expect(sim.world.get("g2.heat")).toEqual({ kind: "number", value: 100 });
    expect(dialogue(a)[0]).toMatchObject({ speaker: "ALPHA", text: "Alpha here." });
    expect(dialogue(b)[0]).toMatchObject({ speaker: "BETA", text: "Beta here." });
  });

  it("routes `-> self.beat` to the owner's inline beat", () => {
    const sim = Sim.fromSources(OWNED_SRC);
    sim.createPerson("g1", "Ada");
    const ev = sim.scan("Alpha", "g1");
    expect(dialogue(ev)[0]!.text).toBe("Alpha here.");
  });

  it("rebinds SELF to the owner on a cross-owner `-> Owner.beat` divert", () => {
    const sim = Sim.fromSources(OWNED_SRC);
    sim.createPerson("g3", "Cy");
    const ev = sim.scan("Caller", "g3");
    // Caller diverts to Alpha.report — it runs and speaks as ALPHA, not CALLER.
    expect(dialogue(ev)[0]).toMatchObject({ speaker: "ALPHA", text: "Alpha here." });
    expect(sim.world.get("g3.heat")).toEqual({ kind: "number", value: 1 });
  });

  it("keeps a bare divert global (back-compat)", () => {
    const sim = Sim.fromSources(OWNED_SRC);
    sim.createPerson("g4", "Di");
    sim.scan("Funnel", "g4");
    expect(sim.world.get("g4.heat")).toEqual({ kind: "number", value: 999 });
  });

  it("diagnoses a qualified divert that resolves to nothing", () => {
    const sim = Sim.fromSources(OWNED_SRC);
    sim.createPerson("g5", "Ev");
    const ev = sim.scan("Ghost", "g5");
    expect(ev.some((e) => e.type === "diagnostic")).toBe(true);
    // Nothing ran, heat untouched.
    expect(sim.world.get("g5.heat")).toEqual({ kind: "number", value: 0 });
  });

  it("resolves visits() owner-first for an owned beat", () => {
    const sim = Sim.fromSources(OWNED_SRC);
    sim.createPerson("g6", "Fi");
    sim.scan("Oracle", "g6"); // visits(self.prophecy)==0 → prophecy (+1)
    expect(sim.world.get("g6.heat")).toEqual({ kind: "number", value: 1 });
    sim.scan("Oracle", "g6"); // now visits==1 → riddle (+10)
    expect(sim.world.get("g6.heat")).toEqual({ kind: "number", value: 11 });
  });
});

// ── Slice 3: robustness niceties ─────────────────────────────────────
describe("Slice 3 — CHARACTER slot defaults", () => {
  it("seeds a CHARACTER's typed-slot defaults into the world", () => {
    const sim = Sim.fromSources(`FACTION F
  ethos: control

ROLE Guest
  faction: any of FACTION

CHARACTER Admin
  faction: F
  captures: 0 to 100 = 0
`);
    // With `self.captures`, the slot is defined (0), not implicitly-zero.
    expect(sim.world.get("Admin.captures")).toEqual({ kind: "number", value: 0 });
  });
});

describe("Slice 3 — inline opener divert", () => {
  it("routes `on scan guest -> beat` on the opener line", () => {
    const sim = Sim.fromSources(`entry: start

FACTION F
  ethos: control

LOCATION Party
  label: The Party

ROLE Guest
  faction: any of FACTION
  heat: 0 to 100 = 0

CHARACTER Prop
  faction: F
  on scan guest -> zap

== start
  setting: Party
  NARRATOR
    Go.

== zap(guest)
  <set: guest.heat += 7>
`);
    sim.createPerson("g1", "A");
    sim.scan("Prop", "g1");
    expect(sim.world.get("g1.heat")).toEqual({ kind: "number", value: 7 });
  });
});

describe("Slice 3 — UnterminatedMixinClause", () => {
  it("flags a wrapped is-clause", () => {
    const [, diags] = parse(`CHARACTER X is Foo(a,\n`);
    expect(diags.some((d) => d.code === "L1008")).toBe(true);
  });

  it("does NOT flag a balanced multi-arg application", () => {
    const [, diags] = parse(`CHARACTER X is CellWatch(loc: Internet, signal: lockdown)\n`);
    expect(diags.some((d) => d.code === "L1008")).toBe(false);
  });
});

// ── Review fixes: 4 confirmed bugs from the adversarial pass ──────────
describe("review fixes", () => {
  it("keys a cross-owner divert's visit under the caller, so visits() agrees (bug #1)", () => {
    // No participant is bound (a self-only `on lockdown` cue). The record used
    // the rebound owner as subject while visits() used the caller — so a beat
    // just entered read as 0 visits. Both must select the same subject.
    const sim = Sim.fromSources(`FACTION F
  ethos: order

ROLE Guest
  faction: any of FACTION

CHARACTER Oracle
  faction: F
  beat prophecy
    NARRATOR
      Doom is near.

CHARACTER Seer
  faction: F
  on lockdown
    -> Oracle.prophecy
    <if: visits(Oracle.prophecy) > 0>
      <set: Oracle.foretold = true>
`);
    sim.signal("lockdown");
    expect(sim.world.get("Oracle.foretold")).toEqual({ kind: "bool", value: true });
  });

  it("routes a trait param to the applier's OWNED beat, not a bare global (bug #2)", () => {
    const sim = Sim.fromSources(`entry: start

FACTION F
  ethos: order

LOCATION Party
  label: The Party

ROLE Guest
  faction: any of FACTION
  heat: 0 to 100 = 0

TRAIT Watcher(target)
  on scan guest
    -> self.target

CHARACTER Guard is Watcher(prophecy)
  faction: F
  beat prophecy(guest)
    SELF
      You are caught.
    <set: guest.heat += 5>

== start
  setting: Party
  NARRATOR
    Go.
`);
    expect(sim.model.beats.has("Guard.prophecy")).toBe(true);
    sim.createPerson("g1", "A");
    const ev = sim.scan("Guard", "g1");
    // The trait param `prophecy` names Guard's OWN beat, so `-> self.target`
    // resolves to Guard.prophecy — not a (nonexistent) global `prophecy`.
    expect(sim.world.get("g1.heat")).toEqual({ kind: "number", value: 5 });
    expect((ev.find((e) => e.type === "dialogue") as { speaker: string }).speaker).toBe("GUARD");
  });

  it("fills positional args after a named one in order (bug #3)", () => {
    const b = bundleFrom(`TRAIT Pair(a, b)
  voice: self.a
  mood: self.b

CHARACTER NPC is Pair(a: first, second)
`);
    const npc = b.mergedCharacters.get("NPC")!;
    expect(npc.properties.get("voice")!.value).toBe("first");
    expect(npc.properties.get("mood")!.value).toBe("second");
    expect(b.projectDiagnostics.some((d) => d.kind === "requiredParamUnfilled")).toBe(false);
  });

  it("inherits a trait-shipped beat, namespaced per deriver (bug #4)", () => {
    const sim = Sim.fromSources(`entry: start

FACTION F
  ethos: order

LOCATION Party
  label: The Party

ROLE Guest
  faction: any of FACTION
  heat: 0 to 100 = 0

TRAIT Greeter
  beat greet(guest)
    SELF
      Hello.
    <set: guest.heat += 7>

CHARACTER Alpha is Greeter
  faction: F
  on scan guest
    -> self.greet

CHARACTER Beta is Greeter
  faction: F
  on scan guest
    -> self.greet

== start
  setting: Party
  NARRATOR
    Go.
`);
    expect(sim.model.beats.has("Alpha.greet")).toBe(true);
    expect(sim.model.beats.has("Beta.greet")).toBe(true);
    sim.createPerson("g1", "A");
    const ev = sim.scan("Alpha", "g1");
    expect(sim.world.get("g1.heat")).toEqual({ kind: "number", value: 7 });
    expect((ev.find((e) => e.type === "dialogue") as { speaker: string }).speaker).toBe("ALPHA");
  });
});

// ── Slice C: derived beat templates (slot / fill) ────────────────────
const GATEKEEPER_SRC = `entry: start

FACTION F
  ethos: order

LOCATION Party
  label: The Party

LOCATION Internet
  label: The Internet
  prison: true

ROLE Guest
  faction: any of FACTION
  captured: bool = false
  heat: 0 to 100 = 0

# The captured-vs-free gate SHAPE, authored once; each deriver fills the holes.
TRAIT Gatekeeper
  beat confront(guest)
    SELF
      <if: guest.captured>
        slot: pitch
      <else>
        slot: dismissal

CHARACTER Booth is Gatekeeper
  faction: F
  on scan guest
    -> self.confront
  fill pitch
    Talk, {guest.name}.
  fill dismissal
    Move along.

CHARACTER Bouncer is Gatekeeper
  faction: F
  on scan guest
    -> self.confront
  fill pitch
    List's closed, {guest.name}.
  fill dismissal
    Not you.

== start
  setting: Party
  NARRATOR
    Go.
`;

describe("Slice C — derived beat templates", () => {
  it("fills one template per deriver, distinctly, with no cross-corruption", () => {
    const sim = Sim.fromSources(GATEKEEPER_SRC);
    expect(sim.model.beats.has("Booth.confront")).toBe(true);
    expect(sim.model.beats.has("Bouncer.confront")).toBe(true);

    sim.createPerson("g1", "Ada");
    sim.capture("g1");
    const a = dialogue(sim.scan("Booth", "g1"))
      .map((l) => l.text)
      .join(" ");
    expect(a).toContain("Talk, Ada");

    sim.createPerson("g2", "Bo");
    sim.capture("g2");
    const b = dialogue(sim.scan("Bouncer", "g2"))
      .map((l) => l.text)
      .join(" ");
    expect(b).toContain("List's closed, Bo");
    // The deriver-specific fill must not leak across: Bouncer never speaks
    // Booth's line (the deep-clone / per-registration fill guarantee).
    expect(b).not.toContain("Talk");
  });

  it("speaks the free-branch fill for an uncaptured guest", () => {
    const sim = Sim.fromSources(GATEKEEPER_SRC);
    sim.createPerson("g1", "Ada");
    const free = dialogue(sim.scan("Booth", "g1"))
      .map((l) => l.text)
      .join(" ");
    expect(free).toContain("Move along.");
    expect(free).not.toContain("Talk, Ada");
  });

  it("reports an unfilled derived slot", () => {
    const b = compileBundle(`FACTION F
  ethos: order

ROLE Guest
  faction: any of FACTION

TRAIT Gate
  beat g(guest)
    SELF
      slot: line

CHARACTER Lonely is Gate
  faction: F
  on scan guest
    -> self.g
`);
    expect(
      b.projectDiagnostics.some(
        (d) =>
          d.kind === "unfilledDerivedSlot" && d.character === "Lonely" && d.slot === "line",
      ),
    ).toBe(true);
  });

  it("flags two parents shipping the same beat name (DerivedBeatConflict)", () => {
    const b = bundleFrom(`TRAIT A
  beat x(g)
    SELF
      From A.

TRAIT B
  beat x(g)
    SELF
      From B.

CHARACTER C is A, B
`);
    expect(
      b.projectDiagnostics.some(
        (d) => d.kind === "derivedBeatConflict" && d.character === "C" && d.beat === "x",
      ),
    ).toBe(true);
  });

  it("treats `slot:` as a placeholder but bare `slot …` as prose", () => {
    const [file] = parse(`== b(guest)\n  SELF\n    slot: pitch\n    slot machines line the wall\n`);
    const beat = file.items.find((i) => i.kind === "beat")!;
    if (beat.kind !== "beat") throw new Error("unreachable");
    const dlg = beat.value.body.find((bi) => bi.kind === "dialogue");
    if (dlg?.kind !== "dialogue") throw new Error("no dialogue block");
    const kinds = dlg.value.body.map((bi) => bi.kind);
    expect(kinds).toContain("slotPlaceholder");
    // The colon-less `slot …` line is prose, not a swallowed placeholder.
    const action = dlg.value.body.find((bi) => bi.kind === "action");
    expect(action && action.kind === "action" ? action.value.value : null).toBe(
      "slot machines line the wall",
    );
  });

  it("fills a LEADING slot — the first body line of a template (review bug #1)", () => {
    // A `slot:` at the top of a beat body was swallowed as beat frontmatter by
    // the re-parse in lowerRawBody, so the fill was silently dropped.
    const sim = Sim.fromSources(`entry: start

FACTION F
  ethos: order

LOCATION Party
  label: The Party

ROLE Guest
  faction: any of FACTION

TRAIT Gate
  beat greet(guest)
    slot: line

CHARACTER Booth is Gate
  faction: F
  on scan guest
    -> self.greet
  fill line
    You shall not pass.

== start
  setting: Party
  NARRATOR
    Go.
`);
    expect(sim.model.beats.get("Booth.greet")!.body.length).toBeGreaterThan(0);
    sim.createPerson("g1", "A");
    const ev = sim.scan("Booth", "g1");
    const texts = ev
      .filter((e) => e.type === "action" || e.type === "dialogue")
      .map((e) => (e as { text: string }).text);
    expect(texts.join(" ")).toContain("You shall not pass.");
  });

  it("does not flag a diamond as a beat conflict (review bug #2)", () => {
    // One authored beat reaching a deriver through two intermediates is fine;
    // only two DISTINCT authored beats of one name collide.
    const b = bundleFrom(`TRAIT Base
  beat greet(g)
    SELF
      Hi.

TRAIT Left is Base

TRAIT Right is Base

CHARACTER Hero is Left, Right
`);
    expect(b.projectDiagnostics.some((d) => d.kind === "derivedBeatConflict")).toBe(false);
    expect(b.mergedCharacters.get("Hero")!.beats.some((bt) => bt.name === "greet")).toBe(true);
  });
});

// ── Remaining gaps: beat `super`, UnresolvedTraitArg, SELF fallback ───
describe("Gap A — beat-level super", () => {
  it("splices the parent template where a re-declared beat says `super`", () => {
    const sim = Sim.fromSources(`entry: start

FACTION F
  ethos: order

LOCATION Party
  label: The Party

ROLE Guest
  faction: any of FACTION

TRAIT Base
  beat greet(guest)
    SELF
      Base line.

CHARACTER Child is Base
  faction: F
  on scan guest
    -> self.greet
  beat greet(guest)
    SELF
      Child prefix.
    super

== start
  setting: Party
  NARRATOR
    Go.
`);
    sim.createPerson("g1", "A");
    const texts = dialogue(sim.scan("Child", "g1")).map((l) => l.text);
    // The override's own line, then the parent template spliced in by `super`.
    expect(texts).toContain("Child prefix.");
    expect(texts).toContain("Base line.");
  });
});

describe("Gap B — UnresolvedTraitArg", () => {
  it("flags a divert-only arg that names no beat", () => {
    const b = bundleFrom(`TRAIT Scanner(beat)
  on scan guest
    -> self.beat

CHARACTER Crawler is Scanner(nope)

== real
  NARRATOR
    hi
`);
    expect(
      b.projectDiagnostics.some(
        (d) =>
          d.kind === "unresolvedTraitArg" && d.character === "Crawler" && d.arg === "nope",
      ),
    ).toBe(true);
  });

  it("does not flag when the arg IS a real beat", () => {
    const b = bundleFrom(`TRAIT Scanner(beat)
  on scan guest
    -> self.beat

CHARACTER Crawler is Scanner(real)

== real
  NARRATOR
    hi
`);
    expect(b.projectDiagnostics.some((d) => d.kind === "unresolvedTraitArg")).toBe(false);
  });

  it("flags a bad arg forwarded through a combined trait (transitive)", () => {
    const b = bundleFrom(`TRAIT Scanner(beat)
  on scan guest
    -> self.beat

TRAIT AlgoScanner(beat) is Scanner(beat)

CHARACTER Crawler is AlgoScanner(nope)

== real
  NARRATOR
    hi
`);
    expect(
      b.projectDiagnostics.some((d) => d.kind === "unresolvedTraitArg" && d.arg === "nope"),
    ).toBe(true);
  });

  it("does not flag a non-divert (event) param arg", () => {
    const b = bundleFrom(`TRAIT Pinged(event)
  on self.event
    <broadcast: x to participant(self)>

CHARACTER Mole is Pinged(betray)
`);
    expect(b.projectDiagnostics.some((d) => d.kind === "unresolvedTraitArg")).toBe(false);
  });
});

describe("Gap C — SELF fallback to cast[0]", () => {
  it("resolves SELF to the beat's first cast member when no self is bound", () => {
    const sim = Sim.fromSources(`FACTION F
  ethos: order

ROLE Guest
  faction: any of FACTION

== greet(guest)
  cast: Herald, guest
  SELF
    Hear ye.
`);
    const from = sim.log.len();
    sim.playBeat("greet", new Map()); // no router → no `self`
    const line = sim.log.since(from).find((e) => e.type === "dialogue") as {
      speaker: string;
      text: string;
    };
    expect(line.speaker).toBe("HERALD");
    expect(line.text).toBe("Hear ye.");
  });
});
