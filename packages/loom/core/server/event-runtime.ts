//! One live event = one `EventRuntime`.
//!
//! Everything that used to be a module-level global in `server.ts` (the
//! `Sim`, its phase, the chat store, sessions, decision docks, the SSE
//! client set, the autonomous ticker, the on-disk journal) now lives on an
//! instance so a single process can host **many** events at once — one per
//! project that has opened its doors. The multi-tenant server keeps a
//! registry of these and routes each request to the right one.
//!
//! The request handlers, fan-out, persistence and restart-recovery are ported
//! verbatim from the original single-event server; only the state they touch
//! moved from `state.*` / free variables onto `this`.

import type { IncomingMessage, ServerResponse } from "node:http";
import { randomUUID } from "node:crypto";

import { Sim, type SimEvent } from "../src/runtime/sim/index.ts";
import { guestView, modView, primeView, rosterRow, type RuntimePhase } from "./views.ts";
import { passOk, type Passcodes } from "./auth.ts";
import { SessionStore } from "./session.ts";
import { Store, type Mutation } from "./store.ts";
import { ChatStore, composeGuestMessages, decisionChannelFor, visibleTo, type ChatMessage } from "./chat.ts";
import { readBody, sendJson, sseSend, str, tokenOf } from "./http-util.ts";

/** A guest view is never rendered against a null sim — this stands in. */
const EMPTY_SIM = Sim.fromSources("");

/** One SSE subscriber attached to a specific event. */
interface Client {
  role: "guest" | "prime" | "mod";
  id: string; // person id (guest), character (prime), or "" (mod)
  res: ServerResponse;
}

/** What a restart-recovery brought back for one event, for the boot banner. */
export interface Restored {
  guests: number;
  events: number;
  sessions: number;
}

/** Construction context for an event runtime. */
export interface EventRuntimeInit {
  /** Stable event id (also the on-disk state sub-directory name). */
  eventId: string;
  /** Durable per-event state (journal / sessions / hidden). */
  store: Store;
  /** The three passcodes gating this event's roles. */
  codes: Passcodes;
  /** Human label for the loaded scenario. */
  scenarioName: string;
  /** The `.loom` source snapshot this event plays. */
  scenarioSource: string;
  /** Best LAN/base URL for guest-join QRs (for `/api/mod/codes`). */
  joinBase: () => string;
}

export class EventRuntime {
  readonly eventId: string;
  readonly codes: Passcodes;
  private readonly store: Store;
  private readonly joinBase: () => string;

  private sim: Sim | null = null;
  private phase: RuntimePhase = "idle";
  private scenarioName: string;
  private scenarioSource: string;

  // Token → capabilities (performer / admin), scoped to this event.
  private readonly sessions = new SessionStore();
  // Server-authoritative chat, rebuilt deterministically from the journal.
  private readonly chat = new ChatStore();
  // Where each guest's pending decision docks (channel id). Ephemeral.
  private readonly decisionChannels = new Map<string, string>();
  // This event's SSE subscribers.
  private readonly clients = new Set<Client>();
  // Ephemeral messages already announced as expired (so we notify once).
  private readonly notifiedExpired = new Set<number>();
  // Elapsed sim time not yet written to the journal (coalesced ticks).
  private pendingTickMs = 0;
  private ticker: ReturnType<typeof setInterval> | null = null;

  constructor(init: EventRuntimeInit) {
    this.eventId = init.eventId;
    this.store = init.store;
    this.codes = init.codes;
    this.scenarioName = init.scenarioName;
    this.scenarioSource = init.scenarioSource;
    this.joinBase = init.joinBase;
  }

  /** Current lifecycle phase (idle | open | paused). */
  get currentPhase(): RuntimePhase {
    return this.phase;
  }

  /** Human label of the loaded scenario. */
  get scenario(): string {
    return this.scenarioName;
  }

  /** How many participants exist in this event's world right now. */
  get guestCount(): number {
    return this.sim?.persons.size ?? 0;
  }

  /** True once the doors are open and a sim exists. */
  private get open(): boolean {
    return this.phase === "open" && this.sim !== null;
  }

