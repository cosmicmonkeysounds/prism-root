//! Loom event server — hosts one live `Sim` and serves the role-based
//! web client to clients on the local network.
//!
//! Transport: Server-Sent Events for server→client push (state
//! snapshots, broadcasts, choice prompts) + JSON `fetch` POST for
//! client→server actions. No WebSocket dependency, so it works on any
//! phone browser on the wifi.
//!
//! Run: `pnpm --filter @loom/core serve` (or `npx tsx server/server.ts`).
//! Env: LOOM_PORT, LOOM_HOST, LOOM_EVENT_PASS, LOOM_MOD_PASS,
//! LOOM_PRIME_PASS (any passcode left unset is auto-generated and printed
//! at boot), LOOM_STATE_DIR (where restart-recovery state is kept),
//! LOOM_APP_DIST.

import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { readFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { networkInterfaces } from "node:os";
import { fileURLToPath, pathToFileURL } from "node:url";
import QRCode from "qrcode";

import { Sim, type SimEvent } from "../src/runtime/sim/index.ts";
import { guestView, modView, primeView, rosterRow, type RuntimePhase } from "./views.ts";
import { passOk, resolvePasscodes } from "./auth.ts";
import { SessionStore } from "./session.ts";
import { Store, type Mutation } from "./store.ts";
import { ChatStore, composeGuestMessages, decisionChannelFor, visibleTo, type ChatMessage } from "./chat.ts";
import { scenarioSource } from "../examples/load.ts";

const PORT = Number(process.env.LOOM_PORT ?? 7000);
const HOST = process.env.LOOM_HOST ?? "0.0.0.0";

// Restart-recovery state lives here; override with LOOM_STATE_DIR.
const STATE_DIR = process.env.LOOM_STATE_DIR ?? fileURLToPath(new URL("./.loom-state/", import.meta.url));
const store = new Store(STATE_DIR);

// Passcodes: env override → persisted (stable across restarts) → fresh.
const PASS = resolvePasscodes(process.env, store.loadCodes());
store.saveCodes(PASS);

const CONSOLE_HTML = readFileSync(new URL("./public/index.html", import.meta.url), "utf8");
// The default scenario is a multi-file project under `examples/`; the loader
// concatenates `main.loom` + the rest into one source (identical to bundling
// the files separately) so the journal-replay store keeps a single string.
const DEFAULT_SCENARIO = scenarioSource("escape-the-internet");

// The built participant app (`loom-play`). Defaults to the sibling
// package's `dist/`; override with LOOM_APP_DIST (absolute path).
const APP_DIST = process.env.LOOM_APP_DIST
  ? pathToFileURL(process.env.LOOM_APP_DIST.replace(/\/?$/, "/"))
  : new URL("../../play/dist/", import.meta.url);

const CONTENT_TYPES: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".json": "application/json",
  ".ico": "image/x-icon",
  ".png": "image/png",
  ".woff2": "font/woff2",
};

/** Serve a file from the built app dir; returns false if it isn't there. */
function serveAppFile(pathname: string, res: ServerResponse): boolean {
  const rel = pathname === "/" ? "index.html" : pathname.replace(/^\/+/, "");
  if (rel.includes("..")) return false;
  try {
    const buf = readFileSync(new URL(rel, APP_DIST));
    const ext = rel.slice(rel.lastIndexOf("."));
    res.writeHead(200, { "content-type": CONTENT_TYPES[ext] ?? "application/octet-stream" });
    res.end(buf);
    return true;
  } catch {
    return false;
  }
}

// ---------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------

interface State {
  sim: Sim | null;
  phase: RuntimePhase;
  scenarioName: string;
  scenarioSource: string;
}

const state: State = {
  sim: null,
  phase: "idle",
  scenarioName: "escape-the-internet",
  scenarioSource: DEFAULT_SCENARIO,
};

// Token → capabilities. One session can be a performer, an admin, or both.
const sessions = new SessionStore();

