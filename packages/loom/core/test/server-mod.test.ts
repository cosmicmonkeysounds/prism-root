//! Integration tests for the run-panel mod routes on `EventRuntime.handle`.
//! Drives the runtime with fake req/res (like `registry.test.ts`) through the
//! author-session moderator path (`opts.moderator: true`), exercising the new
//! `/api/mod/{say,set,beat,scan}` glue end-to-end against a real scenario.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { IncomingMessage, ServerResponse } from "node:http";

import { afterAll, describe, expect, it } from "vitest";

import { EventRuntime } from "../server/event-runtime.ts";
import { Store } from "../server/store.ts";
import { Sim } from "../src/runtime/sim/index.ts";
import { scenarioSource } from "../examples/load.ts";

const SCENARIO = scenarioSource("escape-the-internet");
const CODES = { event: "EVT111", prime: "PRM111", mod: "MOD111" };

const dirs: string[] = [];
function freshRuntime(): EventRuntime {
  const d = mkdtempSync(join(tmpdir(), "loom-mod-"));
  dirs.push(d);
  return new EventRuntime({
    eventId: "evt",
    store: new Store(d),
    codes: CODES,
    scenarioName: "escape-the-internet",
    scenarioSource: SCENARIO,
    joinBase: () => "http://localhost",
  });
}
afterAll(() => {
  for (const d of dirs) rmSync(d, { recursive: true, force: true });
});

interface FakeRes {
  statusCode: number;
  body: string;
}
function fakeReq(body: unknown): IncomingMessage {
  const buf = Buffer.from(JSON.stringify(body ?? {}));
  return {
    headers: {},
    [Symbol.asyncIterator]: async function* () {
      yield buf;
    },
    on() {
      /* no close event in these tests */
    },
  } as unknown as IncomingMessage;
}
function fakeRes(): ServerResponse & FakeRes {
  const res = {
    statusCode: 0,
    body: "",
    writeHead(s: number) {
      res.statusCode = s;
      return res;
    },
    write() {
      return true;
    },
    end(b?: string) {
      if (typeof b === "string") res.body = b;
      return res;
    },
  };
  return res as unknown as ServerResponse & FakeRes;
}

async function post(rt: EventRuntime, path: string, body: unknown, moderator = true) {
  const res = fakeRes();
  const url = new URL(`http://x${path}`);
  await rt.handle(fakeReq(body), res, "POST", path, url, { moderator });
  return { status: res.statusCode, json: JSON.parse(res.body || "{}") };
}
async function get(rt: EventRuntime, path: string) {
  const res = fakeRes();
  const url = new URL(`http://x${path}`);
  await rt.handle(fakeReq({}), res, "GET", url.pathname, url, {});
  return { status: res.statusCode, json: JSON.parse(res.body || "{}") };
}

/** Open the doors and register a guest; returns their id. */
async function withGuest(rt: EventRuntime): Promise<string> {
  rt.openDoors();
  const r = await post(rt, "/api/guest/register", { name: "Alice", passcode: CODES.event }, false);
  return r.json.id as string;
}