  /** A sim that's never null for guest views (returns an empty live sim). */
  private reqSim(): Sim {
    return this.sim ?? EMPTY_SIM;
  }

  // -- SSE fan-out ---------------------------------------------------------

  private snapshotFor(client: Client): unknown {
    if (client.role === "guest") return guestView(this.reqSim(), client.id, this.decisionChannels.get(client.id) ?? null);
    if (client.role === "prime") return primeView(this.sim, client.id);
    return modView(this.sim, this.phase, this.scenarioName);
  }

  /** Push fresh snapshots to every connected client. */
  private pushSnapshots(): void {
    for (const c of this.clients) sseSend(c.res, "snapshot", this.snapshotFor(c));
  }

  private toPrime(character: string, event: string, data: unknown): void {
    for (const c of this.clients) if (c.role === "prime" && c.id === character) sseSend(c.res, event, data);
  }

  /**
   * Deliver one composed message to the clients that should see it: guests
   * whose id is in the audience (hidden messages withheld), and every
   * performer/mod console (the full feed, so admins can moderate live).
   */
  private deliverMessage(m: ChatMessage): void {
    for (const c of this.clients) {
      if (c.role === "guest") {
        if (!m.hidden && visibleTo(m, c.id)) sseSend(c.res, "message", m);
      } else {
        sseSend(c.res, "message", m);
      }
    }
  }

  // -- ephemeral messages --------------------------------------------------

  /** Has a message aged past its channel's ephemeral lifetime? */
  private isExpired(m: ChatMessage): boolean {
    if (this.sim === null) return false;
    const eph = this.sim.ephemeralMsOf(m.channel);
    return eph !== null && this.sim.elapsed() - m.ts >= eph;
  }

  /** Notify clients of ephemeral messages that just expired (drives removal). */
  private sweepEphemeral(): void {
    for (const m of this.chat.all()) {
      if (this.notifiedExpired.has(m.seq) || !this.isExpired(m)) continue;
      this.notifiedExpired.add(m.seq);
      for (const c of this.clients) {
        if (c.role !== "guest" || visibleTo(m, c.id)) sseSend(c.res, "messageExpired", { seq: m.seq });
      }
    }
  }

  /**
   * Route a freshly-emitted batch of sim events. Performer scan readouts go
   * straight to the booth; everything guest-facing is composed into channel
   * messages, appended to the chat store, and delivered.
   */
  private fanout(events: SimEvent[]): void {
    for (const e of events) {
      if (e.type === "respond") this.toPrime(e.to, "response", { text: e.text });
      if (e.type === "choicePrompted" && e.person !== null) {
        this.decisionChannels.set(e.person, decisionChannelFor(events, e.person));
      }
      // Mods (the editor's Run mode) also get the raw sim feed, so the
      // story-graph overlay lights beats up as they fire and a future
      // in-editor simulator can mirror the whole run.
      for (const c of this.clients) if (c.role === "mod") sseSend(c.res, "sim", e);
    }
    for (const m of this.chat.append(composeGuestMessages(this.sim ?? EMPTY_SIM, events))) this.deliverMessage(m);
    for (const id of [...this.decisionChannels.keys()]) {
      if (this.sim?.pendingChoiceFor(id) == null) this.decisionChannels.delete(id);
    }
    this.pushSnapshots();
  }

  // -- persistence (event-sourced journal) ---------------------------------

  private flushTick(): void {
    if (this.pendingTickMs > 0) {
      this.store.appendCommand("tick", [this.pendingTickMs]);
      this.pendingTickMs = 0;
    }
  }

  /** Apply a sim mutation *and* journal it, so it survives a restart. */
  private commit(m: Mutation, ...args: unknown[]): SimEvent[] {
    this.flushTick();
    this.store.appendCommand(m, args);
    return (this.sim![m] as (...a: unknown[]) => SimEvent[])(...args);
  }

  private persistSessions(): void {
    this.store.saveSessions(this.sessions.entries());
  }

  private resetChat(): void {
    this.chat.clear();
    this.store.clearHidden();
    this.decisionChannels.clear();
  }

