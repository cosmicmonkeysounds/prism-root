//! The named-event enumeration (`namedEvents` / `BuiltinVerb`) and the
//! `SimEventType` enum — the closed vocabularies the editor's Sim/Run
//! director surfaces are built on.

import { describe, expect, it } from "vitest";
import {
  BuiltinVerb,
  Sim,
  SimEventType,
  isBuiltinVerb,
  namedEvents,
} from "../src/runtime/sim/index.ts";

const SOURCE = `entry: opening

FACTION Mods
  ethos: order

LOCATION Party
  label: The Party

ROLE Guest
  score: 0 to 100 = 0
  on join Mods
    <set: self.score += 1>

CHARACTER Admin
  faction: Mods
  on lockdown
    <broadcast: lockdown_siren to faction(Mods)>
  on rally guest
    <set: guest.score += 5>
  on scan guest
    <set: guest.score += 1>
  on every 30s
    Ambient hum.

== opening
  The lights dim.
`;

describe("namedEvents", () => {
  it("enumerates authored hook verbs — builtins and timers excluded", () => {
    const sim = Sim.fromSources(SOURCE);
    // `scan` is builtin, `every 30s` is a timer, `join` (role hook) is
    // builtin — only the authored story events remain, sorted.
    expect(namedEvents(sim.model)).toEqual(["lockdown", "rally"]);
  });

  it("classifies builtin verbs", () => {
    expect(isBuiltinVerb(BuiltinVerb.Scan)).toBe(true);
    expect(isBuiltinVerb("captured")).toBe(true);
    expect(isBuiltinVerb("lockdown")).toBe(false);
  });
});

describe("SimEventType", () => {
  it("values are the journal discriminants", () => {
    const sim = Sim.fromSources(SOURCE);
    sim.createPerson("g1", "Grace");
    sim.join("g1", "Mods");
    const events = sim.signal("lockdown");
    expect(events.some((e) => e.type === SimEventType.Signal)).toBe(true);
    expect(events.some((e) => e.type === SimEventType.Broadcast)).toBe(true);
    // Firing an enumerated named event reaches its hook's audience.
    const b = events.find((e) => e.type === SimEventType.Broadcast);
    expect(b !== undefined && b.type === "broadcast" && b.audience).toEqual(["g1"]);
  });
});