describe("run-panel mod routes", () => {
  it("/api/mod/set edits a guest's score, faction, location, and captured", async () => {
    const rt = freshRuntime();
    const id = await withGuest(rt);

    const score = await post(rt, "/api/mod/set", { id, field: "score", value: 88 });
    expect(score.status).toBe(200);
    expect(score.json.guest.score).toBe(88);

    const faction = await post(rt, "/api/mod/set", { id, field: "faction", value: "Mods" });
    expect(faction.json.guest.faction).toBe("Mods");

    const loc = await post(rt, "/api/mod/set", { id, field: "location", value: "Servers" });
    expect(loc.json.guest.location).toBe("Servers");

    const cap = await post(rt, "/api/mod/set", { id, field: "captured", value: true });
    expect(cap.json.guest.captured).toBe(true);
    const free = await post(rt, "/api/mod/set", { id, field: "captured", value: false });
    expect(free.json.guest.captured).toBe(false);
  });

  it("/api/mod/set rejects unknown fields and unknown guests", async () => {
    const rt = freshRuntime();
    const id = await withGuest(rt);
    expect((await post(rt, "/api/mod/set", { id, field: "bogus", value: 1 })).status).toBe(400);
    expect((await post(rt, "/api/mod/set", { id: "nobody", field: "score", value: 1 })).status).toBe(404);
    expect((await post(rt, "/api/mod/set", { id, field: "faction", value: "" })).status).toBe(400);
  });

  it("/api/mod/say posts an operator message any guest in the room can see", async () => {
    const rt = freshRuntime();
    const id = await withGuest(rt);
    const say = await post(rt, "/api/mod/say", { channel: "lobby", text: "Doors are open, welcome!" });
    expect(say.status).toBe(200);

    const history = await get(rt, `/api/history?id=${id}`);
    const mine = history.json.messages.find((m: { text: string }) => m.text === "Doors are open, welcome!");
    expect(mine).toBeDefined();
    expect(mine.from).toBe("Operator");
  });

  it("/api/mod/say can speak in a character's voice", async () => {
    const rt = freshRuntime();
    const id = await withGuest(rt);
    await post(rt, "/api/mod/say", { channel: "lobby", text: "I am watching.", as: "Moderator_Prime" });
    const history = await get(rt, `/api/history?id=${id}`);
    const mine = history.json.messages.find((m: { text: string }) => m.text === "I am watching.");
    expect(mine.from).toBe("Moderator_Prime");
  });

  it("/api/mod/beat fires a known beat and 404s an unknown one", async () => {
    const rt = freshRuntime();
    await withGuest(rt);
    const probe = Sim.fromSources(SCENARIO);
    const beat = [...probe.model.beats.keys()].find((b) => !b.includes("."));
    expect(beat).toBeDefined();
    expect((await post(rt, "/api/mod/beat", { name: beat })).status).toBe(200);
    expect((await post(rt, "/api/mod/beat", { name: "nope-not-a-beat" })).status).toBe(404);
  });

  it("/api/mod/scan scans a guest as a character", async () => {
    const rt = freshRuntime();
    const id = await withGuest(rt);
    const ok = await post(rt, "/api/mod/scan", { as: "Recruiter", target: id });
    expect(ok.status).toBe(200);
    expect((await post(rt, "/api/mod/scan", { as: "NotACharacter", target: id })).status).toBe(404);
  });

  it("a subject-less fired beat replays deterministically across a restart", async () => {
    // Regression: fireBeat(name, undefined) journals its arg as `null`; the
    // replay guard must treat null as "no subject" instead of throwing, or the
    // beat (and its chat messages, which anchor hidden-flags + threads) vanish.
    const d = mkdtempSync(join(tmpdir(), "loom-mod-"));
    dirs.push(d);
    const mk = () =>
      new EventRuntime({
        eventId: "evt",
        store: new Store(d),
        codes: CODES,
        scenarioName: "escape-the-internet",
        scenarioSource: SCENARIO,
        joinBase: () => "http://localhost",
      });

    const rt1 = mk();
    rt1.openDoors();
    await post(rt1, "/api/guest/register", { name: "Alice", passcode: CODES.event }, false);
    const probe = Sim.fromSources(SCENARIO);
    const beat = [...probe.model.beats.keys()].find((b) => !b.includes("."))!;
    await post(rt1, "/api/mod/beat", { name: beat }); // no subject → journaled as null
    rt1.pause(); // stop the autonomous ticker so the ledger is stable to measure
    const live = await get(rt1, "/api/state?role=mod");
    expect(live.json.ledgerLen).toBeGreaterThan(1);

    // Restart: a fresh runtime replays the same journal from disk.
    const rt2 = mk();
    expect(rt2.restore()).not.toBeNull();
    const replayed = await get(rt2, "/api/state?role=mod");
    expect(replayed.json.ledgerLen).toBe(live.json.ledgerLen);
  });

  it("/api/mod/reveal exposes a hidden faction and 404s an unknown one", async () => {
    const rt = freshRuntime();
    await withGuest(rt);
    expect((await post(rt, "/api/mod/reveal", { faction: "TheAlgorithm" })).status).toBe(200);
    expect((await post(rt, "/api/mod/reveal", { faction: "NotAFaction" })).status).toBe(404);
  });

  it("mod routes are refused without the moderator capability", async () => {
    const rt = freshRuntime();
    const id = await withGuest(rt);
    // No moderator flag and no token → 403.
    const denied = await post(rt, "/api/mod/set", { id, field: "score", value: 1 }, false);
    expect(denied.status).toBe(403);
  });
});