// Server-authoritative chat: every visible message, routed to a channel.
// Rebuilt deterministically from the journal on restart (see `restore`).
const chat = new ChatStore();

// Where each guest's currently-pending decision should dock (channel id).
// Ephemeral — re-derived during replay alongside the chat store.
const decisionChannels = new Map<string, string>();

// ---------------------------------------------------------------------
// SSE hub
// ---------------------------------------------------------------------

interface Client {
  role: "guest" | "prime" | "mod";
  id: string; // person id (guest), character (prime), or "" (mod)
  res: ServerResponse;
}

const clients = new Set<Client>();

function sseSend(res: ServerResponse, event: string, data: unknown): void {
  res.write(`event: ${event}\n`);
  res.write(`data: ${JSON.stringify(data)}\n\n`);
}

function snapshotFor(client: Client): unknown {
  if (client.role === "guest") return guestView(reqSim(), client.id, decisionChannels.get(client.id) ?? null);
  if (client.role === "prime") return primeView(state.sim, client.id);
  return modView(state.sim, state.phase, state.scenarioName);
}

/** A sim that's never null for guest views (returns an empty live sim). */
function reqSim(): Sim {
  return state.sim ?? EMPTY_SIM;
}
const EMPTY_SIM = Sim.fromSources("");

/** Push fresh snapshots to every connected client. */
function pushSnapshots(): void {
  for (const c of clients) sseSend(c.res, "snapshot", snapshotFor(c));
}

function toPrime(character: string, event: string, data: unknown): void {
  for (const c of clients) if (c.role === "prime" && c.id === character) sseSend(c.res, event, data);
}

/**
 * Deliver one composed message to the clients that should see it: guests
 * whose id is in the audience (hidden messages withheld), and every
 * performer/mod console (the full feed, so admins can moderate live).
 */
function deliverMessage(m: ChatMessage): void {
  for (const c of clients) {
    if (c.role === "guest") {
      if (!m.hidden && visibleTo(m, c.id)) sseSend(c.res, "message", m);
    } else {
      sseSend(c.res, "message", m);
    }
  }
}

/**
 * Route a freshly-emitted batch of sim events. Performer scan readouts go
 * straight to the booth; everything guest-facing is composed into channel
 * messages (the single source of truth), appended to the chat store, and
 * delivered. Pending decisions remember which channel they dock under.
 */
function fanout(events: SimEvent[]): void {
  for (const e of events) {
    if (e.type === "respond") toPrime(e.to, "response", { text: e.text });
    if (e.type === "choicePrompted" && e.person !== null) {
      decisionChannels.set(e.person, decisionChannelFor(events, e.person));
    }
  }
  for (const m of chat.append(composeGuestMessages(state.sim ?? EMPTY_SIM, events))) deliverMessage(m);
  // A consumed choice clears its dock so the next snapshot drops the badge.
  for (const id of [...decisionChannels.keys()]) {
    if (state.sim?.pendingChoiceFor(id) == null) decisionChannels.delete(id);
  }
  pushSnapshots();
}

// --- persistence (event-sourced journal, see store.ts) -----------------

// Elapsed sim time not yet written to the journal. Coalescing ticks keeps
// the journal small without changing replay: the sim's generator/timer
// loops are cumulative, so one `tick(15000)` ≡ fifteen `tick(1000)`.
let pendingTickMs = 0;
function flushTick(): void {
  if (pendingTickMs > 0) {
    store.appendCommand("tick", [pendingTickMs]);
    pendingTickMs = 0;
  }
}

/**
 * Apply a sim mutation *and* journal it, so it survives a restart. Any
 * accumulated clock time is flushed first to preserve command/tick order.
 */
function commit(m: Mutation, ...args: unknown[]): SimEvent[] {
  flushTick();
  store.appendCommand(m, args);
  return (state.sim![m] as (...a: unknown[]) => SimEvent[])(...args);
}

function persistSessions(): void {
  store.saveSessions(sessions.entries());
}

/** Wipe chat history + moderation when a fresh story timeline begins. */
function resetChat(): void {
  chat.clear();
  store.clearHidden();
  decisionChannels.clear();
}

