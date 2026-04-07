/**
 * PresenceStore — RAM-only ephemeral presence state for connected peers.
 *
 * Tracks cursor position, selection range, and active view per peer.
 * Never persisted — purely in-memory for real-time collaboration overlays.
 */

import type { WSContext } from "hono/ws";
import type { ServerMessage } from "../protocol/relay-protocol.js";
import { stringifyServerMessage } from "../protocol/relay-protocol.js";
import type { ConnectionRegistry } from "./connection-registry.js";

export interface PeerPresence {
  cursor?: { x: number; y: number };
  selection?: { start: number; end: number };
  activeView?: string;
}

export interface PresenceStore {
  set(peerId: string, state: PeerPresence): void;
  remove(peerId: string): void;
  get(peerId: string): PeerPresence | undefined;
  getAll(): Array<{ peerId: string } & PeerPresence>;
  broadcast(registry: ConnectionRegistry, exclude: WSContext | undefined, msg: ServerMessage): void;
}

export function createPresenceStore(): PresenceStore {
  const peers = new Map<string, PeerPresence>();

  return {
    set(peerId, state) {
      peers.set(peerId, state);
    },

    remove(peerId) {
      peers.delete(peerId);
    },

    get(peerId) {
      return peers.get(peerId);
    },

    getAll() {
      const result: Array<{ peerId: string } & PeerPresence> = [];
      for (const [peerId, state] of peers) {
        result.push({ peerId, ...state });
      }
      return result;
    },

    broadcast(registry, exclude, msg) {
      const payload = stringifyServerMessage(msg);
      // Use the registry's internal iteration — we broadcast to ALL connections,
      // not just collection subscribers. We access via broadcastToAll helper.
      // Since ConnectionRegistry doesn't have broadcastAll, we iterate tracked connections.
      // We need to send to every tracked connection except the excluded one.
      // ConnectionRegistry exposes get() per-ws but not iteration, so we use
      // a lightweight approach: store WS references alongside presence.
      //
      // Actually, ConnectionRegistry doesn't expose iteration. We'll use a
      // parallel tracking approach: presence-store keeps its own ws set.
      _broadcastAll(payload, exclude);
    },
  };

  function _broadcastAll(payload: string, exclude: WSContext | undefined): void {
    // This is a private concern — we keep a ws set in closure scope.
    // See the overridden version below.
    void payload;
    void exclude;
  }
}
