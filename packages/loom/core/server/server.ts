//! Loom event server — the multi-tenant backbone that hosts live `Sim`
//! events and serves the role-based web client.
//!
//! Transport: Server-Sent Events for server→client push (state
//! snapshots, broadcasts, choice prompts) + JSON `fetch` POST for
//! client→server actions. No WebSocket dependency, so it works on any
//! phone browser on the wifi.
//!
//! Each live event is an `EventRuntime` (see `event-runtime.ts`). This file
//! owns the process: static app serving, the QR endpoint, restart-recovery,
//! and routing each request to the right runtime. During this first slice a
//! single default event is hosted at the root paths (identical to the old
//! single-event server); the `EventRegistry` + `/e/:eventId` namespacing land
//! next.
//!
//! Run: `pnpm --filter @loom/core serve` (or `npx tsx server/server.ts`).
//! Env: LOOM_PORT, LOOM_HOST, LOOM_EVENT_PASS, LOOM_MOD_PASS,
//! LOOM_PRIME_PASS (any passcode left unset is auto-generated and printed
//! at boot), LOOM_STATE_DIR (where restart-recovery state is kept),
//! LOOM_APP_DIST.

import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { readFileSync } from "node:fs";
import { networkInterfaces } from "node:os";
import { fileURLToPath, pathToFileURL } from "node:url";
import QRCode from "qrcode";

import { toNodeHandler } from "better-auth/node";

import { resolvePasscodes } from "./auth.ts";
import { Store } from "./store.ts";
import { scenarioSource } from "../examples/load.ts";
import { EventRuntime } from "./event-runtime.ts";
import { EventRegistry } from "./registry.ts";
import { readBody, sendJson, str } from "./http-util.ts";
import { auth, authUser, migrateAuth } from "./auth-server.ts";
import { dbReady, initSchema } from "./db/index.ts";
import { eventOwnerId, liveEvents } from "./db/queries.ts";
import { DATABASE_URL } from "./config.ts";
import { handleProjects } from "./projects.ts";
import { handleEvent, specFromRow } from "./events-api.ts";

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

// The multi-tenant registry: one `EventRuntime` per live event, keyed by id.
const registry = new EventRegistry(STATE_DIR, joinBase);

// The default event journals to the state-dir root (not a sub-directory) so
// an existing single-event deployment recovers its `.loom-state/` in place.
// It's reachable both at the root paths (back-compat: the operator console +
// the current participant app) and at `/e/default`.
const defaultEvent = registry.register(
  new EventRuntime({
    eventId: "default",
    store,
    codes: PASS,
    scenarioName: "escape-the-internet",
    scenarioSource: DEFAULT_SCENARIO,
    joinBase,
  }),
);

// ---------------------------------------------------------------------
// Control plane (author accounts + projects + events) — needs a database.
// The event plane above runs without one; if the DB is unreachable the
// control plane stays disabled and its routes answer 503, so a LAN-only
// deployment is unaffected.
// ---------------------------------------------------------------------

const authHandler = toNodeHandler(auth);
let controlPlane = false;

async function initControlPlane(): Promise<void> {
  if (!(await dbReady())) {
    process.stderr.write(`  ⚠️  control plane disabled — database unreachable at ${DATABASE_URL}\n`);
    return;
  }
  await migrateAuth();
  await initSchema();
  controlPlane = true;
  // Rehydrate every event that was live before this process started: replay
  // its journal and (for open ones) restart its clock — the multi-event
  // generalization of the default event's `restore()`.
  let rehydrated = 0;
  for (const row of await liveEvents()) {
    registry.ensure(specFromRow(row));
    rehydrated++;
  }
  if (rehydrated > 0) process.stdout.write(`  ↻ Rehydrated ${rehydrated} live event(s) from the database\n`);
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

  // --- author auth plane (BetterAuth owns everything under /api/auth) ---
  if (path.startsWith("/api/auth")) {
    if (!controlPlane) return void sendJson(res, 503, { error: "authoring is offline (no database)" });
    await authHandler(req, res);
    return;
  }

  // --- author control plane: projects + files + events (authors only) ---
  if (path === "/api/projects" || path.startsWith("/api/projects/")) {
    if (!controlPlane) return void sendJson(res, 503, { error: "authoring is offline (no database)" });
    const user = await authUser(req);
    if (user === null) return void sendJson(res, 401, { error: "sign in" });
    const segs = path.split("/").filter(Boolean); // ["api","projects",id,"event",action?]
    if (segs.length >= 4 && segs[3] === "event") {
      if (await handleEvent(req, res, method, segs, user, { registry, joinBase })) return;
      return void sendJson(res, 404, { error: "not found" });
    }
    if (await handleProjects(req, res, method, path, user)) return;
    return void sendJson(res, 404, { error: "not found" });
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

  // --- public bootstrap: a short passcode → which event + role it grants ---
  // A guest/performer/moderator knows only their code; this tells the client
  // which `/e/:eventId` to talk to before it registers or signs in.
  if (method === "POST" && path === "/api/resolve-code") {
    const body = await readBody(req);
    const hit = registry.resolveCode(str(body, "code"));
    if (hit === null) return void sendJson(res, 404, { error: "no event with that code" });
    return void sendJson(res, 200, hit);
  }

  // --- event-scoped routes under /e/:eventId ---
  if (path.startsWith("/e/")) {
    const rest = path.slice(3);
    const slash = rest.indexOf("/");
    const eventId = slash === -1 ? rest : rest.slice(0, slash);
    const subPath = slash === -1 ? "/" : rest.slice(slash);
    const runtime = registry.get(eventId);
    if (runtime === undefined) return void sendJson(res, 404, { error: "unknown event" });
    // The owning author moderates via their session — no mod code needed.
    let moderator = false;
    if (controlPlane && subPath.startsWith("/api/mod/")) {
      const user = await authUser(req);
      if (user !== null) moderator = (await eventOwnerId(eventId)) === user.id;
    }
    if (await runtime.handle(req, res, method, subPath, url, { moderator })) return;
    return void sendJson(res, 404, { error: "not found" });
  }

  // --- back-compat: bare event routes target the default event ---
  if (await defaultEvent.handle(req, res, method, path, url)) return;

  sendJson(res, 404, { error: "not found" });
}

// ---------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------

const restored = defaultEvent.restore();
void initControlPlane();

server.listen(PORT, HOST, () => {
  const urls = lanUrls(PORT);
  let appBuilt = true;
  try {
    readFileSync(new URL("index.html", APP_DIST));
  } catch {
    appBuilt = false;
  }
  process.stdout.write(`\n  Loom event server — "${defaultEvent.scenario}"  (phase: ${defaultEvent.currentPhase})\n\n`);
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