function persistMeta(): void {
  store.saveMeta({
    version: 1,
    scenarioName: state.scenarioName,
    scenarioSource: state.scenarioSource,
    phase: state.phase,
  });
}

/**
 * Best LAN-reachable base URL for guest join QRs. A QR built from the
 * console's `location.origin` is `http://localhost:…` when the operator
 * opened the console locally — useless to a phone (localhost is the phone
 * itself). The server knows its real LAN IP, so it hands one out.
 */
function joinBase(): string {
  const urls = lanUrls(PORT);
  return urls.find((u) => !u.includes("localhost")) ?? urls[0]!;
}

// The autonomous clock: while the doors are open, tick the sim once a
// second so ambient generators + time-driven hooks advance.
let ticker: ReturnType<typeof setInterval> | null = null;
function startTicker(): void {
  if (ticker !== null) return;
  ticker = setInterval(() => {
    if (state.sim !== null && state.phase === "open") {
      const evs = state.sim.tick(1000);
      pendingTickMs += 1000;
      // Flush on activity (preserve bark timing) or every ~15s (bound loss).
      if (evs.length > 0) {
        flushTick();
        fanout(evs);
      } else if (pendingTickMs >= 15000) {
        flushTick();
      }
    }
  }, 1000);
}
function stopTicker(): void {
  if (ticker !== null) {
    clearInterval(ticker);
    ticker = null;
  }
}

// ---------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------

function sendJson(res: ServerResponse, status: number, body: unknown): void {
  const json = JSON.stringify(body);
  res.writeHead(status, { "content-type": "application/json", "access-control-allow-origin": "*" });
  res.end(json);
}

async function readBody(req: IncomingMessage): Promise<Record<string, unknown>> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) chunks.push(chunk as Buffer);
  if (chunks.length === 0) return {};
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8")) as Record<string, unknown>;
  } catch {
    return {};
  }
}

function str(body: Record<string, unknown>, key: string): string {
  const v = body[key];
  return typeof v === "string" ? v : "";
}

/** The caller's session token, from the header or the body (`""` → undefined). */
function tokenOf(req: IncomingMessage, body: Record<string, unknown>): string | undefined {
  return (req.headers["x-loom-token"] as string | undefined) || str(body, "token") || undefined;
}

function loadScenario(source: string, name: string): void {
  state.sim = Sim.fromSources(source);
  state.scenarioSource = source;
  state.scenarioName = name;
  state.phase = "paused";
}

// ---------------------------------------------------------------------
// Routing
// ---------------------------------------------------------------------

const server = createServer((req, res) => {
  void route(req, res).catch((err) => {
    sendJson(res, 500, { error: String(err) });
  });
});