  private persistMeta(): void {
    this.store.saveMeta({
      version: 1,
      scenarioName: this.scenarioName,
      scenarioSource: this.scenarioSource,
      phase: this.phase,
    });
  }

  private loadScenario(source: string, name: string): void {
    this.sim = Sim.fromSources(source);
    this.scenarioSource = source;
    this.scenarioName = name;
    this.phase = "paused";
  }

  // -- autonomous clock ----------------------------------------------------

  private startTicker(): void {
    if (this.ticker !== null) return;
    this.ticker = setInterval(() => {
      if (this.sim !== null && this.phase === "open") {
        const evs = this.sim.tick(1000);
        this.pendingTickMs += 1000;
        if (evs.length > 0) {
          this.flushTick();
          this.fanout(evs);
        } else if (this.pendingTickMs >= 15000) {
          this.flushTick();
        }
        this.sweepEphemeral();
      }
    }, 1000);
  }

  private stopTicker(): void {
    if (this.ticker !== null) {
      clearInterval(this.ticker);
      this.ticker = null;
    }
  }

  /**
   * Open the doors: load the scenario on first open, start the clock, and
   * let guests register. The control-plane equivalent of `/api/mod/start`.
   */
  openDoors(): void {
    if (this.sim === null) {
      this.loadScenario(this.scenarioSource, this.scenarioName);
      this.store.clearJournal();
      this.resetChat();
      this.pendingTickMs = 0;
    }
    this.phase = "open";
    this.persistMeta();
    this.startTicker();
    this.pushSnapshots();
  }

  /** Pause the event: stop the clock, keep all state. `/api/mod/stop`. */
  pause(): void {
    this.flushTick();
    this.phase = "paused";
    this.stopTicker();
    this.persistMeta();
    this.pushSnapshots();
  }

  /** Tear the runtime down (stop the clock, drop SSE clients). */
  dispose(): void {
    this.stopTicker();
    for (const c of this.clients) {
      try {
        c.res.end();
      } catch {
        /* client already gone */
      }
    }
    this.clients.clear();
  }

  // -- restart recovery ----------------------------------------------------

  /**
   * Rebuild live state from disk by replaying the journal into a fresh sim.
   * Returns `null` when this event has no persisted state yet.
   */
  restore(): Restored | null {
    const meta = this.store.loadMeta();
    if (meta === null) return null;
    this.loadScenario(meta.scenarioSource, meta.scenarioName); // sets phase → paused
    let events = 0;
    for (const e of this.store.readJournal()) {
      const fn = (this.sim as unknown as Record<string, unknown>)[e.m];
      if (typeof fn === "function") {
        try {
          const out = (fn as (...a: unknown[]) => unknown).apply(this.sim, e.a);
          events++;
          if (Array.isArray(out)) {
            const batch = out as SimEvent[];
            for (const ev of batch) {
              if (ev.type === "choicePrompted" && ev.person !== null) {
                this.decisionChannels.set(ev.person, decisionChannelFor(batch, ev.person));
              }
            }
            this.chat.append(composeGuestMessages(this.sim!, batch));
          }
        } catch {
          /* tolerate a single bad/torn entry rather than abort recovery */
        }
      }
    }
    this.chat.loadHidden(this.store.loadHidden());
    for (const m of this.chat.all()) if (this.isExpired(m)) this.notifiedExpired.add(m.seq);
    for (const id of [...this.decisionChannels.keys()]) {
      if (this.sim?.pendingChoiceFor(id) == null) this.decisionChannels.delete(id);
    }
    this.phase = meta.phase;
    const entries = this.store.loadSessions();
    this.sessions.load(entries);
    if (this.phase === "open") this.startTicker();
    return { guests: this.sim?.persons.size ?? 0, events, sessions: entries.length };
  }

  // -- request handling ----------------------------------------------------

