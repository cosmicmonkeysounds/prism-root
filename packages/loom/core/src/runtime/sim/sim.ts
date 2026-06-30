//! The ecosystem simulation driver.
//!
//! Holds the world state, the event log, and the live population, and
//! advances the world by ingesting external events (account creation,
//! QR scans, faction joins) and draining the reactive hook engine to a
//! fixpoint. See `DESIGN.md`.

import type { Beat, BodyItem, DialogueBlock } from "../../parser/index.ts";
import {
  CallArg,
  ExprError,
  VNULL,
  World,
  asNumber,
  display,
  evaluate,
  expandPath,
  parseExpr,
  truthy,
  vBool,
  vList,
  vNumber,
  vString,
  type Bindings,
  type Value,
} from "../expr.ts";
import { SimLog, type SimEvent } from "./event.ts";
import { compileModel, type Hook, type SimModel } from "./model.ts";
import { parseSet, splitDirective, splitKeyword } from "./effects.ts";
import { Bundle, type LoomFileEntry } from "../bundle.ts";
import { parse } from "../../parser/index.ts";

export interface Person {
  id: string;
  name: string;
  role: string;
}

/** A queued hook-firing request produced by an event. */
interface Trigger {
  verb: string;
  subject: string;
  /** The scanner for `scan` triggers (only its hooks fire). */
  scanner: string | null;
  /** Entity filter the event carries (joined faction, …). */
  filter: string | null;
}

/** One frame of the explicit executor stack — a body + cursor + scope. */
interface Frame {
  items: BodyItem[];
  index: number;
  bindings: Bindings;
}

/**
 * A live choice awaiting a participant's decision. `continuation` is a
 * snapshot of the entire work stack at the suspension point, so picking
 * an option resumes everything that was pending — not just the option
 * body — in the right order across nested beats/hooks.
 */
interface PendingChoice {
  options: Array<{ text: string; body: BodyItem[] }>;
  bindings: Bindings;
  continuation: Frame[];
}

export class Sim {
  readonly world = new World();
  readonly log = new SimLog();
  readonly model: SimModel;
  readonly persons = new Map<string, Person>();

  private membership = new Map<string, Set<string>>();
  private occupants = new Map<string, Set<string>>();
  private pending: Trigger[] = [];
  private beatVisits = new Map<string, number>();
  private revealed = new Set<string>();
  private pendingChoices = new Map<string, PendingChoice[]>();
  private currentBindings: Bindings = new Map();
  private elapsedMs = 0;
  private genState = new Map<string, number>();
  private genCursor = new Map<string, number>();
  private timerState = new Map<Hook, number>();
  private timerFired = new Set<Hook>();

  constructor(model: SimModel) {
    this.model = model;
    // Seed every entity id as its own identity string so barewords read
    // naturally in expressions (`guest.faction == Chatters`).
    for (const id of model.entityKind.keys()) {
      this.world.set(id, vString(id));
    }
    for (const [id, def] of model.factions) {
      this.membership.set(id, new Set());
      this.syncFaction(id);
      // Faction politics is readable state, not dead metadata.
      if (def.ethos !== null) this.world.set(`${id}.ethos`, vString(def.ethos));
      if (def.rival !== null) this.world.set(`${id}.rival`, vString(def.rival));
      this.world.set(`${id}.hidden`, vBool(def.hidden));
      this.world.set(`${id}.revealed`, vBool(false));
    }
    for (const l of model.locations.keys()) {
      this.occupants.set(l, new Set());
      this.syncLocation(l);
    }
    // A character's own faction must be in the world so `self.faction`
    // and `guest.faction == self.faction` resolve.
    for (const char of model.characters.values()) {
      if (char.faction !== null) this.world.set(`${char.id}.faction`, vString(char.faction));
    }
    this.world.setCollection("Persons", vList([]));
  }

  /** Build a `Sim` from `.loom` sources (one entry per file). */
  static fromSources(...sources: Array<string | { path: string; source: string }>): Sim {
    const bundle = new Bundle();
    sources.forEach((s, i) => {
      const source = typeof s === "string" ? s : s.source;
      const path = typeof s === "string" ? `f${i}.loom` : s.path;
      const [file, diagnostics] = parse(source);
      const stem = path.replace(/\.loom$/u, "").split("/").pop() ?? path;
      const entry: LoomFileEntry = { path, stem, qualifier: "", source, file, diagnostics };
      bundle.files.push(entry);
    });
    return new Sim(compileModel(bundle));
  }