async function route(req: IncomingMessage, res: ServerResponse): Promise<void> {
  const url = new URL(req.url ?? "/", `http://${req.headers.host ?? "localhost"}`);
  const path = url.pathname;
  const method = req.method ?? "GET";

  if (method === "OPTIONS") {
    res.writeHead(204, {
      "access-control-allow-origin": "*",
      "access-control-allow-headers": "content-type, x-loom-token",
      "access-control-allow-methods": "GET, POST, OPTIONS",
    });
    res.end();
    return;
  }

  // --- operator console (vanilla, no build step) ---
  if (method === "GET" && path === "/console") {
    res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    res.end(CONSOLE_HTML);
    return;
  }
  // --- the built participant app (loom-play) at `/` + its static assets ---
  if (
    method === "GET" &&
    (path === "/" || path === "/index.html" || path.startsWith("/assets/") || path === "/favicon.ico")
  ) {
    if (serveAppFile(path, res)) return;
    // Not built yet — fall back to the console so the server is still usable.
    res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    res.end(CONSOLE_HTML);
    return;
  }

  // --- QR code for a guest id ---
  if (method === "GET" && path === "/api/qr") {
    const text = url.searchParams.get("text") ?? "";
    const svg = await QRCode.toString(text || " ", { type: "svg", margin: 1, width: 240 });
    res.writeHead(200, { "content-type": "image/svg+xml", "cache-control": "no-store" });
    res.end(svg);
    return;
  }

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
    clients.add(client);
    sseSend(res, "snapshot", snapshotFor(client));
    // Replay history so a re-login (or late arrival) sees the conversation so
    // far — a guest gets their own threads; a performer/mod console gets the
    // whole room's feed (visible messages) for context + moderation.
    sseSend(res, "history", role === "guest" ? chat.historyFor(id, false) : chat.all().filter((m) => !m.hidden));
    const ping = setInterval(() => res.write(":ping\n\n"), 25000);
    req.on("close", () => {
      clearInterval(ping);
      clients.delete(client);
    });
    return;
  }

  // --- read-only state snapshot ---
  if (method === "GET" && path === "/api/state") {
    const role = url.searchParams.get("role") ?? "mod";
    const id = url.searchParams.get("id") ?? "";
    const view =
      role === "guest"
        ? guestView(reqSim(), id, decisionChannels.get(id) ?? null)
        : role === "prime"
          ? primeView(state.sim, id)
          : modView(state.sim, state.phase, state.scenarioName);
    sendJson(res, 200, view);
    return;
  }

  // --- a participant's full thread history (since the event began) ---
  // Guests get their own (hidden withheld); an admin token may inspect any
  // guest's threads *including* hidden messages, to moderate them.
  if (method === "GET" && path === "/api/history") {
    const id = url.searchParams.get("id") ?? "";
    const token = (req.headers["x-loom-token"] as string | undefined) || url.searchParams.get("token") || undefined;
    const admin = sessions.canModerate(token);
    sendJson(res, 200, { messages: chat.historyFor(id, admin) });
    return;
  }

  if (method !== "POST") {
    sendJson(res, 404, { error: "not found" });
    return;
  }

  const body = await readBody(req);
  const open = state.phase === "open" && state.sim !== null;

  switch (path) {
    // --- guest actions ---
    case "/api/guest/register": {
      if (!passOk(str(body, "passcode"), PASS.event)) return void sendJson(res, 403, { error: "wrong event code" });
      if (!open) return void sendJson(res, 409, { error: "doors are closed" });
      const name = str(body, "name") || "Guest";
      const id = `g-${randomUUID().slice(0, 6)}`;
      fanout(commit("createPerson", id, name));
      return void sendJson(res, 200, { id, name });
    }
    case "/api/guest/join": {
      if (!open) return void sendJson(res, 409, { error: "doors are closed" });
      fanout(commit("join", str(body, "id"), str(body, "faction")));
      return void sendJson(res, 200, { ok: true });
    }
    case "/api/guest/defect": {
      if (!open) return void sendJson(res, 409, { error: "doors are closed" });
      fanout(commit("defect", str(body, "id"), str(body, "to")));
      return void sendJson(res, 200, { ok: true });
    }
    case "/api/guest/choose": {
      if (!open) return void sendJson(res, 409, { error: "doors are closed" });
      const idx = Number(body["index"] ?? -1);
      fanout(commit("choose", str(body, "id"), idx));
      return void sendJson(res, 200, { ok: true });
    }
    case "/api/guest/escape": {
      if (!open) return void sendJson(res, 409, { error: "doors are closed" });
      fanout(commit("escape", str(body, "id")));
      return void sendJson(res, 200, { ok: true });
    }

    // --- performer (prime) login: grants the `character` capability ---
    case "/api/prime/login": {
      const character = str(body, "character");
      if (!passOk(str(body, "passcode"), PASS.prime)) return void sendJson(res, 403, { error: "bad passcode" });
      if (state.sim !== null && !state.sim.model.characters.has(character)) {
        return void sendJson(res, 404, { error: "unknown character" });
      }
      // Upgrade the caller's existing session if they have one (e.g. an
      // admin picking up a character); otherwise mint a fresh token.
      let token = tokenOf(req, body);
      if (token && sessions.grant(token, { character })) {
        /* upgraded in place */
      } else {
        token = randomUUID();
        sessions.set(token, { character, admin: false });
      }
      persistSessions();
      return void sendJson(res, 200, { token, character, admin: sessions.canModerate(token) });
    }

    // --- unified scan: capability decides what it does -------------------
    case "/api/scan": {
      const token = tokenOf(req, body);
      if (!sessions.canScan(token)) return void sendJson(res, 403, { error: "no scan capability — sign in" });
      if (!open) return void sendJson(res, 409, { error: "doors are closed" });
      const target = str(body, "target");
      if (!state.sim!.persons.has(target)) return void sendJson(res, 404, { error: "unknown guest" });
      const admin = sessions.canModerate(token);
      // Scanner identity. A performer scans as their own character. An admin
      // may instead pick any character via `as` (firing that character's
      // story hooks); a blank `as` from a headless admin means a silent,
      // moderation-only scan.
      const chosen = str(body, "as");
      let scanAs = sessions.characterOf(token);
      if (chosen && admin) {
        if (!state.sim!.model.characters.has(chosen)) return void sendJson(res, 404, { error: "unknown character" });
        scanAs = chosen;
      }
      // A character identity → run the story scan (hooks + the `respond`
      // that streams back to that booth). No character → silent.
      let responses: string[] = [];
      if (scanAs !== null) {
        const events = commit("scan", scanAs, target);
        // The scanner's readout — `respond` lines addressed to this scanner.
        responses = events
          .filter((e): e is Extract<SimEvent, { type: "respond" }> => e.type === "respond" && e.to === scanAs)
          .map((e) => e.text);
        fanout(events);
      }
      // Admins also get the guest identified for moderation; performers get
      // a basic confirmation (their story beat also arrives over SSE).
      return void sendJson(res, 200, {
        ok: true,
        scannedAs: scanAs,
        canModerate: admin,
        responses,
        guest: admin
          ? rosterRow(state.sim!, target)
          : { id: target, name: state.sim!.persons.get(target)?.name ?? target, captured: state.sim!.isCaptured(target) },
      });
    }

    // --- moderator login: grants the `admin` capability -----------------
    case "/api/mod/login": {
      if (!passOk(str(body, "passcode"), PASS.mod)) return void sendJson(res, 403, { error: "bad passcode" });
      // Upgrade the caller's existing session (a performer becoming an admin
      // keeps their token + character); otherwise mint a headless admin.
      let token = tokenOf(req, body);
      if (token && sessions.grant(token, { admin: true })) {
        /* upgraded in place */
      } else {
        token = randomUUID();
        sessions.set(token, { character: null, admin: true });
      }
      persistSessions();
      // The mod is the trusted operator — hand back every code so the
      // console can show them: event (for guests), prime (for performers),
      // and mod itself (to recruit a co-moderator). Echoing mod back leaks
      // nothing: the caller just proved they already know it.
      return void sendJson(res, 200, {
        token,
        character: sessions.characterOf(token),
        eventPass: PASS.event,
        primePass: PASS.prime,
        modPass: PASS.mod,
      });
    }
    default:
      break;
  }

  // everything below requires the admin capability
  if (path.startsWith("/api/mod/")) {
    if (!sessions.canModerate(tokenOf(req, body))) return void sendJson(res, 403, { error: "moderators only" });
    switch (path) {
      case "/api/mod/load": {
        const source = str(body, "source") || DEFAULT_SCENARIO;
        const name = str(body, "name") || "custom";
        stopTicker();
        loadScenario(source, name);
        store.clearJournal(); // a new story starts a fresh timeline
        resetChat();
        pendingTickMs = 0;
        persistMeta();
        pushSnapshots();
        return void sendJson(res, 200, { ok: true, phase: state.phase });
      }
      case "/api/mod/start": {
        if (state.sim === null) {
          loadScenario(DEFAULT_SCENARIO, "escape-the-internet");
          store.clearJournal();
          resetChat();
          pendingTickMs = 0;
        }
        state.phase = "open";
        persistMeta();
        startTicker();
        pushSnapshots();
        return void sendJson(res, 200, { ok: true, phase: state.phase });
      }
      case "/api/mod/stop": {
        flushTick();
        state.phase = "paused";
        stopTicker();
        persistMeta();
        pushSnapshots();
        return void sendJson(res, 200, { ok: true, phase: state.phase });
      }
      case "/api/mod/reset": {
        stopTicker();
        loadScenario(state.scenarioSource, state.scenarioName);
        store.clearJournal(); // wipe the timeline; the doors reopen empty
        resetChat();
        pendingTickMs = 0;
        persistMeta();
        pushSnapshots();
        return void sendJson(res, 200, { ok: true, phase: state.phase });
      }
      case "/api/mod/codes": {
        // Fresh codes for the dashboard (a cached login may predate a code
        // change), plus the LAN URL the join QR should point at.
        return void sendJson(res, 200, {
          eventPass: PASS.event,
          primePass: PASS.prime,
          modPass: PASS.mod,
          joinUrl: joinBase(),
        });
      }
      case "/api/mod/act": {
        // Moderate a scanned guest. Reuses sim primitives so the action set
        // grows by adding cases, not endpoints.
        if (state.sim === null) return void sendJson(res, 409, { error: "no scenario loaded" });
        const id = str(body, "id");
        if (!state.sim.persons.has(id)) return void sendJson(res, 404, { error: "unknown guest" });
        switch (str(body, "action")) {
          case "capture":
            fanout(commit("capture", id));
            break;
          case "release":
            fanout(commit("escape", id));
            break;
          case "signal":
            fanout(commit("signal", str(body, "name"), id));
            break;
          default:
            return void sendJson(res, 400, { error: "unknown action" });
        }
        return void sendJson(res, 200, { ok: true, guest: rosterRow(state.sim, id) });
      }
      case "/api/mod/signal": {
        if (state.sim === null) return void sendJson(res, 409, { error: "no scenario loaded" });
        const subject = str(body, "subject");
        fanout(commit("signal", str(body, "name"), subject === "" ? undefined : subject));
        return void sendJson(res, 200, { ok: true });
      }
      case "/api/mod/broadcast": {
        if (state.sim === null) return void sendJson(res, 409, { error: "no scenario loaded" });
        // An ad-hoc operator broadcast: route a synthetic broadcast event
        // through the same composer so it lands in the right channel and can
        // be moderated. (Not journaled — operator nudges don't replay.)
        const scope = str(body, "scope");
        const cue = str(body, "cue") || "cue";
        const synthetic: SimEvent = { type: "broadcast", cue, audience: state.sim.audienceFor(scope), scope };
        const stored = chat.append(composeGuestMessages(state.sim, [synthetic]));
        for (const m of stored) deliverMessage(m);
        pushSnapshots();
        return void sendJson(res, 200, { ok: true, reached: stored.reduce((n, m) => n + (m.audience === "all" ? -1 : m.audience.length), 0) });
      }
      case "/api/mod/message": {
        // Hide or restore a single message. Guests in its audience see it
        // vanish / reappear; performer consoles get the updated flag.
        const seq = Number(body["seq"] ?? -1);
        const hide = body["hidden"] === true;
        const m = chat.setHidden(seq, hide);
        if (m === null) return void sendJson(res, 404, { error: "unknown message" });
        store.saveHidden(chat.hiddenSeqs());
        for (const c of clients) {
          if (c.role === "guest") {
            if (!visibleTo(m, c.id)) continue;
            if (m.hidden) sseSend(c.res, "messageModerated", { seq: m.seq, hidden: true });
            else sseSend(c.res, "message", m); // restored → re-deliver in full
          } else {
            sseSend(c.res, "messageModerated", m); // admins: full payload + flag
          }
        }
        return void sendJson(res, 200, { ok: true, seq, hidden: m.hidden });
      }
      default:
        return void sendJson(res, 404, { error: "unknown mod action" });
    }
  }

  sendJson(res, 404, { error: "not found" });
}

