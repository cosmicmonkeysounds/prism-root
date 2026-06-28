//! Append-only event ledger — the canonical record of everything the
//! playhead emits. Drives live-performance scope filters (spec §13),
//! the `since`/`visits` query helpers (spec §12.2), and the booth
//! transcript view (spec §13.4).

import type { BeatRef } from "./bundle.ts";
import { CallArg, ExprError, VNULL, vBool, vNumber, vString, type Value } from "./expr.ts";

/** Classification of a `cellEntered` envelope (loom-editor.html §3). */
export type CellKind = "beatVisit" | "improv" | "generatorYield" | "directive";

export interface ChoiceOption {
  index: number;
  text: string;
  sticky: boolean;
}

/** Stable track id. `0` is the implicit main track (`TRACK_MAIN`). */
export type TrackId = number;
export const TRACK_MAIN: TrackId = 0;

/** Reserved name for the Booth track in `registerTrack` / `trackFor`. */
export const BOOTH_TRACK_NAME = "<booth>";

/** One event written by the playhead. */
export type Event =
  | { type: "beatEntered"; beat: string; file: string; reference: BeatRef | null }
  | { type: "scene"; text: string }
  | { type: "action"; text: string }
  | {
      type: "dialogue";
      speaker: string;
      speakers: string[];
      parenthetical: string | null;
      text: string;
    }
  | { type: "metadata"; text: string }
  | { type: "choicePrompted"; options: ChoiceOption[] }
  | { type: "choiceTaken"; index: number; text: string }
  | { type: "diverted"; target: string; beat: string }
  | { type: "tunneled"; target: string; beat: string }
  | { type: "returned" }
  | { type: "ended" }
  | { type: "directive"; kind: string; positional: string[]; named: Array<[string, string]> }
  | { type: "worldSet"; path: string; value: string }
  | { type: "knowledgeChanged"; character: string; field: string; value: string }
  | { type: "fired"; name: string; payload: Array<[string, string]> }
  | { type: "conditionalArm"; condition: string | null }
  | { type: "letEvaluated"; name: string; value: string }
  | { type: "sceneSpawned"; scene: string; coroutine: number; tier: string }
  | { type: "sceneAdvanced"; scene: string; coroutine: number; state: string }
  | { type: "sceneCompleted"; scene: string; coroutine: number; value: string }
  | { type: "generatorYielded"; generator: string; coroutine: number; text: string }
  | { type: "coroutineWaiting"; coroutine: number; reason: string }
  | { type: "participantJoined"; id: string }
  | { type: "participantRetired"; id: string }
  | { type: "participantRecast"; oldId: string; newId: string }
  | { type: "beatSkipped"; beat: string }
  | { type: "bundleReloaded" }
  | { type: "participantEnteredLocation"; id: string; location: string }
  | { type: "cohortEnrolled"; id: string; cohort: string }
  | { type: "improvBeatStarted"; handle: number }
  | { type: "improvBeatAdvanced"; handle: number; reason: string }
  | { type: "afterLatched"; beat: string; anchor: string }
  | { type: "castBound"; person: string; role: string }
  | { type: "castReleased"; person: string; role: string }
  | { type: "castSwapped"; role: string; oldPlayer: string; newPlayer: string }
  | { type: "rolePromoted"; person: string; role: string }
  | { type: "rosterLoaded"; roster: string }
  | { type: "cellEntered"; track: TrackId; kind: CellKind; bundleRef: string }
  | { type: "cellExited"; track: TrackId; snapshot: string }
  | { type: "hookFired"; track: TrackId; clause: string; cause: number };

/** Metadata attached to every envelope (loom-editor.html §3). */
export interface EnvelopeMeta {
  track: TrackId;
  cause: number | null;
  clock: number | null;
}

export function defaultEnvelopeMeta(): EnvelopeMeta {
  return { track: TRACK_MAIN, cause: null, clock: null };
}

/** An ordered log of `Event`s + per-envelope `EnvelopeMeta`. */
export class Ledger {
  private eventsList: Event[] = [];
  private metaList: EnvelopeMeta[] = [];
  private trackTails = new Map<TrackId, number>();
  private nameToTrack = new Map<string, TrackId>();
  private currentClock: number | null = null;

  /** Push on the implicit main track, chaining `cause` to its tail. */
  push(event: Event): void {
    this.pushOn(TRACK_MAIN, event);
  }

  /** Push on a specific track, chaining `cause` to that track's tail. */
  pushOn(track: TrackId, event: Event): void {
    const cause = this.trackTails.get(track) ?? null;
    this.pushWithMeta(event, { track, cause, clock: null });
  }

  /** Push with explicit metadata (used by the hook-drain path). */
  pushWithMeta(event: Event, meta: EnvelopeMeta): void {
    if (meta.clock === null) meta = { ...meta, clock: this.currentClock };
    const idx = this.eventsList.length;
    this.eventsList.push(event);
    this.metaList.push(meta);
    this.trackTails.set(meta.track, idx);
  }

  setClock(clock: number | null): void {
    this.currentClock = clock;
  }

  events(): readonly Event[] {
    return this.eventsList;
  }

  meta(): readonly EnvelopeMeta[] {
    return this.metaList;
  }

  registerTrack(name: string, id: TrackId): void {
    this.nameToTrack.set(name, id);
  }

