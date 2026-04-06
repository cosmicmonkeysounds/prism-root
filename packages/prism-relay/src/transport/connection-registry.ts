/**
 * ConnectionRegistry — tracks active WS connections and their collection subscriptions.
 *
 * Used for broadcasting sync-update messages to all peers subscribed to a collection.
 */

import type { WSContext } from "hono/ws";
import type { ServerMessage } from "../protocol/relay-protocol.js";
import { stringifyServerMessage } from "../protocol/relay-protocol.js";

export interface TrackedConnection {
  ws: WSContext;
  subscribedCollections: Set<string>;
}

export interface ConnectionRegistry {
  add(ws: WSContext): TrackedConnection;
  remove(ws: WSContext): void;
  get(ws: WSContext): TrackedConnection | undefined;
  broadcastToCollection(collectionId: string, msg: ServerMessage, exclude?: WSContext): void;
}

export function createConnectionRegistry(): ConnectionRegistry {
  const connections = new Map<WSContext, TrackedConnection>();

  return {
    add(ws) {
      const tracked: TrackedConnection = {
        ws,
        subscribedCollections: new Set(),
      };
      connections.set(ws, tracked);
      return tracked;
    },

    remove(ws) {
      connections.delete(ws);
    },

    get(ws) {
      return connections.get(ws);
    },

    broadcastToCollection(collectionId, msg, exclude) {
      const payload = stringifyServerMessage(msg);
      for (const [, conn] of connections) {
        if (conn.subscribedCollections.has(collectionId) && conn.ws !== exclude) {
          conn.ws.send(payload);
        }
      }
    },
  };
}