// ---------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------

/** Summary of what a restart-recovery brought back, for the boot banner. */
interface Restored {
  guests: number;
  events: number;
  sessions: number;
}

/**
 * Rebuild live state from disk by replaying the journal into a fresh sim
 * (see store.ts). Returns `null` on a clean first run. Makes a process
 * restart transparent: nobody re-authenticates, nobody loses their place.
 */
function restore(): Restored | null {
  const meta = store.loadMeta();
  if (meta === null) return null;
  loadScenario(meta.scenarioSource, meta.scenarioName); // sets phase → paused
  let events = 0;
  for (const e of store.readJournal()) {
    const fn = (state.sim as unknown as Record<string, unknown>)[e.m];
    if (typeof fn === "function") {
      try {
        const out = (fn as (...a: unknown[]) => unknown).apply(state.sim, e.a);
        events++;
        // Re-derive chat from the same events live operation composed from,
        // in the same order → identical `seq`s, so persisted moderation
        // (keyed by seq) lines back up below.
        if (Array.isArray(out)) {
          const batch = out as SimEvent[];
          for (const ev of batch) {
            if (ev.type === "choicePrompted" && ev.person !== null) {
              decisionChannels.set(ev.person, decisionChannelFor(batch, ev.person));
            }
          }
          chat.append(composeGuestMessages(state.sim!, batch));
        }
      } catch {
        /* tolerate a single bad/torn entry rather than abort recovery */
      }
    }
  }
  chat.loadHidden(store.loadHidden());
  for (const id of [...decisionChannels.keys()]) {
    if (state.sim?.pendingChoiceFor(id) == null) decisionChannels.delete(id);
  }
  state.phase = meta.phase;
  const entries = store.loadSessions();
  sessions.load(entries);
  if (state.phase === "open") startTicker();
  return { guests: state.sim?.persons.size ?? 0, events, sessions: entries.length };
}