  // -------------------------------------------------------------------
  // Input API — what the app / props / actors call
  // -------------------------------------------------------------------

  /** A party-goer creates an account at the door. */
  createPerson(id: string, name: string, role?: string): SimEvent[] {
    const from = this.log.len();
    const roleId = role ?? this.model.defaultRole ?? "Guest";
    this.persons.set(id, { id, name, role: roleId });
    this.world.set(id, vString(id));
    this.world.set(`${id}.name`, vString(name));
    this.world.set(`${id}.role`, vString(roleId));
    this.model.entityKind.set(id, "person");
    const def = this.model.roles.get(roleId);
    if (def !== undefined) {
      for (const [k, v] of def.defaults) this.world.set(`${id}.${k}`, v);
    }
    this.syncPersons();
    this.seedRelationships(id, roleId);
    this.record({ type: "accountCreated", person: id, name, role: roleId });
    this.fire({ verb: "account_created", subject: id, scanner: null, filter: null });
    this.drain();
    return this.log.since(from);
  }

  /** A party-goer joins (or is moved into) a faction. */
  join(person: string, faction: string): SimEvent[] {
    const from = this.log.len();
    this.setFaction(person, faction);
    this.record({ type: "joined", person, faction });
    this.fire({ verb: "join", subject: person, scanner: null, filter: faction });
    this.drain();
    return this.log.since(from);
  }

  /** A party-goer defects from their current faction to another. */
  defect(person: string, to: string): SimEvent[] {
    const from = this.log.len();
    const prior = this.factionOf(person);
    if (prior !== null) this.membership.get(prior)?.delete(person);
    if (prior !== null) this.syncFaction(prior);
    this.setFaction(person, to);
    this.record({ type: "defected", person, from: prior, to });
    this.fire({ verb: "defect", subject: person, scanner: null, filter: to });
    this.drain();
    return this.log.since(from);
  }

  /** An official character or prop scans a party-goer's QR code. */
  scan(scanner: string, person: string): SimEvent[] {
    const from = this.log.len();
    this.record({ type: "scanned", scanner, person });
    this.fire({ verb: "scan", subject: person, scanner, filter: null });
    this.drain();
    return this.log.since(from);
  }

  /** A party-goer physically moves to a location. */
  arrive(person: string, location: string): SimEvent[] {
    const from = this.log.len();
    const prior = this.locationOf(person);
    this.setLocation(person, location);
    this.record({ type: "arrived", person, location, from: prior });
    this.fire({ verb: "arrive", subject: person, scanner: null, filter: location });
    this.drain();
    return this.log.since(from);
  }

  /** A generic external signal fires `on <name>` hooks. */
  signal(name: string, subject?: string): SimEvent[] {
    const from = this.log.len();
    this.record({ type: "signal", name, subject: subject ?? null });
    this.fire({ verb: name, subject: subject ?? "", scanner: null, filter: null });
    this.drain();
    return this.log.since(from);
  }

  /** A party-goer escapes the prison (or any captured state). */
  escape(person: string): SimEvent[] {
    const from = this.log.len();
    this.doEscape(person);
    this.drain();
    return this.log.since(from);
  }

