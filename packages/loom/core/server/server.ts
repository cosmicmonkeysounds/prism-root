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
import { guestView, modView, primeView, type RuntimePhase } from "./views.ts";
import { passOk, resolvePasscodes } from "./auth.ts";
import { Store, type Mutation } from "./store.ts";

const PORT = Number(process.env.LOOM_PORT ?? 7000);
const HOST = process.env.LOOM_HOST ?? "0.0.0.0";

// Restart-recovery state lives here; override with LOOM_STATE_DIR.
const STATE_DIR = process.env.LOOM_STATE_DIR ?? fileURLToPath(new URL("./.loom-state/", import.meta.url));
const store = new Store(STATE_DIR);

// Passcodes: env override → persisted (stable across restarts) → fresh.
const PASS = resolvePasscodes(process.env, store.loadCodes());
store.saveCodes(PASS);

const CONSOLE_HTML = readFileSync(new URL("./public/index.html", import.meta.url), "utf8");
const DEFAULT_SCENARIO_URL = new URL("../examples/escape-the-internet.loom", import.meta.url);
const DEFAULT_SCENARIO = readFileSync(DEFAULT_SCENARIO_URL, "utf8");

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

const modTokens = new Set<string>();
const primeTokens = new Map<string, string>(); // token → character

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
  if (client.role === "guest") return guestView(reqSim(), client.id);
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

function toGuest(id: string, event: string, data: unknown): void {
  for (const c of clients) if (c.role === "guest" && c.id === id) sseSend(c.res, event, data);
}
function toPrime(character: string, event: string, data: unknown): void {
  for (const c of clients) if (c.role === "prime" && c.id === character) sseSend(c.res, event, data);
}
function toAll(event: string, data: unknown): void {
  for (const c of clients) sseSend(c.res, event, data);
}

/** Route the freshly-emitted ledger events to the clients that care. */
function fanout(events: SimEvent[]): void {
  for (const e of events) {
    switch (e.type) {
      case "broadcast":
        for (const id of e.audience) toGuest(id, "notify", { cue: e.cue });
        break;
      case "dialogue":
        for (const id of e.audience) toGuest(id, "line", { speaker: e.speaker, text: e.text });
        break;
      case "respond":
        toPrime(e.to, "response", { text: e.text });
        break;
      case "choicePrompted":
        if (e.person !== null) toGuest(e.person, "choice", { options: e.options });
        break;
      case "factionRevealed":
        toAll("reveal", { faction: e.faction });
        break;
      case "ambient":
        for (const c of clients) if (c.role === "guest") sseSend(c.res, "ambient", { text: e.text });
        break;
      default:
        break;
    }
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
  store.saveSessions({ mod: [...modTokens], prime: [...primeTokens] });
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

function isMod(req: IncomingMessage, body: Record<string, unknown>): boolean {
  const token = (req.headers["x-loom-token"] as string | undefined) ?? str(body, "token");
  return modTokens.has(token);
}
function primeChar(req: IncomingMessage, body: Record<string, unknown>): string | null {
  const token = (req.headers["x-loom-token"] as string | undefined) ?? str(body, "token");
  return primeTokens.get(token) ?? null;
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
        ? guestView(reqSim(), id)
        : role === "prime"
          ? primeView(state.sim, id)
          : modView(state.sim, state.phase, state.scenarioName);
    sendJson(res, 200, view);
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

    // --- performer (prime) actions ---
    case "/api/prime/login": {
      const character = str(body, "character");
      if (!passOk(str(body, "passcode"), PASS.prime)) return void sendJson(res, 403, { error: "bad passcode" });
      if (state.sim !== null && !state.sim.model.characters.has(character)) {
        return void sendJson(res, 404, { error: "unknown character" });
      }
      const token = randomUUID();
      primeTokens.set(token, character);
      persistSessions();
      return void sendJson(res, 200, { token, character });
    }
    case "/api/prime/scan": {
      const character = primeChar(req, body);
      if (character === null) return void sendJson(res, 403, { error: "not logged in" });
      if (!open) return void sendJson(res, 409, { error: "doors are closed" });
      fanout(commit("scan", character, str(body, "target")));
      return void sendJson(res, 200, { ok: true });
    }

    // --- moderator actions ---
    case "/api/mod/login": {
      if (!passOk(str(body, "passcode"), PASS.mod)) return void sendJson(res, 403, { error: "bad passcode" });
      const token = randomUUID();
      modTokens.add(token);
      persistSessions();
      // The mod is the trusted operator — hand back every code so the
      // console can show them: event (for guests), prime (for performers),
      // and mod itself (to recruit a co-moderator). Echoing mod back leaks
      // nothing: the caller just proved they already know it.
      return void sendJson(res, 200, { token, eventPass: PASS.event, primePass: PASS.prime, modPass: PASS.mod });
    }
    default:
      break;
  }

  // everything below requires a mod token
  if (path.startsWith("/api/mod/")) {
    if (!isMod(req, body)) return void sendJson(res, 403, { error: "moderators only" });
    switch (path) {
      case "/api/mod/load": {
        const source = str(body, "source") || DEFAULT_SCENARIO;
        const name = str(body, "name") || "custom";
        stopTicker();
        loadScenario(source, name);
        store.clearJournal(); // a new story starts a fresh timeline
        pendingTickMs = 0;
        persistMeta();
        pushSnapshots();
        return void sendJson(res, 200, { ok: true, phase: state.phase });
      }
      case "/api/mod/start": {
        if (state.sim === null) {
          loadScenario(DEFAULT_SCENARIO, "escape-the-internet");
          store.clearJournal();
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
      case "/api/mod/signal": {
        if (state.sim === null) return void sendJson(res, 409, { error: "no scenario loaded" });
        const subject = str(body, "subject");
        fanout(commit("signal", str(body, "name"), subject === "" ? undefined : subject));
        return void sendJson(res, 200, { ok: true });
      }
      case "/api/mod/broadcast": {
        if (state.sim === null) return void sendJson(res, 409, { error: "no scenario loaded" });
        // Drive a broadcast through a synthetic directive on the world.
        const audience = state.sim.audienceFor(str(body, "scope"));
        const cue = str(body, "cue") || "cue";
        for (const id of audience) toGuest(id, "notify", { cue });
        pushSnapshots();
        return void sendJson(res, 200, { ok: true, reached: audience.length });
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
        (fn as (...a: unknown[]) => unknown).apply(state.sim, e.a);
        events++;
      } catch {
        /* tolerate a single bad/torn entry rather than abort recovery */
      }
    }
  }
  state.phase = meta.phase;
  const s = store.loadSessions();
  for (const t of s.mod) modTokens.add(t);
  for (const [t, c] of s.prime) primeTokens.set(t, c);
  if (state.phase === "open") startTicker();
  return { guests: state.sim?.persons.size ?? 0, events, sessions: s.mod.length + s.prime.length };
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