const restored = restore();

server.listen(PORT, HOST, () => {
  const urls = lanUrls(PORT);
  let appBuilt = true;
  try {
    readFileSync(new URL("index.html", APP_DIST));
  } catch {
    appBuilt = false;
  }
  process.stdout.write(`\n  Loom event server — "${state.scenarioName}"  (phase: ${state.phase})\n\n`);
  process.stdout.write(`  Passcodes — share with the room:\n`);
  process.stdout.write(`    🎟️  Guest event code  : ${PASS.event}\n`);
  process.stdout.write(`    🎭  Performer passcode: ${PASS.prime}\n`);
  process.stdout.write(`    🛡️  Moderator passcode: ${PASS.mod}\n`);
  process.stdout.write(`    (sign in at /console with the moderator code; also saved to ${STATE_DIR}/codes.json)\n\n`);
  if (restored) {
    process.stdout.write(
      `  ↻ Restored ${restored.guests} guest(s), ${restored.events} event(s), ${restored.sessions} live session(s) from ${STATE_DIR}\n\n`,
    );
  }
  for (const u of urls) process.stdout.write(`  → ${u}  (participant app)\n`);
  process.stdout.write(`  → ${urls[0]}/console  (operator console)\n\n`);
  if (appBuilt) {
    process.stdout.write(`  Guests/performers use the app at /. Mods open the doors from /console.\n\n`);
  } else {
    process.stdout.write(`  ⚠️  participant app not built — run \`pnpm --filter loom-play build\`\n`);
    process.stdout.write(`     (or for live dev: \`cd ../play && pnpm dev\` and use :5174). / falls back to the console.\n\n`);
  }
});

function lanUrls(port: number): string[] {
  const out = [`http://localhost:${port}`];
  const ifaces = networkInterfaces();
  for (const list of Object.values(ifaces)) {
    for (const net of list ?? []) {
      if (net.family === "IPv4" && !net.internal) out.push(`http://${net.address}:${port}`);
    }
  }
  return out;
}