  /**
   * Advance the autonomous clock by `dtMs`: update `Time.*`, fire due
   * ambient generators and time-driven hooks, then drain. Pure-reactive
   * scenarios never need to call this; a timed one (escalating threat,
   * ambient barks) wants a steady tick from the host (the server ticks
   * once a second while the doors are open).
   */
  tick(dtMs: number): SimEvent[] {
    const from = this.log.len();
    this.elapsedMs += Math.max(0, dtMs);
    this.world.set("Time.elapsed", vNumber(Math.floor(this.elapsedMs / 1000)));
    this.world.set("Time.minute", vNumber(Math.floor(this.elapsedMs / 60000)));

    // Ambient generators emit a bark per interval, cycled deterministically.
    for (const g of this.model.gens) {
      let last = this.genState.get(g.id) ?? 0;
      let guard = 0;
      while (this.elapsedMs >= last + g.intervalMs && guard++ < 1000) {
        last += g.intervalMs;
        const cursor = this.genCursor.get(g.id) ?? 0;
        this.genCursor.set(g.id, cursor + 1);
        this.record({ type: "ambient", source: g.id, text: g.barks[cursor % g.barks.length]! });
      }
      this.genState.set(g.id, last);
    }

    // Time-driven character hooks (self = the owning character).
    for (const hook of this.model.timerHooks) {
      const t = hook.timer!;
      if (t.mode === "every") {
        let last = this.timerState.get(hook) ?? 0;
        let guard = 0;
        while (this.elapsedMs >= last + t.ms && guard++ < 1000) {
          last += t.ms;
          this.exec([{ items: hook.body, index: 0, bindings: new Map([["self", hook.ownerId]]) }]);
        }
        this.timerState.set(hook, last);
      } else if (!this.timerFired.has(hook) && this.elapsedMs >= t.ms) {
        this.timerFired.add(hook);
        this.exec([{ items: hook.body, index: 0, bindings: new Map([["self", hook.ownerId]]) }]);
      }
    }

    this.drain();
    return this.log.since(from);
  }

  /** Story-clock elapsed, in milliseconds. */
  elapsed(): number {
    return this.elapsedMs;
  }

  /**
   * A party-goer secretly switches true allegiance while keeping their
   * displayed faction — a double agent (spec: defect vs. betray).
   */
  betray(person: string, secret: string): SimEvent[] {
    const from = this.log.len();
    this.doBetray(person, secret);
    this.drain();
    return this.log.since(from);
  }

  /** A live participant answers their oldest pending choice (by index). */
  choose(person: string, index: number): SimEvent[] {
    const from = this.log.len();
    const queue = this.pendingChoices.get(person);
    if (queue !== undefined && queue.length > 0) {
      const pc = queue[0]!;
      const opt = pc.options[index];
      // Validate before consuming, so a fat-fingered index can't destroy
      // the prompt — the participant can retry.
      if (opt !== undefined) {
        queue.shift();
        if (queue.length === 0) this.pendingChoices.delete(person);
        // Resume the selected option, then the full saved continuation.
        const stack: Frame[] = pc.continuation.map((f) => ({
          items: f.items,
          index: f.index,
          bindings: f.bindings,
        }));
        stack.push({ items: opt.body, index: 0, bindings: pc.bindings });
        this.exec(stack);
      }
    }
    this.drain();
    return this.log.since(from);
  }

  /** The oldest outstanding choice's options for a person, if any. */
  pendingChoiceFor(person: string): string[] | null {
    const queue = this.pendingChoices.get(person);
    return queue !== undefined && queue.length > 0 ? queue[0]!.options.map((o) => o.text) : null;
  }

  /**
   * The faction the app should *show* for a person — hidden factions
   * read as `null` until revealed (the secret-villain mechanic).
   */
  publicFactionOf(person: string): string | null {
    const f = this.factionOf(person);
    if (f === null) return null;
    const def = this.model.factions.get(f);
    if (def !== undefined && def.hidden && !this.revealed.has(f)) return null;
    return f;
  }

  factionRevealed(faction: string): boolean {
    return this.revealed.has(faction);
  }
  trueFactionOf(person: string): string | null {
    const v = this.world.peek(`${person}.trueFaction`);
    if (v !== null && v.kind === "string") return v.value;
    return this.factionOf(person);
  }

  // -------------------------------------------------------------------
  // Read helpers (the app's view of a person)
  // -------------------------------------------------------------------

  /** The person's true faction (engine view). The app should call
   *  `publicFactionOf`, which masks hidden factions until revealed. */
  factionOf(person: string): string | null {
    const v = this.world.peek(`${person}.faction`);
    return v !== null && v.kind === "string" ? v.value : null;
  }
  locationOf(person: string): string | null {
    const v = this.world.peek(`${person}.location`);
    return v !== null && v.kind === "string" ? v.value : null;
  }
  isCaptured(person: string): boolean {
    return truthy(this.world.get(`${person}.captured`));
  }
  scoreOf(person: string): number {
    return asNumber(this.world.get(`${person}.score`)) ?? 0;
  }
  factionMembers(faction: string): string[] {
    return [...(this.membership.get(faction) ?? [])];
  }
  /** Resolve a broadcast scope (`faction(Mods)`, …) to person ids. */
  audienceFor(scope: string): string[] {
    return this.resolveScope(scope, new Map());
  }

