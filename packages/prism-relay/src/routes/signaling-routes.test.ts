/**
 * Signaling routes — integration tests for WebRTC signaling relay.
 */

import { describe, it, expect, beforeAll } from "vitest";
import { Hono } from "hono";
import { createSignalingRoutes } from "./signaling-routes.js";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  webrtcSignalingModule,
  vaultHostModule,
} from "@prism/core/relay";
import { createIdentity } from "@prism/core/identity";

describe("signaling routes", () => {
  let app: Hono;

  beforeAll(async () => {
    const identity = await createIdentity();
    const relay = createRelayBuilder({ relayDid: identity.did })
      .use(blindMailboxModule())
      .use(relayRouterModule())
      .use(webrtcSignalingModule())
      .use(vaultHostModule())
      .build();

    app = new Hono();
    app.route("/api/signaling", createSignalingRoutes(relay));
  });

  function get(path: string) {
    return app.request(`http://localhost/api/signaling${path}`);
  }

  function post(path: string, body: Record<string, unknown>) {
    return app.request(`http://localhost/api/signaling${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
  }

  it("lists rooms (empty initially)", async () => {
    const res = await get("/rooms");
    expect(res.status).toBe(200);
    const data = await res.json() as Record<string, unknown>;
    expect(data.rooms).toEqual([]);
    expect(data.count).toBe(0);
  });

  it("joins a room and returns empty peer list for first peer", async () => {
    const res = await post("/rooms/test-room/join", {
      peerId: "peer-a",
      displayName: "Alice",
    });
    expect(res.status).toBe(200);
    const data = await res.json() as Record<string, unknown>;
    expect(data.ok).toBe(true);
    expect(data.roomId).toBe("test-room");
    expect(data.peers).toEqual([]);
  });

  it("second peer sees first peer on join", async () => {
    const res = await post("/rooms/test-room/join", {
      peerId: "peer-b",
      displayName: "Bob",
    });
    expect(res.status).toBe(200);
    const data = await res.json() as Record<string, unknown>;
    expect(data.ok).toBe(true);
    const peers = data.peers as Array<Record<string, unknown>>;
    expect(peers).toHaveLength(1);
    expect(peers[0]?.peerId).toBe("peer-a");
  });

  it("lists rooms with peer count", async () => {
    const res = await get("/rooms");
    expect(res.status).toBe(200);
    const data = await res.json() as Record<string, unknown>;
    const rooms = data.rooms as Array<Record<string, unknown>>;
    expect(rooms).toHaveLength(1);
    expect(rooms[0]?.peerCount).toBe(2);
  });

  it("lists peers in a room", async () => {
    const res = await get("/rooms/test-room/peers");
    expect(res.status).toBe(200);
    const data = await res.json() as Record<string, unknown>;
    expect(data.count).toBe(2);
    const peers = data.peers as Array<Record<string, unknown>>;
    expect(peers.map((p) => p.peerId)).toEqual(
      expect.arrayContaining(["peer-a", "peer-b"]),
    );
  });

  it("relays an SDP offer between peers", async () => {
    const res = await post("/rooms/test-room/signal", {
      type: "offer",
      from: "peer-a",
      to: "peer-b",
      payload: { sdp: "v=0\r\n..." },
    });
    expect(res.status).toBe(200);
    const data = await res.json() as Record<string, unknown>;
    expect(data.ok).toBe(true);
    expect(data.delivered).toBe(true);
  });

  it("peer-b can poll buffered signals", async () => {
    const res = await post("/rooms/test-room/poll", {
      peerId: "peer-b",
    });
    expect(res.status).toBe(200);
    const data = await res.json() as Record<string, unknown>;
    expect(data.count).toBe(1);
    const signals = data.signals as Array<Record<string, unknown>>;
    expect(signals[0]?.type).toBe("offer");
    expect(signals[0]?.from).toBe("peer-a");
  });

  it("poll drains the buffer", async () => {
    const res = await post("/rooms/test-room/poll", {
      peerId: "peer-b",
    });
    const data = await res.json() as Record<string, unknown>;
    expect(data.count).toBe(0);
  });

  it("relays an SDP answer back", async () => {
    const res = await post("/rooms/test-room/signal", {
      type: "answer",
      from: "peer-b",
      to: "peer-a",
      payload: { sdp: "v=0\r\nanswer..." },
    });
    expect(res.status).toBe(200);
    const data = await res.json() as Record<string, unknown>;
    expect(data.delivered).toBe(true);
  });

  it("relays ICE candidates", async () => {
    const res = await post("/rooms/test-room/signal", {
      type: "ice-candidate",
      from: "peer-a",
      to: "peer-b",
      payload: { candidate: "candidate:1 1 udp 2130706431 ..." },
    });
    expect(res.status).toBe(200);
    const data = await res.json() as Record<string, unknown>;
    expect(data.delivered).toBe(true);
  });

  it("rejects invalid signal type", async () => {
    const res = await post("/rooms/test-room/signal", {
      type: "invalid",
      from: "peer-a",
      to: "peer-b",
      payload: {},
    });
    expect(res.status).toBe(400);
  });

  it("returns delivered=false for unknown peer", async () => {
    const res = await post("/rooms/test-room/signal", {
      type: "offer",
      from: "peer-a",
      to: "peer-unknown",
      payload: {},
    });
    expect(res.status).toBe(200);
    const data = await res.json() as Record<string, unknown>;
    expect(data.delivered).toBe(false);
  });

  it("peer leaves a room", async () => {
    const res = await post("/rooms/test-room/leave", {
      peerId: "peer-a",
    });
    expect(res.status).toBe(200);
    const data = await res.json() as Record<string, unknown>;
    expect(data.ok).toBe(true);

    // Verify peer count decreased
    const peersRes = await get("/rooms/test-room/peers");
    const peersData = await peersRes.json() as Record<string, unknown>;
    expect(peersData.count).toBe(1);
  });

  it("peer-b receives leave notification via poll", async () => {
    const res = await post("/rooms/test-room/poll", {
      peerId: "peer-b",
    });
    const data = await res.json() as Record<string, unknown>;
    // Should have leave notification + any buffered ICE/answer signals
    const signals = data.signals as Array<Record<string, unknown>>;
    const leaveSignals = signals.filter((s) => s.type === "leave");
    expect(leaveSignals.length).toBeGreaterThanOrEqual(1);
    expect(leaveSignals[0]?.from).toBe("peer-a");
  });

  it("rejects join when peerId is missing", async () => {
    const res = await post("/rooms/new-room/join", {});
    expect(res.status).toBe(400);
  });
});