  /**
   * Handle one event-scoped request. `path` is already relative to this
   * event (the `/e/:eventId` prefix, if any, has been stripped by the
   * router). `opts.moderator` is set by the router when the caller is the
   * owning author (a BetterAuth session), granting mod access without a code.
   * Returns true when the request matched an event route.
   */
  async handle(
    req: IncomingMessage,
    res: ServerResponse,
    method: string,
    path: string,
    url: URL,
    opts: { moderator?: boolean } = {},
  ): Promise<boolean> {
    // --- SSE stream ---
    if (method === "GET" && path === "/events") {
      const role = (url.searchParams.get("role") as Client["role"] | null) ?? "mod";
      const id = url.searchParams.get("id") ?? "";
      res.writeHead(200, {
        "content-type": "text/event-stream",
        "cache-control": "no-cache",
        connection: "keep-alive",
        "access-control-allow-origin": "*",
      });
      res.write(":ok\n\n");
      const client: Client = { role, id, res };
      this.clients.add(client);
      sseSend(res, "snapshot", this.snapshotFor(client));
      // A moderator sees the full feed *including* hidden messages (greyed in
      // the UI) so they can un-hide; performers see the public feed only.
      sseSend(
        res,
        "history",
        role === "guest"
          ? this.chat.historyFor(id, false)
          : role === "mod"
            ? [...this.chat.all()]
            : this.chat.all().filter((m) => !m.hidden),
      );
      const ping = setInterval(() => res.write(":ping\n\n"), 25000);
      req.on("close", () => {
        clearInterval(ping);
        this.clients.delete(client);
      });
      return true;
    }

    // --- read-only state snapshot ---
    if (method === "GET" && path === "/api/state") {
      const role = url.searchParams.get("role") ?? "mod";
      const id = url.searchParams.get("id") ?? "";
      const view =
        role === "guest"
          ? guestView(this.reqSim(), id, this.decisionChannels.get(id) ?? null)
          : role === "prime"
            ? primeView(this.sim, id)
            : modView(this.sim, this.phase, this.scenarioName);
      sendJson(res, 200, view);
      return true;
    }

    // --- a participant's full thread history ---
    if (method === "GET" && path === "/api/history") {
      const id = url.searchParams.get("id") ?? "";
      const token = (req.headers["x-loom-token"] as string | undefined) || url.searchParams.get("token") || undefined;
      const admin = this.sessions.canModerate(token);
      sendJson(res, 200, { messages: this.chat.historyFor(id, admin).filter((m) => !this.isExpired(m)) });
      return true;
    }

    if (method !== "POST") return false;

    const body = await readBody(req);

    // --- guest / performer / login actions ---
    if (this.handlePost(req, res, path, body)) return true;

    // --- admin-only actions ---
    // Authorized by a per-event mod token OR by the owning author's session
    // (opts.moderator) — the same capability, two front doors.
    if (path.startsWith("/api/mod/")) {
      if (!opts.moderator && !this.sessions.canModerate(tokenOf(req, body))) {
        sendJson(res, 403, { error: "moderators only" });
        return true;
      }
      return this.handleMod(res, path, body);
    }

    return false;
  }