  // -------------------------------------------------------------------
  // Drain loop
  // -------------------------------------------------------------------

  private fire(trigger: Trigger): void {
    this.pending.push(trigger);
  }

  private drain(): void {
    let guard = 0;
    while (this.pending.length > 0) {
      if (guard++ > 10000) {
        this.record({ type: "diagnostic", message: "drain exceeded 10000 cycles (hook loop?)" });
        break; // runaway-cycle backstop
      }
      const trigger = this.pending.shift()!;
      for (const [hook, bindings] of this.matchHooks(trigger)) {
        this.exec([{ items: hook.body, index: 0, bindings }]);
      }
    }
  }

  private *matchHooks(t: Trigger): Generator<[Hook, Bindings]> {
    if (t.verb === "scan") {
      if (t.scanner === null) return;
      // The scanner may be a CHARACTER/prop or a Person (peer scan).
      const char = this.model.characters.get(t.scanner);
      if (char !== undefined) {
        for (const hook of char.hooks) {
          if (hook.verb === "scan") yield [hook, this.bindScan(t.scanner, hook, t.subject)];
        }
      }
      const scanner = this.persons.get(t.scanner);
      if (scanner !== undefined) {
        const role = this.model.roles.get(scanner.role);
        if (role !== undefined) {
          for (const hook of role.hooks) {
            if (hook.verb === "scan") yield [hook, this.bindScan(t.scanner, hook, t.subject)];
          }
        }
      }
      return;
    }
    // Role hooks (no param) — `self` is the affected person.
    const person = this.persons.get(t.subject);
    if (person !== undefined) {
      const role = this.model.roles.get(person.role);
      if (role !== undefined) {
        for (const hook of role.hooks) {
          if (hook.timer !== null) continue;
          if (hook.verb !== "scan" && hook.verb === t.verb && this.filterOk(hook, t)) {
            const bindings: Bindings = new Map([["self", t.subject]]);
            if (hook.param !== null) bindings.set(hook.param, t.subject);
            yield [hook, bindings];
          }
        }
      }
    }
    // Character reactions. A `param` binds the subject (`on captured
    // guest`); no param is a global cue (`on lockdown`), `self`-only.
    for (const char of this.model.characters.values()) {
      for (const hook of char.hooks) {
        if (hook.timer !== null) continue;
        if (hook.verb !== t.verb || !this.filterOk(hook, t)) continue;
        if (hook.param !== null) {
          yield [hook, new Map([["self", char.id], [hook.param, t.subject]])];
        } else {
          yield [hook, new Map([["self", char.id]])];
        }
      }
    }
  }

  private filterOk(hook: Hook, t: Trigger): boolean {
    return hook.filter === null || hook.filter === t.filter;
  }

  private bindScan(scanner: string, hook: Hook, person: string): Bindings {
    return new Map([
      ["self", scanner],
      [hook.param ?? "guest", person],
    ]);
  }

  // -------------------------------------------------------------------
  // Executor — shared by beats and hook bodies
  // -------------------------------------------------------------------

