/**
 * WebSocket transport — bridges Hono WS connections to RelayRouter.
 *
 * Each connection follows the lifecycle:
 *   1. Client authenticates with { type: "auth", did }
 *   2. Router.registerPeer(did, deliverFn) — queued envelopes flush automatically
 *   3. Client sends envelopes → router.route() → route-result reply
 *   4. sync-request / sync-update for CRDT collection sync
 *   5. hashcash-proof for spam protection
 *   6. On close → router.unregisterPeer(did)
 */

import type { WSContext } from "hono/ws";
import type { RelayInstance, RelayRouter, RelayEnvelope, CollectionHost, HashcashGate } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { DID } from "@prism/core/identity";
import {
  parseClientMessage,
  stringifyServerMessage,
  serializeEnvelope,
  deserializeEnvelope,
  encodeBase64,
  decodeBase64,
} from "../protocol/relay-protocol.js";
import type { ServerMessage } from "../protocol/relay-protocol.js";
import type { ConnectionRegistry } from "./connection-registry.js";

export interface WsConnection {
  did: DID | null;
}

function send(ws: WSContext, msg: ServerMessage): void {
  ws.send(stringifyServerMessage(msg));
}

export function handleWsOpen(_ws: WSContext, conn: WsConnection): void {
  conn.did = null;
}

export function handleWsMessage(
  ws: WSContext,
  conn: WsConnection,
  relay: RelayInstance,
  data: string,
  registry?: ConnectionRegistry,
): void {
  let msg;
  try {
    msg = parseClientMessage(data);
  } catch {
    send(ws, { type: "error", message: "malformed message" });
    return;
  }

  const router = relay.getCapability<RelayRouter>(RELAY_CAPABILITIES.ROUTER);

  switch (msg.type) {
    case "auth": {
      if (conn.did) {
        send(ws, { type: "error", message: "already authenticated" });
        return;
      }
      conn.did = msg.did;
      if (router) {
        router.registerPeer(msg.did, (envelope: RelayEnvelope) => {
          send(ws, { type: "envelope", envelope: serializeEnvelope(envelope) });
        });
      }
      send(ws, { type: "auth-ok", relayDid: relay.did, modules: relay.modules });
      return;
    }

    case "envelope": {
      if (!conn.did) {
        send(ws, { type: "error", message: "not authenticated" });
        return;
      }
      if (!router) {
        send(ws, { type: "error", message: "router module not installed" });
        return;
      }
      const envelope = deserializeEnvelope(msg.envelope);
      const result = router.route(envelope);
      send(ws, { type: "route-result", result });
      return;
    }

    case "collect": {
      // Collect is implicit — mailbox envelopes flush on registerPeer.
      // This message is a no-op acknowledgement for future extension.
      return;
    }

    case "ping": {
      send(ws, { type: "pong" });
      return;
    }

    case "sync-request": {
      if (!conn.did) {
        send(ws, { type: "error", message: "not authenticated" });
        return;
      }
      const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
      if (!host) {
        send(ws, { type: "error", message: "collections module not installed" });
        return;
      }
      const store = host.get(msg.collectionId);
      if (!store) {
        send(ws, { type: "error", message: `collection not found: ${msg.collectionId}` });
        return;
      }
      // Subscribe this connection to the collection
      if (registry) {
        const tracked = registry.get(ws);
        if (tracked) {
          tracked.subscribedCollections.add(msg.collectionId);
        }
      }
      const snapshot = store.exportSnapshot();
      send(ws, {
        type: "sync-snapshot",
        collectionId: msg.collectionId,
        snapshot: encodeBase64(snapshot),
      });
      return;
    }

    case "sync-update": {
      if (!conn.did) {
        send(ws, { type: "error", message: "not authenticated" });
        return;
      }
      const host2 = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
      if (!host2) {
        send(ws, { type: "error", message: "collections module not installed" });
        return;
      }
      const store2 = host2.get(msg.collectionId);
      if (!store2) {
        send(ws, { type: "error", message: `collection not found: ${msg.collectionId}` });
        return;
      }
      const updateBytes = decodeBase64(msg.update);
      store2.import(updateBytes);
      // Broadcast to all other subscribers
      if (registry) {
        registry.broadcastToCollection(
          msg.collectionId,
          { type: "sync-update", collectionId: msg.collectionId, update: msg.update },
          ws,
        );
      }
      return;
    }

    case "hashcash-proof": {
      if (!conn.did) {
        send(ws, { type: "error", message: "not authenticated" });
        return;
      }
      const gate = relay.getCapability<HashcashGate>(RELAY_CAPABILITIES.HASHCASH);
      if (!gate) {
        send(ws, { type: "error", message: "hashcash module not installed" });
        return;
      }
      gate.verifyProof(msg.proof).then((valid) => {
        if (valid && conn.did) {
          gate.markVerified(conn.did);
          send(ws, { type: "hashcash-ok" });
        } else {
          send(ws, { type: "error", message: "invalid proof-of-work" });
        }
      }).catch(() => {
        send(ws, { type: "error", message: "proof verification failed" });
      });
      return;
    }
  }
}

export function handleWsClose(conn: WsConnection, relay: RelayInstance): void {
  if (conn.did) {
    const router = relay.getCapability<RelayRouter>(RELAY_CAPABILITIES.ROUTER);
    router?.unregisterPeer(conn.did);
    conn.did = null;
  }
}