  /**
   * Non-admin POST routes (guest actions, performer chat, prime/mod login).
   * Returns true when `path` matched one of them.
   */
  private handlePost(req: IncomingMessage, res: ServerResponse, path: string, body: Record<string, unknown>): boolean {
    const open = this.open;
    switch (path) {
      // --- guest actions ---
      case "/api/guest/register": {
        if (!passOk(str(body, "passcode"), this.codes.event)) {
          sendJson(res, 403, { error: "wrong event code" });
          return true;
        }
        if (!open) {
          sendJson(res, 409, { error: "doors are closed" });
          return true;
        }
        const name = str(body, "name") || "Guest";
        const id = `g-${randomUUID().slice(0, 6)}`;
        this.fanout(this.commit("createPerson", id, name));
        sendJson(res, 200, { id, name });
        return true;
      }
      case "/api/guest/join": {
        if (!open) {
          sendJson(res, 409, { error: "doors are closed" });
          return true;
        }
        this.fanout(this.commit("join", str(body, "id"), str(body, "faction")));
        sendJson(res, 200, { ok: true });
        return true;
      }
      case "/api/guest/defect": {
        if (!open) {
          sendJson(res, 409, { error: "doors are closed" });
          return true;
        }
        this.fanout(this.commit("defect", str(body, "id"), str(body, "to")));
        sendJson(res, 200, { ok: true });
        return true;
      }
      case "/api/guest/choose": {
        if (!open) {
          sendJson(res, 409, { error: "doors are closed" });
          return true;
        }
        const idx = Number(body["index"] ?? -1);
        this.fanout(this.commit("choose", str(body, "id"), idx));
        sendJson(res, 200, { ok: true });
        return true;
      }
      case "/api/guest/escape": {
        if (!open) {
          sendJson(res, 409, { error: "doors are closed" });
          return true;
        }
        this.fanout(this.commit("escape", str(body, "id")));
        sendJson(res, 200, { ok: true });
        return true;
      }
      case "/api/guest/say": {
        if (!open) {
          sendJson(res, 409, { error: "doors are closed" });
          return true;
        }
        const id = str(body, "id");
        if (!this.sim!.persons.has(id)) {
          sendJson(res, 404, { error: "unknown guest" });
          return true;
        }
        const channel = str(body, "channel") || "lobby";
        const text = str(body, "text").trim();
        if (text === "") {
          sendJson(res, 400, { error: "empty message" });
          return true;
        }
        if (!this.sim!.canPost(id, channel)) {
          sendJson(res, 403, { error: "you can't post here" });
          return true;
        }
        const wait = this.sim!.slowModeRemainingMs(id, channel);
        if (wait > 0) {
          sendJson(res, 429, { error: `slow mode — wait ${Math.ceil(wait / 1000)}s`, retryMs: wait });
          return true;
        }
        const parentSeq = this.sim!.threadableOf(channel) && body["parentSeq"] != null ? Number(body["parentSeq"]) : null;
        this.fanout(this.commit("say", id, channel, text, parentSeq));
        sendJson(res, 200, { ok: true });
        return true;
      }
      case "/api/guest/channel/invite": {
        if (!open) {
          sendJson(res, 409, { error: "doors are closed" });
          return true;
        }
        const id = str(body, "id");
        if (!this.sim!.persons.has(id)) {
          sendJson(res, 404, { error: "unknown guest" });
          return true;
        }
        this.fanout(this.commit("inviteToChannel", id, str(body, "person"), str(body, "channel")));
        sendJson(res, 200, { ok: true });
        return true;
      }
      case "/api/guest/channel/leave": {
        if (!open) {
          sendJson(res, 409, { error: "doors are closed" });
          return true;
        }
        const id = str(body, "id");
        if (!this.sim!.persons.has(id)) {
          sendJson(res, 404, { error: "unknown guest" });
          return true;
        }
        this.fanout(this.commit("leaveChannel", id, str(body, "channel")));
        sendJson(res, 200, { ok: true });
        return true;
      }

      // --- performer (prime) login: grants the `character` capability ---
      case "/api/prime/login": {
        const character = str(body, "character");
        if (!passOk(str(body, "passcode"), this.codes.prime)) {
          sendJson(res, 403, { error: "bad passcode" });
          return true;
        }
        if (this.sim !== null && !this.sim.model.characters.has(character)) {
          sendJson(res, 404, { error: "unknown character" });
          return true;
        }
        let token = tokenOf(req, body);
        if (token && this.sessions.grant(token, { character })) {
          /* upgraded in place */
        } else {
          token = randomUUID();
          this.sessions.set(token, { character, admin: false });
        }
        this.persistSessions();
        sendJson(res, 200, { token, character, admin: this.sessions.canModerate(token) });
        return true;
      }

      // --- unified scan: capability decides what it does ---
      case "/api/scan": {
        const token = tokenOf(req, body);
        if (!this.sessions.canScan(token)) {
          sendJson(res, 403, { error: "no scan capability — sign in" });
          return true;
        }
        if (!open) {
          sendJson(res, 409, { error: "doors are closed" });
          return true;
        }
        const target = str(body, "target");
        if (!this.sim!.persons.has(target)) {
          sendJson(res, 404, { error: "unknown guest" });
          return true;
        }
        const admin = this.sessions.canModerate(token);
        const chosen = str(body, "as");
        let scanAs = this.sessions.characterOf(token);
        if (chosen && admin) {
          if (!this.sim!.model.characters.has(chosen)) {
            sendJson(res, 404, { error: "unknown character" });
            return true;
          }
          scanAs = chosen;
        }
        let responses: string[] = [];
        if (scanAs !== null) {
          const events = this.commit("scan", scanAs, target);
          responses = events
            .filter((e): e is Extract<SimEvent, { type: "respond" }> => e.type === "respond" && e.to === scanAs)
            .map((e) => e.text);
          this.fanout(events);
        }
        sendJson(res, 200, {
          ok: true,
          scannedAs: scanAs,
          canModerate: admin,
          responses,
          guest: admin
            ? rosterRow(this.sim!, target)
            : { id: target, name: this.sim!.persons.get(target)?.name ?? target, captured: this.sim!.isCaptured(target) },
        });
        return true;
      }

      // --- performer types into a channel (hybrid chat) ---
      case "/api/prime/say": {
        const token = tokenOf(req, body);
        const character = this.sessions.characterOf(token);
        if (!character) {
          sendJson(res, 403, { error: "no character — sign in" });
          return true;
        }
        if (!open) {
          sendJson(res, 409, { error: "doors are closed" });
          return true;
        }
        const text = str(body, "text").trim();
        if (text === "") {
          sendJson(res, 400, { error: "empty message" });
          return true;
        }
        let channel = str(body, "channel") || "lobby";
        let audience: "all" | string[] | undefined;
        if (channel.startsWith("guest:")) {
          const gid = channel.slice("guest:".length);
          if (!this.sim!.persons.has(gid)) {
            sendJson(res, 404, { error: "unknown guest" });
            return true;
          }
          channel = `dm:${character}`;
          audience = [gid];
        }
        const parentSeq = this.sim!.threadableOf(channel) && body["parentSeq"] != null ? Number(body["parentSeq"]) : null;
        this.fanout(this.commit("say", character, channel, text, parentSeq, audience));
        sendJson(res, 200, { ok: true });
        return true;
      }
      case "/api/prime/channel/invite": {
        const character = this.sessions.characterOf(tokenOf(req, body));
        if (!character) {
          sendJson(res, 403, { error: "no character — sign in" });
          return true;
        }
        if (!open) {
          sendJson(res, 409, { error: "doors are closed" });
          return true;
        }
        this.fanout(this.commit("inviteToChannel", character, str(body, "person"), str(body, "channel")));
        sendJson(res, 200, { ok: true });
        return true;
      }
      case "/api/prime/channel/leave": {
        const character = this.sessions.characterOf(tokenOf(req, body));
        if (!character) {
          sendJson(res, 403, { error: "no character — sign in" });
          return true;
        }
        if (!open) {
          sendJson(res, 409, { error: "doors are closed" });
          return true;
        }
        this.fanout(this.commit("leaveChannel", character, str(body, "channel")));
        sendJson(res, 200, { ok: true });
        return true;
      }

      // --- moderator login: grants the `admin` capability ---
      case "/api/mod/login": {
        if (!passOk(str(body, "passcode"), this.codes.mod)) {
          sendJson(res, 403, { error: "bad passcode" });
          return true;
        }
        let token = tokenOf(req, body);
        if (token && this.sessions.grant(token, { admin: true })) {
          /* upgraded in place */
        } else {
          token = randomUUID();
          this.sessions.set(token, { character: null, admin: true });
        }
        this.persistSessions();
        sendJson(res, 200, {
          token,
          character: this.sessions.characterOf(token),
          eventPass: this.codes.event,
          primePass: this.codes.prime,
          modPass: this.codes.mod,
        });
        return true;
      }
      default:
        return false;
    }
  }

