import { describe, it, expect, beforeAll, beforeEach } from "vitest";
import type { DID } from "@prism/core/identity";
import { createIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
} from "@prism/core/relay";
import type { RelayInstance, RelayRouter } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { serializeEnvelope } from "../protocol/relay-protocol.js";
import type { ServerMessage } from "../protocol/relay-protocol.js";
import { handleWsOpen, handleWsMessage, handleWsClose } from "./ws-transport.js";
import type { WsConnection } from "./ws-transport.js";

const ALICE = "did:key:zAlice" as DID;
const BOB = "did:key:zBob" as DID;

let relay: RelayInstance;

beforeAll(async () => {
  const identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .build();
  await relay.start();
});

function createMockWs(): { ws: MockWs; sent: ServerMessage[] } {
  const sent: ServerMessage[] = [];
  const ws = {
    send(data: string) {
      sent.push(JSON.parse(data) as ServerMessage);
    },
  };
  return { ws: ws as MockWs, sent };
}

type MockWs = { send(data: string): void };

function msg(sent: ServerMessage[], idx: number): ServerMessage {
  const m = sent[idx];
  if (!m) throw new Error(`No message at index ${idx}`);
  return m;
}

describe("ws-transport", () => {
  let conn: WsConnection;

  beforeEach(() => {
    const router = relay.getCapability<RelayRouter>(RELAY_CAPABILITIES.ROUTER);
    for (const did of router?.onlinePeers() ?? []) {
      router?.unregisterPeer(did);
    }
    conn = { did: null };
  });

  it("authenticates and registers peer", () => {
    const { ws, sent } = createMockWs();
    handleWsOpen(ws as never, conn);
    expect(conn.did).toBeNull();

    handleWsMessage(ws as never, conn, relay, JSON.stringify({ type: "auth", did: ALICE }));
    expect(conn.did).toBe(ALICE);
    expect(sent).toHaveLength(1);
    expect(msg(sent, 0).type).toBe("auth-ok");

    const router = relay.getCapability<RelayRouter>(RELAY_CAPABILITIES.ROUTER);
    expect(router?.isOnline(ALICE)).toBe(true);
  });

  it("rejects duplicate auth", () => {
    const { ws, sent } = createMockWs();
    handleWsOpen(ws as never, conn);
    handleWsMessage(ws as never, conn, relay, JSON.stringify({ type: "auth", did: ALICE }));
    handleWsMessage(ws as never, conn, relay, JSON.stringify({ type: "auth", did: BOB }));
    expect(msg(sent, 1).type).toBe("error");
  });

  it("rejects envelope before auth", () => {
    const { ws, sent } = createMockWs();
    handleWsOpen(ws as never, conn);
    const envelope = serializeEnvelope({
      id: "e1",
      from: ALICE,
      to: BOB,
      ciphertext: new Uint8Array([1, 2, 3]),
      submittedAt: new Date().toISOString(),
      ttlMs: 60_000,
    });
    handleWsMessage(ws as never, conn, relay, JSON.stringify({ type: "envelope", envelope }));
    const first = msg(sent, 0);
    expect(first.type).toBe("error");
    if (first.type === "error") {
      expect(first.message).toContain("not authenticated");
    }
  });

  it("routes envelope between peers", () => {
    const alice = { did: null as DID | null };
    const { ws: aliceWs, sent: aliceSent } = createMockWs();
    handleWsOpen(aliceWs as never, alice);
    handleWsMessage(aliceWs as never, alice, relay, JSON.stringify({ type: "auth", did: ALICE }));

    const bob = { did: null as DID | null };
    const { ws: bobWs, sent: bobSent } = createMockWs();
    handleWsOpen(bobWs as never, bob);
    handleWsMessage(bobWs as never, bob, relay, JSON.stringify({ type: "auth", did: BOB }));

    const envelope = serializeEnvelope({
      id: "e2",
      from: ALICE,
      to: BOB,
      ciphertext: new Uint8Array([10, 20]),
      submittedAt: new Date().toISOString(),
      ttlMs: 60_000,
    });
    handleWsMessage(aliceWs as never, alice, relay, JSON.stringify({ type: "envelope", envelope }));

    const routeResult = aliceSent.find((m) => m.type === "route-result");
    expect(routeResult).toBeDefined();
    if (routeResult?.type === "route-result") {
      expect(routeResult.result.status).toBe("delivered");
    }

    const delivered = bobSent.find((m) => m.type === "envelope");
    expect(delivered).toBeDefined();

    handleWsClose(alice, relay);
    handleWsClose(bob, relay);
  });

  it("queues envelope for offline peer", () => {
    const { ws: aliceWs, sent: aliceSent } = createMockWs();
    handleWsOpen(aliceWs as never, conn);
    handleWsMessage(aliceWs as never, conn, relay, JSON.stringify({ type: "auth", did: ALICE }));

    const envelope = serializeEnvelope({
      id: "e3",
      from: ALICE,
      to: BOB,
      ciphertext: new Uint8Array([5]),
      submittedAt: new Date().toISOString(),
      ttlMs: 60_000,
    });
    handleWsMessage(aliceWs as never, conn, relay, JSON.stringify({ type: "envelope", envelope }));

    const routeResult = aliceSent.find((m) => m.type === "route-result");
    if (routeResult?.type === "route-result") {
      expect(routeResult.result.status).toBe("queued");
    }

    handleWsClose(conn, relay);
  });

  it("responds to ping with pong", () => {
    const { ws, sent } = createMockWs();
    handleWsOpen(ws as never, conn);
    handleWsMessage(ws as never, conn, relay, JSON.stringify({ type: "ping" }));
    expect(msg(sent, 0).type).toBe("pong");
  });

  it("handles malformed message", () => {
    const { ws, sent } = createMockWs();
    handleWsOpen(ws as never, conn);
    handleWsMessage(ws as never, conn, relay, "not json");
    expect(msg(sent, 0).type).toBe("error");
  });

  it("unregisters peer on close", () => {
    const { ws } = createMockWs();
    handleWsOpen(ws as never, conn);
    handleWsMessage(ws as never, conn, relay, JSON.stringify({ type: "auth", did: ALICE }));

    const router = relay.getCapability<RelayRouter>(RELAY_CAPABILITIES.ROUTER);
    expect(router?.isOnline(ALICE)).toBe(true);

    handleWsClose(conn, relay);
    expect(router?.isOnline(ALICE)).toBe(false);
  });
});
