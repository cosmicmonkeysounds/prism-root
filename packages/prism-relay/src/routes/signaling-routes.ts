/**
 * WebRTC Signaling Routes — P2P/SFU connection negotiation for all relays.
 *
 * Provides HTTP-based signaling for WebRTC connection setup:
 *   POST /rooms/:roomId/join    — join a signaling room
 *   POST /rooms/:roomId/leave   — leave a room
 *   POST /rooms/:roomId/signal  — relay SDP offer/answer/ICE candidate
 *   GET  /rooms/:roomId/peers   — list peers in a room
 *   GET  /rooms                 — list active rooms
 *
 * The relay is transport-agnostic: it routes opaque signaling payloads
 * between peers. Actual WebRTC connections are established peer-to-peer.
 */

import { Hono } from "hono";
import type { RelayInstance, SignalingHub, SignalingPeer, SignalType } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";

// Buffered signals for peers (polled via /signal response or future WS)
const peerBuffers = new Map<string, Array<Record<string, unknown>>>();

function bufferKey(roomId: string, peerId: string): string {
  return `${roomId}:${peerId}`;
}

export function createSignalingRoutes(relay: RelayInstance): Hono {
  const app = new Hono();
  const hub = relay.getCapability<SignalingHub>(RELAY_CAPABILITIES.SIGNALING);

  // GET /rooms — list active rooms
  app.get("/rooms", (c) => {
    if (!hub) return c.json({ error: "signaling module not installed" }, 404);
    const rooms = hub.listRooms();
    return c.json({ rooms, count: rooms.length });
  });

  // GET /rooms/:roomId/peers — list peers in a room
  app.get("/rooms/:roomId/peers", (c) => {
    if (!hub) return c.json({ error: "signaling module not installed" }, 404);
    const roomId = c.req.param("roomId");
    const peers = hub.getPeers(roomId);
    return c.json({ peers, count: peers.length });
  });

  // POST /rooms/:roomId/join — join a signaling room
  app.post("/rooms/:roomId/join", async (c) => {
    if (!hub) return c.json({ error: "signaling module not installed" }, 404);

    const roomId = c.req.param("roomId");
    const body = await c.req.json<{
      peerId: string;
      displayName?: string;
      metadata?: Record<string, unknown>;
    }>();

    if (!body.peerId) {
      return c.json({ error: "peerId is required" }, 400);
    }

    const peer: SignalingPeer = {
      peerId: body.peerId,
      ...(body.displayName !== undefined && { displayName: body.displayName }),
      joinedAt: new Date().toISOString(),
      ...(body.metadata !== undefined && { metadata: body.metadata }),
    };

    // Set up a buffer for delivering signals to this peer
    const key = bufferKey(roomId, body.peerId);
    peerBuffers.set(key, []);

    try {
      const existingPeers = hub.join(roomId, peer, (msg) => {
        // Buffer signals for HTTP polling
        const buf = peerBuffers.get(key);
        if (buf) buf.push(msg as unknown as Record<string, unknown>);
      });

      return c.json({
        ok: true,
        roomId,
        peerId: body.peerId,
        peers: existingPeers,
      });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return c.json({ error: message }, 409);
    }
  });

  // POST /rooms/:roomId/leave — leave a room
  app.post("/rooms/:roomId/leave", async (c) => {
    if (!hub) return c.json({ error: "signaling module not installed" }, 404);

    const roomId = c.req.param("roomId");
    const body = await c.req.json<{ peerId: string }>();

    if (!body.peerId) {
      return c.json({ error: "peerId is required" }, 400);
    }

    hub.leave(roomId, body.peerId);
    peerBuffers.delete(bufferKey(roomId, body.peerId));

    return c.json({ ok: true });
  });

  // POST /rooms/:roomId/signal — relay a signaling message
  app.post("/rooms/:roomId/signal", async (c) => {
    if (!hub) return c.json({ error: "signaling module not installed" }, 404);

    const roomId = c.req.param("roomId");
    const body = await c.req.json<{
      type: string;
      from: string;
      to: string;
      payload: unknown;
    }>();

    if (!body.type || !body.from || !body.to) {
      return c.json({ error: "type, from, and to are required" }, 400);
    }

    const validTypes: SignalType[] = ["offer", "answer", "ice-candidate", "leave"];
    if (!validTypes.includes(body.type as SignalType)) {
      return c.json({ error: `invalid signal type: ${body.type}` }, 400);
    }

    const delivered = hub.relay({
      type: body.type as SignalType,
      from: body.from,
      to: body.to,
      roomId,
      payload: body.payload,
    });

    return c.json({ ok: true, delivered });
  });

  // POST /rooms/:roomId/poll — poll buffered signals for a peer
  app.post("/rooms/:roomId/poll", async (c) => {
    if (!hub) return c.json({ error: "signaling module not installed" }, 404);

    const roomId = c.req.param("roomId");
    const body = await c.req.json<{ peerId: string }>();

    if (!body.peerId) {
      return c.json({ error: "peerId is required" }, 400);
    }

    const key = bufferKey(roomId, body.peerId);
    const signals = peerBuffers.get(key) ?? [];
    // Drain the buffer
    peerBuffers.set(key, []);

    return c.json({ signals, count: signals.length });
  });

  return app;
}