  /**
   * Run a work stack of frames to completion (or until a choice
   * suspends it). Using an explicit stack — rather than the JS call
   * stack — lets a choice snapshot the *entire* continuation so nested
   * beats/hooks resume in order after `choose()`.
   */
  private exec(stack: Frame[]): void {
    while (stack.length > 0) {
      const frame = stack[stack.length - 1]!;
      if (frame.index >= frame.items.length) {
        stack.pop();
        continue;
      }
      const item = frame.items[frame.index]!;
      frame.index += 1;
      const b = frame.bindings;
      switch (item.kind) {
        case "action":
        case "sceneHeading":
          this.record({ type: "action", text: this.interpolate(item.value.value, b) });
          break;
        case "metadata":
        case "slotPlaceholder":
          break;
        case "dialogue":
          this.runDialogue(item.value, b);
          break;
        case "directive":
          this.runDirective(item.value.raw, b);
          break;
        case "directiveBlock":
          this.runDirective(item.value.directive.raw, b);
          stack.push({ items: item.value.body, index: 0, bindings: b });
          break;
        case "conditional":
          for (const arm of item.value.arms) {
            if (arm.condition === null || this.evalCond(arm.condition, b)) {
              stack.push({ items: arm.body, index: 0, bindings: b });
              break;
            }
          }
          break;
        case "afterMorph":
          stack.push({
            items: this.evalCond(item.value.condition, b) ? item.value.after : item.value.otherwise,
            index: 0,
            bindings: b,
          });
          break;
        case "match": {
          const scrutinee = display(this.evalValue(item.value.scrutinee, b));
          for (const arm of item.value.arms) {
            if (arm.pattern === scrutinee) {
              stack.push({ items: arm.body, index: 0, bindings: b });
              break;
            }
          }
          break;
        }
        case "eachVisit":
          stack.push({ items: item.value.first, index: 0, bindings: b });
          break;
        case "inlineLet":
          this.world.set(item.value.name, this.evalValue(item.value.expression, b));
          break;
        case "divert": {
          const d = item.value;
          if (d.kind === "to") {
            const beat = this.model.beats.get(d.target.name);
            if (beat !== undefined) {
              const key = this.visitKey(d.target.name, b);
              this.beatVisits.set(key, (this.beatVisits.get(key) ?? 0) + 1);
              this.record({ type: "beatEntered", beat: d.target.name });
              stack.push({ items: beat.body, index: 0, bindings: b });
            }
          }
          break; // end / return / tunnel: terminate this branch
        }
        case "choice": {
          // A choice menu suspends execution. The current work stack IS
          // the continuation — snapshot it (this frame's cursor already
          // points past the menu) and resume on `choose()`.
          let j = frame.index - 1;
          const options: Array<{ text: string; body: BodyItem[] }> = [];
          while (j < frame.items.length && frame.items[j]!.kind === "choice") {
            const c = frame.items[j]! as Extract<BodyItem, { kind: "choice" }>;
            options.push({ text: this.interpolate(c.value.text, b), body: c.value.body });
            j += 1;
          }
          frame.index = j; // advance past the whole menu
          const person = this.subjectAudience(b)[0] ?? "__global";
          const continuation = stack.map((f) => ({
            items: f.items,
            index: f.index,
            bindings: f.bindings,
          }));
          const queue = this.pendingChoices.get(person) ?? [];
          queue.push({ options, bindings: b, continuation });
          this.pendingChoices.set(person, queue);
          this.record({
            type: "choicePrompted",
            person: person === "__global" ? null : person,
            promptId: person,
            options: options.map((o) => o.text),
          });
          return; // suspend
        }
      }
    }
  }

  private runDialogue(block: DialogueBlock, bindings: Bindings): void {
    const text = block.lines
      .filter((l) => l.kind === "text")
      .map((l) => (l.kind === "text" ? l.value.value : ""))
      .join(" ");
    const audience = this.subjectAudience(bindings);
    this.record({
      type: "dialogue",
      speaker: block.speaker,
      text: this.interpolate(text, bindings),
      audience,
    });
  }

  /** Play a scripted beat in the given binding scope. */
  playBeat(name: string, bindings: Bindings): void {
    const beat = this.model.beats.get(name);
    if (beat === undefined) return;
    const key = this.visitKey(name, bindings);
    this.beatVisits.set(key, (this.beatVisits.get(key) ?? 0) + 1);
    this.record({ type: "beatEntered", beat: name });
    this.exec([{ items: beat.body, index: 0, bindings }]);
  }

  /** Per-person visit key so `visits(beat)` is scoped to the participant. */
  private visitKey(name: string, bindings: Bindings): string {
    const subj = bindings.get("guest") ?? bindings.get("self") ?? "__global";
    return `${name}::${subj}`;
  }

  // -------------------------------------------------------------------
  // Directive vocabulary (the effect language)
  // -------------------------------------------------------------------

