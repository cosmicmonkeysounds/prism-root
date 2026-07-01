//! Slice 1 of the functional redesign (docs/dev/loom-functional-redesign.md):
//! the `is`-merge is now actually run on the sim compile path, ROLEs may mix
//! in traits (they are schemas, never "abstract"), and the `SELF`/`ME`
//! dialogue speaker resolves to whoever `self` is bound to.

import { describe, expect, it } from "vitest";
import { parse } from "../src/parser/index.ts";
import { Bundle, type LoomFileEntry } from "../src/runtime/index.ts";
import { Sim } from "../src/runtime/sim/index.ts";
import type { SimEvent } from "../src/runtime/sim/index.ts";

function bundleFrom(...sources: string[]): Bundle {
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
  b.rebuildSimulacra();
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
