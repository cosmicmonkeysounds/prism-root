//! Loom event server — hosts one live `Sim` and serves the role-based
//! web client to clients on the local network.
//!
//! Transport: Server-Sent Events for server→client push (state
//! snapshots, broadcasts, choice prompts) + JSON `fetch` POST for
//! client→server actions. No WebSocket dependency, so it works on any
//! phone browser on the wifi.
//!
//! Run: `pnpm --filter @loom/core serve` (or `npx tsx server/server.ts`).
//! Env: LOOM_PORT, LOOM_HOST, LOOM_MOD_PASS, LOOM_PRIME_PASS.

import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { readFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { networkInterfaces } from "node:os";
import QRCode from "qrcode";

import { Sim, type SimEvent } from "../src/runtime/sim/index.ts";
import { guestView, modView, primeView, type RuntimePhase } from "./views.ts";

const PORT = Number(process.env.LOOM_PORT ?? 7000);
const HOST = process.env.LOOM_HOST ?? "0.0.0.0";
const MOD_PASS = process.env.LOOM_MOD_PASS ?? "mod";
const PRIME_PASS = process.env.LOOM_PRIME_PASS ?? "backstage";

const CLIENT_HTML = readFileSync(new URL("./public/index.html", import.meta.url), "utf8");
const DEFAULT_SCENARIO_URL = new URL("../examples/escape-the-internet.loom", import.meta.url);
const DEFAULT_SCENARIO = readFileSync(DEFAULT_SCENARIO_URL, "utf8");

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
      default:
        break;
    }
  }
  pushSnapshots();
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

  // --- static client ---
  if (method === "GET" && (path === "/" || path === "/index.html")) {
    res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    res.end(CLIENT_HTML);
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
      if (!open) return void sendJson(res, 409, { error: "doors are closed" });
      const name = str(body, "name") || "Guest";
      const id = `g-${randomUUID().slice(0, 6)}`;
      fanout(state.sim!.createPerson(id, name));
      return void sendJson(res, 200, { id, name });
    }
    case "/api/guest/join": {
      if (!open) return void sendJson(res, 409, { error: "doors are closed" });
      fanout(state.sim!.join(str(body, "id"), str(body, "faction")));
      return void sendJson(res, 200, { ok: true });
    }
    case "/api/guest/defect": {
      if (!open) return void sendJson(res, 409, { error: "doors are closed" });
      fanout(state.sim!.defect(str(body, "id"), str(body, "to")));
      return void sendJson(res, 200, { ok: true });
    }
    case "/api/guest/choose": {
      if (!open) return void sendJson(res, 409, { error: "doors are closed" });
      const idx = Number(body["index"] ?? -1);
      fanout(state.sim!.choose(str(body, "id"), idx));
      return void sendJson(res, 200, { ok: true });
    }

    // --- performer (prime) actions ---
    case "/api/prime/login": {
      const character = str(body, "character");
      if (str(body, "passcode") !== PRIME_PASS) return void sendJson(res, 403, { error: "bad passcode" });
      if (state.sim !== null && !state.sim.model.characters.has(character)) {
        return void sendJson(res, 404, { error: "unknown character" });
      }
      const token = randomUUID();
      primeTokens.set(token, character);
      return void sendJson(res, 200, { token, character });
    }
    case "/api/prime/scan": {
      const character = primeChar(req, body);
      if (character === null) return void sendJson(res, 403, { error: "not logged in" });
      if (!open) return void sendJson(res, 409, { error: "doors are closed" });
      fanout(state.sim!.scan(character, str(body, "target")));
      return void sendJson(res, 200, { ok: true });
    }

    // --- moderator actions ---
    case "/api/mod/login": {
      if (str(body, "passcode") !== MOD_PASS) return void sendJson(res, 403, { error: "bad passcode" });
      const token = randomUUID();
      modTokens.add(token);
      return void sendJson(res, 200, { token });
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
        loadScenario(source, name);
        pushSnapshots();
        return void sendJson(res, 200, { ok: true, phase: state.phase });
      }
      case "/api/mod/start": {
        if (state.sim === null) loadScenario(DEFAULT_SCENARIO, "escape-the-internet");
        state.phase = "open";
        pushSnapshots();
        return void sendJson(res, 200, { ok: true, phase: state.phase });
      }
      case "/api/mod/stop": {
        state.phase = "paused";
        pushSnapshots();
        return void sendJson(res, 200, { ok: true, phase: state.phase });
      }
      case "/api/mod/reset": {
        loadScenario(state.scenarioSource, state.scenarioName);
        pushSnapshots();
        return void sendJson(res, 200, { ok: true, phase: state.phase });
      }
      case "/api/mod/signal": {
        if (state.sim === null) return void sendJson(res, 409, { error: "no scenario loaded" });
        const subject = str(body, "subject");
        fanout(state.sim.signal(str(body, "name"), subject === "" ? undefined : subject));
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

server.listen(PORT, HOST, () => {
  const urls = lanUrls(PORT);
  process.stdout.write(`\n  Loom event server — "${state.scenarioName}"\n`);
  process.stdout.write(`  Moderator passcode: ${MOD_PASS}   Performer passcode: ${PRIME_PASS}\n\n`);
  for (const u of urls) process.stdout.write(`  → ${u}\n`);
  process.stdout.write(`\n  Guests join from any phone on this wifi. Mods open the doors to start.\n\n`);
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