  private runDirective(raw: string, bindings: Bindings): void {
    const { verb, rest } = splitDirective(raw);
    switch (verb) {
      case "set":
        this.runSet(rest, bindings);
        break;
      case "capture":
        this.runCapture(rest, bindings);
        break;
      case "release":
        this.runRelease(rest, bindings);
        break;
      case "escape":
        this.doEscape(this.resolveId(rest, bindings));
        break;
      case "join": {
        const kw = splitKeyword(rest, "to");
        if (kw !== null) {
          const p = this.resolveId(kw[0], bindings);
          const f = this.resolveId(kw[1], bindings);
          this.setFaction(p, f);
          this.record({ type: "joined", person: p, faction: f });
          this.fire({ verb: "join", subject: p, scanner: null, filter: f });
        }
        break;
      }
      case "defect": {
        const fromKw = splitKeyword(rest, "from");
        const toKw = fromKw !== null ? splitKeyword(fromKw[1], "to") : null;
        if (fromKw !== null && toKw !== null) {
          const p = this.resolveId(fromKw[0], bindings);
          const a = this.resolveId(toKw[0], bindings);
          const b = this.resolveId(toKw[1], bindings);
          this.membership.get(a)?.delete(p);
          this.syncFaction(a);
          this.setFaction(p, b);
          this.record({ type: "defected", person: p, from: a, to: b });
          this.fire({ verb: "defect", subject: p, scanner: null, filter: b });
        }
        break;
      }
      case "broadcast":
        this.runBroadcast(rest, bindings);
        break;
      case "cast":
      case "promote": {
        const kw = splitKeyword(rest, verb === "cast" ? "as" : "to");
        if (kw !== null) {
          const p = this.resolveId(kw[0], bindings);
          const role = this.resolveId(kw[1], bindings);
          const person = this.persons.get(p);
          if (person !== undefined) person.role = role;
          this.world.set(`${p}.role`, vString(role));
          this.seedRelationships(p, role); // dispositions tied to the new role
          this.record({ type: verb === "cast" ? "cast" : "promoted", person: p, role });
        }
        break;
      }
      case "betray": {
        const kw = splitKeyword(rest, "to");
        if (kw !== null) this.doBetray(this.resolveId(kw[0], bindings), this.resolveId(kw[1], bindings));
        break;
      }
      case "reveal":
        this.doReveal(this.resolveId(rest, bindings));
        break;
      case "respond": {
        const to = bindings.get("self") ?? "";
        this.record({ type: "respond", to, text: this.interpolate(rest, bindings) });
        break;
      }
      case "fire": {
        const name = this.resolveId(rest.split(",")[0]?.trim() ?? rest, bindings);
        this.record({ type: "directive", verb, args: rest });
        this.fire({ verb: name, subject: "", scanner: null, filter: null });
        break;
      }
      default:
        this.record({ type: "directive", verb, args: this.interpolate(rest, bindings) });
        break;
    }
  }

  private runSet(rest: string, bindings: Bindings): void {
    const clause = parseSet(rest);
    if (clause === null) return;
    const path = expandPath(clause.path, bindings);
    const rhs = this.evalValue(clause.rhs, bindings);
    let next: Value;
    if (clause.op === "=") {
      // A bare identifier that resolves to nothing is an enum/sum value
      // (`<set: g.allegiance = loyal>`), not a missing path.
      next =
        rhs.kind === "null" && /^[A-Za-z_][A-Za-z0-9_]*$/u.test(clause.rhs.trim())
          ? vString(clause.rhs.trim())
          : rhs;
    } else {
      const cur = asNumber(this.world.get(path)) ?? 0;
      const r = asNumber(rhs) ?? 0;
      const n =
        clause.op === "+=" ? cur + r : clause.op === "-=" ? cur - r : clause.op === "*=" ? cur * r : cur / r;
      next = vNumber(n);
    }
    this.world.set(path, next);
    this.record({ type: "worldSet", path, value: display(next) });
    // Surface relationship writes (`Char.trusts.Person`) distinctly.
    const segs = path.split(".");
    if (segs.length === 3) {
      this.record({
        type: "relationshipChanged",
        subject: segs[0]!,
        relation: segs[1]!,
        object: segs[2]!,
        value: asNumber(next) ?? 0,
      });
    }
  }

