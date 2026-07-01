//! Pure per-role projections of the live `Sim` into the snapshots each
//! client renders. Kept dependency-free and side-effect-free so they're
//! unit-testable without standing up the HTTP/SSE server.

import type { Sim } from "../src/runtime/sim/index.ts";

export type RuntimePhase = "idle" | "open" | "paused";

/** An authored channel a participant can see, for the sidebar. */
export interface ChannelSnapshot {
  id: string;
  kind: string; // open | private | faction | group | dm
  title: string;
  spaceId: string;
  /** True when the viewer is an explicit member (drives leave/invite). */
  member: boolean;
  /** May the viewer post here (post policy)? Drives the composer. */
  canPost: boolean;
  /** Can messages here open threads? Drives the reply affordance. */
  threadable: boolean;
}

/** A Discord-style sidebar section title. */
export interface SpaceSnapshot {
  id: string;
  title: string;
}

/** What a single party-goer sees about themselves. */
export interface GuestView {
  id: string;
  name: string;
  role: string | null;
  /** App-facing faction — hidden factions read `null` until revealed. */
  faction: string | null;
  score: number;
  location: string | null;
  captured: boolean;
  /** Outstanding choice options awaiting this guest, if any. */
  pendingChoice: string[] | null;
  /** Channel the pending decision docks under (`"lobby"`, a DM, …). */
  decisionChannel: string | null;
  /** Authored channels this guest can see (open + faction + member rooms). */
  channels: ChannelSnapshot[];
  /** Titles for the authored sidebar sections. */
  spaces: SpaceSnapshot[];
  /** Other participants (id + name) — the invite picker's source. */
  roster: Array<{ id: string; name: string }>;
}

export function guestView(sim: Sim, id: string, decisionChannel: string | null = null): GuestView {
  const p = sim.persons.get(id);
  const pendingChoice = sim.pendingChoiceFor(id);
  return {
    id,
    name: p?.name ?? id,
    role: p?.role ?? null,
    faction: sim.publicFactionOf(id),
    score: sim.scoreOf(id),
    location: sim.locationOf(id),
    captured: sim.isCaptured(id),
    pendingChoice,
    decisionChannel: pendingChoice ? (decisionChannel ?? "lobby") : null,
    channels: sim.visibleChannelsFor(id),
    spaces: sim.spaceList(),
    roster: sim.publicRoster(id),
  };
}

export interface RosterRow {
  id: string;
  name: string;
  role: string;
  /** True faction (operator view — includes hidden allegiances). */
  faction: string | null;
  trueFaction: string | null;
  location: string | null;
  captured: boolean;
  score: number;
}

/** The operator's god-view of one guest. Null if the QR is unknown. */
export function rosterRow(sim: Sim, id: string): RosterRow | null {
  const p = sim.persons.get(id);
  if (p === undefined) return null;
  return {
    id: p.id,
    name: p.name,
    role: p.role,
    faction: sim.factionOf(p.id),
    trueFaction: sim.trueFactionOf(p.id),
    location: sim.locationOf(p.id),
    captured: sim.isCaptured(p.id),
    score: sim.scoreOf(p.id),
  };
}

export interface FactionSummary {
  id: string;
  hidden: boolean;
  revealed: boolean;
  ethos: string | null;
  rival: string | null;
  members: string[];
}

export interface LocationSummary {
  id: string;
  label: string | null;
  prison: boolean;
  occupants: string[];
}

/** The operator's full god-view of the world. */
export interface ModView {
  phase: RuntimePhase;
  scenario: string | null;
  roster: RosterRow[];
  factions: FactionSummary[];
  locations: LocationSummary[];
  characters: string[];
  ledgerLen: number;
}

export function modView(sim: Sim | null, phase: RuntimePhase, scenario: string | null): ModView {
  if (sim === null) {
    return { phase, scenario, roster: [], factions: [], locations: [], characters: [], ledgerLen: 0 };
  }
  const roster: RosterRow[] = [...sim.persons.keys()].map((id) => rosterRow(sim, id)!);
  const factions: FactionSummary[] = [...sim.model.factions.values()].map((f) => ({
    id: f.id,
    hidden: f.hidden,
    revealed: sim.factionRevealed(f.id),
    ethos: f.ethos,
    rival: f.rival,
    members: sim.factionMembers(f.id),
  }));
  const locations: LocationSummary[] = [...sim.model.locations.values()].map((l) => ({
    id: l.id,
    label: l.label,
    prison: l.prison,
    occupants: roster.filter((r) => r.location === l.id).map((r) => r.id),
  }));
  return {
    phase,
    scenario,
    roster,
    factions,
    locations,
    characters: [...sim.model.characters.keys()],
    ledgerLen: sim.log.len(),
  };
}

export interface PrimeGuest {
  id: string;
  name: string;
  faction: string | null;
  captured: boolean;
}

/** What an actor playing a character sees: their part + scannable guests. */
export interface PrimeView {
  character: string;
  faction: string | null;
  guests: PrimeGuest[];
  /** Every authored channel — performers run every room. */
  channels: ChannelSnapshot[];
  spaces: SpaceSnapshot[];
}

export function primeView(sim: Sim | null, character: string): PrimeView {
  if (sim === null) return { character, faction: null, guests: [], channels: [], spaces: [] };
  const c = sim.model.characters.get(character);
  const guests: PrimeGuest[] = [...sim.persons.values()].map((p) => ({
    id: p.id,
    name: p.name,
    faction: sim.publicFactionOf(p.id),
    captured: sim.isCaptured(p.id),
  }));
  return {
    character,
    faction: c?.faction ?? null,
    guests,
    channels: sim.allChannelsFor(character),
    spaces: sim.spaceList(),
  };
}