  trackFor(name: string): TrackId | null {
    return this.nameToTrack.get(name) ?? null;
  }

  /** Resolve a name to its track id, case-insensitively. */
  trackForCi(name: string): TrackId | null {
    const exact = this.nameToTrack.get(name);
    if (exact !== undefined) return exact;
    const lower = name.toLowerCase();
    for (const [k, id] of this.nameToTrack) {
      if (k.toLowerCase() === lower) return id;
    }
    return null;
  }

  pushFor(name: string, event: Event): void {
    this.pushOn(this.trackFor(name) ?? TRACK_MAIN, event);
  }

  iterWithMeta(): Array<[number, Event, EnvelopeMeta]> {
    return this.eventsList.map((e, i) => [i, e, this.metaList[i]!]);
  }

  len(): number {
    return this.eventsList.length;
  }

  isEmpty(): boolean {
    return this.eventsList.length === 0;
  }

  /** Index of the last event matching `predicate` (from start), or null. */
  lastMatching(predicate: (e: Event) => boolean): number | null {
    for (let i = this.eventsList.length - 1; i >= 0; i--) {
      if (predicate(this.eventsList[i]!)) return i;
    }
    return null;
  }

  /** Count of `beatEntered` events naming `beat`. Backs `visits(name)`. */
  beatVisitCount(beat: string): number {
    let n = 0;
    for (const e of this.eventsList) {
      if (e.type === "beatEntered" && e.beat === beat) n += 1;
    }
    return n;
  }

  /** Backs `played(name)`. */
  played(name: string): boolean {
    return this.eventsList.some((e) => eventNames(e, name));
  }

  afterLatched(beat: string, anchor: string): boolean {
    return this.eventsList.some(
      (e) => e.type === "afterLatched" && e.beat === beat && e.anchor === anchor,
    );
  }

  /** Steps elapsed since the most recent event named `name`, or null. */
  since(name: string): number | null {
    for (let idx = this.eventsList.length - 1; idx >= 0; idx--) {
      if (eventNames(this.eventsList[idx]!, name)) {
        return this.eventsList.length - idx - 1;
      }
    }
    return null;
  }

  /** Scoped form — `since(scope, name)` (spec §12.2). */
  sinceScoped(scope: string, name: string): number | null {
    for (let idx = this.eventsList.length - 1; idx >= 0; idx--) {
      const event = this.eventsList[idx]!;
      if (!eventInScope(event, scope)) continue;
      let matches = eventNames(event, name);
      if (!matches && event.type === "worldSet") {
        const dot = event.path.lastIndexOf(".");
        matches = dot >= 0 && event.path.slice(dot + 1) === name;
      }
      if (matches) return this.eventsList.length - idx - 1;
    }
    return null;
  }

  /** `last(target, speaker)` — speaker of the most recent dialogue
   *  whose parenthetical names `target` (spec §12.2). */
  lastSpeakerTo(target: string): string | null {
    for (let i = this.eventsList.length - 1; i >= 0; i--) {
      const event = this.eventsList[i]!;
      if (event.type === "dialogue") {
        const mentions =
          event.parenthetical !== null &&
          event.parenthetical
            .split(/[^0-9A-Za-z_]/u)
            .some((w) => w === target);
        if (mentions) return event.speaker;
      }
    }
    return null;
  }
}

function eventInScope(event: Event, scope: string): boolean {
  switch (event.type) {
    case "participantJoined":
    case "participantRetired":
    case "participantEnteredLocation":
    case "cohortEnrolled":
      return event.id === scope;
    case "participantRecast":
      return event.oldId === scope || event.newId === scope;
    case "dialogue":
      return event.speaker === scope;
    case "worldSet": {
      const dot = event.path.indexOf(".");
      return dot >= 0 && event.path.slice(0, dot) === scope;
    }
    case "fired":
      return event.payload.some(([, v]) => v === scope);
    default:
      return false;
  }
}

function eventNames(event: Event, name: string): boolean {
  switch (event.type) {
    case "beatEntered":
      return event.beat === name;
    case "fired":
      return event.name === name;
    case "directive":
      return event.kind === "anchor" && (event.positional[0] ?? null) === name;
    default:
      return false;
  }
}

/** Resolve a ledger query call — `played` / `visits` / `since` / `last`. */
export function callQuery(ledger: Ledger, name: string, args: CallArg[]): Value {
  const firstName = (): string => (args[0] !== undefined ? args[0].asName() : "");
  const nthName = (n: number): string => (args[n] !== undefined ? args[n]!.asName() : "");
  switch (name) {
    case "played":
      return vBool(ledger.played(firstName()));
    case "visits":
      return vNumber(ledger.beatVisitCount(firstName()));
    case "since": {
      if (args.length >= 2) {
        const steps = ledger.sinceScoped(firstName(), nthName(1));
        return steps !== null ? vNumber(steps) : VNULL;
      }
      const steps = ledger.since(firstName());
      return steps !== null ? vNumber(steps) : VNULL;
    }
    case "last": {
      const target = firstName();
      const field = nthName(1);
      if (field === "speaker") {
        const s = ledger.lastSpeakerTo(target);
        return s !== null ? vString(s) : VNULL;
      }
      return VNULL;
    }
    default:
      throw new ExprError(`unknown function \`${name}\``);
  }
}