  private runCapture(rest: string, bindings: Bindings): void {
    const kw = splitKeyword(rest, "into");
    const person = this.resolveId(kw !== null ? kw[0] : rest, bindings);
    // Idempotent: re-scanning an already-imprisoned guest is a no-op,
    // so the cascade (score dock, villain counter) doesn't double-fire.
    if (this.isCaptured(person)) return;
    if (!this.persons.has(person)) return; // unknown QR — fail quietly
    const location = kw !== null ? this.resolveId(kw[1], bindings) : this.prisonLocation();
    this.world.set(`${person}.captured_from`, vString(this.locationOf(person) ?? this.freeLocation()));
    this.setLocation(person, location);
    this.world.set(`${person}.captured`, vBool(true));
    const by = bindings.get("self") ?? null;
    this.record({ type: "captured", person, location, by });
    this.fire({ verb: "captured", subject: person, scanner: null, filter: location });
  }

  private runRelease(rest: string, bindings: Bindings): void {
    const kw = splitKeyword(rest, "from");
    const person = this.resolveId(kw !== null ? kw[0] : rest, bindings);
    const location = kw !== null ? this.resolveId(kw[1], bindings) : this.prisonLocation();
    this.world.set(`${person}.captured`, vBool(false));
    this.setLocation(person, this.freeLocation());
    this.record({ type: "released", person, location });
    this.fire({ verb: "released", subject: person, scanner: null, filter: location });
  }

  private doEscape(person: string): void {
    // Only a genuine imprisoned→free transition rewards the player, so
    // the score can't be farmed by repeated escape signals.
    if (!this.isCaptured(person)) return;
    this.world.set(`${person}.captured`, vBool(false));
    const back = this.world.peek(`${person}.captured_from`);
    this.setLocation(person, back !== null && back.kind === "string" ? back.value : this.freeLocation());
    this.record({ type: "escaped", person });
    this.fire({ verb: "escape", subject: person, scanner: null, filter: null });
  }

  private doBetray(person: string, secret: string): void {
    const displayed = this.factionOf(person);
    this.world.set(`${person}.trueFaction`, vString(secret));
    this.record({ type: "betrayed", person, displayed, secret });
    this.fire({ verb: "betray", subject: person, scanner: null, filter: secret });
  }

  private doReveal(faction: string): void {
    if (this.revealed.has(faction)) return;
    this.revealed.add(faction);
    this.world.set(`${faction}.revealed`, vBool(true));
    this.record({ type: "factionRevealed", faction });
    this.fire({ verb: "revealed", subject: faction, scanner: null, filter: null });
  }

  private runBroadcast(rest: string, bindings: Bindings): void {
    const kw = splitKeyword(rest, "to");
    const cue = kw !== null ? kw[0].trim() : rest.trim();
    const scopeText = kw !== null ? kw[1].trim() : "";
    const audience = this.resolveScope(scopeText, bindings);
    this.record({ type: "broadcast", cue, audience, scope: scopeText });
  }

  /** Resolve a broadcast scope into a list of person ids. */
  private resolveScope(scopeText: string, bindings: Bindings): string[] {
    const out = new Set<string>();
    for (const term of scopeText.split("|")) {
      const t = term.trim();
      const m = /^(\w+)\((.*)\)$/u.exec(t);
      if (m === null) continue;
      const kind = m[1]!;
      // Evaluate the inner arg so `faction(guest.faction)` resolves to
      // the triggering guest's concrete faction id.
      const arg = display(this.evalValue(m[2]!.trim(), bindings));
      if (kind === "participant") {
        if (this.persons.has(arg)) out.add(arg);
      } else if (kind === "faction") {
        for (const p of this.membership.get(arg) ?? []) out.add(p);
      } else if (kind === "location") {
        for (const p of this.occupants.get(arg) ?? []) out.add(p);
      }
    }
    return [...out];
  }

  // -------------------------------------------------------------------
  // State mutation primitives (keep collections + scalars in sync)
  // -------------------------------------------------------------------

  private setFaction(person: string, faction: string): void {
    const prior = this.factionOf(person);
    if (prior !== null && prior !== faction) {
      this.membership.get(prior)?.delete(person);
      this.syncFaction(prior);
    }
    this.world.set(`${person}.faction`, vString(faction));
    let set = this.membership.get(faction);
    if (set === undefined) {
      set = new Set();
      this.membership.set(faction, set);
    }
    set.add(person);
    this.syncFaction(faction);
  }

