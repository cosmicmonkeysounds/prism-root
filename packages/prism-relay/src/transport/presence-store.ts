/**
 * PresenceStore — RAM-only ephemeral presence state for connected peers.
 *
 * Tracks cursor position, selection range, and active view per peer.
 * Never persisted — purely in-memory for real-time collaboration overlays.
 */

import type { WSContext } from "hono/ws";
import type { ServerMessage } from "../protocol/relay-protocol.js";
import type { ConnectionRegistry } from "./connection-registry.js";

export interface PeerPresence {
  cursor?: { x: number; y: number } | undefined;
  selection?: { start: number; end: number } | undefined;
  activeView?: string | undefined;
}

export interface PresenceStore {
  set(peerId: string, state: PeerPresence): void;
  remove(peerId: string): void;
  get(peerId: string): PeerPresence | undefined;
  getAll(): Array<{ peerId: string } & PeerPresence>;
  /** Broadcast a presence message to all connected peers, optionally excluding a sender. */
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
      registry.broadcastAll(msg, exclude);
    },
  };
}