  /** Admin-only routes (caller already proven to hold the mod capability). */
  private handleMod(res: ServerResponse, path: string, body: Record<string, unknown>): boolean {
    switch (path) {
      case "/api/mod/load": {
        const source = str(body, "source") || this.scenarioSource;
        const name = str(body, "name") || "custom";
        this.stopTicker();
        this.loadScenario(source, name);
        this.store.clearJournal();
        this.resetChat();
        this.pendingTickMs = 0;
        this.persistMeta();
        this.pushSnapshots();
        sendJson(res, 200, { ok: true, phase: this.phase });
        return true;
      }
      case "/api/mod/start": {
        this.openDoors();
        sendJson(res, 200, { ok: true, phase: this.phase });
        return true;
      }
      case "/api/mod/stop": {
        this.pause();
        sendJson(res, 200, { ok: true, phase: this.phase });
        return true;
      }
      case "/api/mod/reset": {
        this.stopTicker();
        this.loadScenario(this.scenarioSource, this.scenarioName);
        this.store.clearJournal();
        this.resetChat();
        this.pendingTickMs = 0;
        this.persistMeta();
        this.pushSnapshots();
        sendJson(res, 200, { ok: true, phase: this.phase });
        return true;
      }
      case "/api/mod/codes": {
        sendJson(res, 200, {
          eventPass: this.codes.event,
          primePass: this.codes.prime,
          modPass: this.codes.mod,
          joinUrl: this.joinBase(),
        });
        return true;
      }
      case "/api/mod/act": {
        if (this.sim === null) {
          sendJson(res, 409, { error: "no scenario loaded" });
          return true;
        }
        const id = str(body, "id");
        if (!this.sim.persons.has(id)) {
          sendJson(res, 404, { error: "unknown guest" });
          return true;
        }
        switch (str(body, "action")) {
          case "capture":
            this.fanout(this.commit("capture", id));
            break;
          case "release":
            this.fanout(this.commit("escape", id));
            break;
          case "signal":
            this.fanout(this.commit("signal", str(body, "name"), id));
            break;
          default:
            sendJson(res, 400, { error: "unknown action" });
            return true;
        }
        sendJson(res, 200, { ok: true, guest: rosterRow(this.sim, id) });
        return true;
      }
      case "/api/mod/signal": {
        if (this.sim === null) {
          sendJson(res, 409, { error: "no scenario loaded" });
          return true;
        }
        const subject = str(body, "subject");
        this.fanout(this.commit("signal", str(body, "name"), subject === "" ? undefined : subject));
        sendJson(res, 200, { ok: true });
        return true;
      }
      case "/api/mod/broadcast": {
        if (this.sim === null) {
          sendJson(res, 409, { error: "no scenario loaded" });
          return true;
        }
        const scope = str(body, "scope");
        const cue = str(body, "cue") || "cue";
        const synthetic: SimEvent = { type: "broadcast", cue, audience: this.sim.audienceFor(scope), scope };
        const stored = this.chat.append(composeGuestMessages(this.sim, [synthetic]));
        for (const m of stored) this.deliverMessage(m);
        this.pushSnapshots();
        sendJson(res, 200, { ok: true, reached: stored.reduce((n, m) => n + (m.audience === "all" ? -1 : m.audience.length), 0) });
        return true;
      }
      case "/api/mod/message": {
        const seq = Number(body["seq"] ?? -1);
        const hide = body["hidden"] === true;
        const m = this.chat.setHidden(seq, hide);
        if (m === null) {
          sendJson(res, 404, { error: "unknown message" });
          return true;
        }
        this.store.saveHidden(this.chat.hiddenSeqs());
        for (const c of this.clients) {
          if (c.role === "guest") {
            if (!visibleTo(m, c.id)) continue;
            if (m.hidden) sseSend(c.res, "messageModerated", { seq: m.seq, hidden: true });
            else sseSend(c.res, "message", m);
          } else {
            sseSend(c.res, "messageModerated", m);
          }
        }
        sendJson(res, 200, { ok: true, seq, hidden: m.hidden });
        return true;
      }
      case "/api/mod/say": {
        // The operator types into *any* room — as themselves ("Operator") or
        // in a character's voice (`as`). Unlike a guest `say`, this bypasses
        // the post policy (an operator can post into a read-only feed).
        if (this.sim === null) {
          sendJson(res, 409, { error: "no scenario loaded" });
          return true;
        }
        const text = str(body, "text").trim();
        if (text === "") {
          sendJson(res, 400, { error: "empty message" });
          return true;
        }
        const as = str(body, "as");
        const speaker = as !== "" ? as : "Operator";
        let channel = str(body, "channel") || "lobby";
        let audience: "all" | string[] | undefined;
        // Address one guest's DM thread: `channel: "guest:<id>"`.
        if (channel.startsWith("guest:")) {
          const gid = channel.slice("guest:".length);
          if (!this.sim.persons.has(gid)) {
            sendJson(res, 404, { error: "unknown guest" });
            return true;
          }
          channel = `dm:${speaker}`;
          audience = [gid];
        }
        const parentSeq = this.sim.threadableOf(channel) && body["parentSeq"] != null ? Number(body["parentSeq"]) : null;
        this.fanout(this.commit("say", speaker, channel, text, parentSeq, audience));
        sendJson(res, 200, { ok: true });
        return true;
      }
      case "/api/mod/set": {
        // Live-edit a guest's stats from the run panel's inspector.
        if (this.sim === null) {
          sendJson(res, 409, { error: "no scenario loaded" });
          return true;
        }
        const id = str(body, "id");
        if (!this.sim.persons.has(id)) {
          sendJson(res, 404, { error: "unknown guest" });
          return true;
        }
        const field = str(body, "field");
        const value = body["value"];
        switch (field) {
          case "score":
            this.fanout(this.commit("setScore", id, Number(value ?? 0)));
            break;
          case "faction": {
            const to = str(body, "value");
            if (to === "") {
              sendJson(res, 400, { error: "faction required" });
              return true;
            }
            this.fanout(this.commit("defect", id, to));
            break;
          }
          case "location": {
            const to = str(body, "value");
            if (to === "") {
              sendJson(res, 400, { error: "location required" });
              return true;
            }
            this.fanout(this.commit("arrive", id, to));
            break;
          }
          case "captured": {
            const on = value === true || value === "true";
            // `capture` is idempotent; `escape` only fires on a real transition.
            this.fanout(this.commit(on ? "capture" : "escape", id));
            break;
          }
          default:
            sendJson(res, 400, { error: "unknown field" });
            return true;
        }
        sendJson(res, 200, { ok: true, guest: rosterRow(this.sim, id) });
        return true;
      }
      case "/api/mod/beat": {
        // Fire a named story beat directly (booth live-patch).
        if (this.sim === null) {
          sendJson(res, 409, { error: "no scenario loaded" });
          return true;
        }
        const name = str(body, "name");
        if (name === "" || !this.sim.model.beats.has(name)) {
          sendJson(res, 404, { error: "unknown beat" });
          return true;
        }
        const subject = str(body, "subject");
        this.fanout(this.commit("fireBeat", name, subject === "" ? undefined : subject));
        sendJson(res, 200, { ok: true });
        return true;
      }
      case "/api/mod/scan": {
        // Scan a guest *as* a character — fires that character's scan hooks
        // against them (the beat-firing the operator console does via /api/scan,
        // reachable here through the owning author's session).
        if (this.sim === null) {
          sendJson(res, 409, { error: "no scenario loaded" });
          return true;
        }
        const as = str(body, "as");
        const target = str(body, "target");
        if (!this.sim.model.characters.has(as)) {
          sendJson(res, 404, { error: "unknown character" });
          return true;
        }
        if (!this.sim.persons.has(target)) {
          sendJson(res, 404, { error: "unknown guest" });
          return true;
        }
        this.fanout(this.commit("scan", as, target));
        sendJson(res, 200, { ok: true, guest: rosterRow(this.sim, target) });
        return true;
      }
      case "/api/mod/reveal": {
        // Expose a hidden faction (the secret-villain reveal).
        if (this.sim === null) {
          sendJson(res, 409, { error: "no scenario loaded" });
          return true;
        }
        const faction = str(body, "faction");
        if (!this.sim.model.factions.has(faction)) {
          sendJson(res, 404, { error: "unknown faction" });
          return true;
        }
        this.fanout(this.commit("reveal", faction));
        sendJson(res, 200, { ok: true });
        return true;
      }
      default:
        sendJson(res, 404, { error: "unknown mod action" });
        return true;
    }
  }
}