  private setLocation(person: string, location: string): void {
    const prior = this.locationOf(person);
    if (prior !== null && prior !== location) {
      this.occupants.get(prior)?.delete(person);
      this.syncLocation(prior);
      this.fire({ verb: "exits", subject: person, scanner: null, filter: prior });
    }
    this.world.set(`${person}.location`, vString(location));
    let set = this.occupants.get(location);
    if (set === undefined) {
      set = new Set();
      this.occupants.set(location, set);
    }
    set.add(person);
    this.syncLocation(location);
    // Symmetric with `exits`: every movement path (arrive, capture,
    // escape, release) fires `enters` so `on enters LOCATION` works.
    if (prior !== location) {
      this.fire({ verb: "enters", subject: person, scanner: null, filter: location });
    }
  }

  private seedRelationships(person: string, roleId: string): void {
    // Seed each character's disposition axis against the new person
    // (`trusts Guest: 50 of 100` → `Char.trusts.person = 50`).
    for (const char of this.model.characters.values()) {
      for (const axis of char.disposition) {
        if (axis.target === roleId || axis.target === "Guest" || axis.target === person) {
          this.world.set(`${char.id}.${axis.verb}.${person}`, vNumber(axis.current));
        }
      }
    }
  }

  private prisonLocation(): string {
    for (const [id, def] of this.model.locations) if (def.prison) return id;
    return "Internet";
  }
  private freeLocation(): string {
    for (const [id, def] of this.model.locations) if (!def.prison) return id;
    return "Party";
  }

  private syncFaction(faction: string): void {
    this.world.setCollection(
      `${faction}.members`,
      vList([...(this.membership.get(faction) ?? [])].map(vString)),
    );
  }
  private syncLocation(location: string): void {
    this.world.setCollection(
      `${location}.occupants`,
      vList([...(this.occupants.get(location) ?? [])].map(vString)),
    );
  }
  private syncPersons(): void {
    this.world.setCollection("Persons", vList([...this.persons.keys()].map(vString)));
  }

  private subjectAudience(bindings: Bindings): string[] {
    for (const name of ["guest", "person", "subject"]) {
      const id = bindings.get(name);
      if (id !== undefined && this.persons.has(id)) return [id];
    }
    const self = bindings.get("self");
    return self !== undefined && this.persons.has(self) ? [self] : [];
  }

  // -------------------------------------------------------------------
  // Expression bridge
  // -------------------------------------------------------------------

  private resolveId(text: string, bindings: Bindings): string {
    const t = text.trim();
    return bindings.get(t) ?? t;
  }

  private evalValue(src: string, bindings: Bindings): Value {
    this.currentBindings = bindings;
    try {
      return evaluate(parseExpr(src), this.world, this.callFn, bindings);
    } catch (e) {
      if (e instanceof ExprError) return VNULL;
      throw e;
    }
  }

  private evalCond(src: string, bindings: Bindings): boolean {
    return truthy(this.evalValue(src, bindings));
  }

  private callFn = (name: string, args: CallArg[]): Value => {
    switch (name) {
      case "members": {
        const f = args[0]?.asName() ?? "";
        return vList(this.factionMembers(f).map(vString));
      }
      case "occupants": {
        const l = args[0]?.asName() ?? "";
        return vList([...(this.occupants.get(l) ?? [])].map(vString));
      }
      case "count": {
        const v = args[0]?.value;
        return vNumber(v !== undefined && v.kind === "list" ? v.items.length : 0);
      }
      case "visits": {
        const subj =
          this.currentBindings.get("guest") ?? this.currentBindings.get("self") ?? "__global";
        return vNumber(this.beatVisits.get(`${args[0]?.asName() ?? ""}::${subj}`) ?? 0);
      }
      default:
        return VNULL;
    }
  };

  private interpolate(text: string, bindings: Bindings): string {
    return text.replace(/\{([^}]+)\}/gu, (_m, expr: string) =>
      display(this.evalValue(expr.trim(), bindings)),
    );
  }

  private record(event: SimEvent): number {
    return this.log.push(event);
  }
}

// Re-exported convenience: a fresh model from a single source.
export function compileFromSource(source: string): SimModel {
  const [file, diagnostics] = parse(source);
  const bundle = new Bundle();
  bundle.files.push({ path: "f0.loom", stem: "f0", qualifier: "", source, file, diagnostics });
  return compileModel(bundle);
}

// Used by `Sim.beat` typing.
export type { Beat };
