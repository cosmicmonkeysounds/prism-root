//! Durable state for the event server, so a process restart (laptop
//! sleep, crash, Ctrl-C, deploy) is transparent to everyone in the room.
//!
//! Strategy: **event sourcing**. The `Sim` is fully deterministic — no
//! clocks, no randomness — so we never serialize its tangled internal
//! state (choice continuations hold AST frames that don't round-trip).
//! Instead we journal every *mutation call* (`createPerson`, `join`,
//! `choose`, `tick`, …) as it happens, and on boot rebuild the live sim by
//! replaying the journal against a fresh `Sim.fromSources(scenarioSource)`.
//! Replay is exact because the inputs are exact.
//!
//! Alongside the journal we persist:
//!   - `codes.json`    the three passcodes (stable across restarts)
//!   - `meta.json`     scenario name + source + phase
//!   - `sessions.json` live mod / performer tokens (no re-login needed)
//!
//! Everything is small and written synchronously: durability before the
//! HTTP response matters far more than throughput on a one-room LAN.

import { appendFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import type { Passcodes } from "./auth.ts";
import type { RuntimePhase } from "./views.ts";

/** The sim mutations we journal — exactly the `Sim` methods the server calls. */
export type Mutation =
  | "createPerson"
  | "join"
  | "defect"
  | "scan"
  | "signal"
  | "escape"
  | "choose"
  | "tick";

/** One replayable line of the journal: a method name + its arguments. */
export interface JournalEntry {
  m: Mutation;
  a: unknown[];
}

export interface Meta {
  version: 1;
  scenarioName: string;
  scenarioSource: string;
  phase: RuntimePhase;
}

export interface Sessions {
  /** Moderator session tokens. */
  mod: string[];
  /** Performer tokens paired with their character id. */
  prime: Array<[string, string]>;
}

const CODES = "codes.json";
const META = "meta.json";
const JOURNAL = "journal.ndjson";
const SESSIONS = "sessions.json";

/** Read + JSON-parse a file, returning `null` on any miss/corruption. */
function readJson<T>(path: string): T | null {
  try {
    return JSON.parse(readFileSync(path, "utf8")) as T;
  } catch {
    return null;
  }
}

export class Store {
  constructor(private readonly dir: string) {
    mkdirSync(dir, { recursive: true });
  }

  private path(name: string): string {
    return join(this.dir, name);
  }

  // --- passcodes ----------------------------------------------------------

  loadCodes(): Partial<Passcodes> | null {
    return readJson<Partial<Passcodes>>(this.path(CODES));
  }
  saveCodes(codes: Passcodes): void {
    writeFileSync(this.path(CODES), JSON.stringify(codes, null, 2));
  }

  // --- scenario meta ------------------------------------------------------

  loadMeta(): Meta | null {
    const m = readJson<Meta>(this.path(META));
    return m && m.version === 1 ? m : null;
  }
  saveMeta(meta: Meta): void {
    writeFileSync(this.path(META), JSON.stringify(meta, null, 2));
  }

  // --- command journal ----------------------------------------------------

  /** Append one mutation. Synchronous so it lands before we respond. */
  appendCommand(m: Mutation, a: unknown[]): void {
    appendFileSync(this.path(JOURNAL), JSON.stringify({ m, a }) + "\n");
  }

  /** Every journaled mutation in order. Skips any unparseable line. */
  readJournal(): JournalEntry[] {
    let raw: string;
    try {
      raw = readFileSync(this.path(JOURNAL), "utf8");
    } catch {
      return [];
    }
    const out: JournalEntry[] = [];
    for (const line of raw.split("\n")) {
      if (line.trim() === "") continue;
      try {
        out.push(JSON.parse(line) as JournalEntry);
      } catch {
        /* tolerate a torn final write */
      }
    }
    return out;
  }

  /** Start a fresh story timeline (new scenario, or a reset). */
  clearJournal(): void {
    rmSync(this.path(JOURNAL), { force: true });
  }

  // --- live sessions ------------------------------------------------------

  loadSessions(): Sessions {
    return readJson<Sessions>(this.path(SESSIONS)) ?? { mod: [], prime: [] };
  }
  saveSessions(sessions: Sessions): void {
    writeFileSync(this.path(SESSIONS), JSON.stringify(sessions));
  }

  /** Has any prior run left state here to restore? */
  hasState(): boolean {
    return existsSync(this.path(META));
  }
}
