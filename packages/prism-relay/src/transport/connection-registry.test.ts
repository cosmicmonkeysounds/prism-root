import { describe, it, expect } from "vitest";
import type { ServerMessage } from "../protocol/relay-protocol.js";
import { createConnectionRegistry } from "./connection-registry.js";

function createMockWs(): { ws: unknown; sent: ServerMessage[] } {
  const sent: ServerMessage[] = [];
  const ws = {
    send(data: string) {
      sent.push(JSON.parse(data) as ServerMessage);
    },
  };
  return { ws, sent };
}

describe("connection-registry", () => {
  it("tracks connections", () => {
    const registry = createConnectionRegistry();
    const { ws } = createMockWs();
    const tracked = registry.add(ws as never);
    expect(tracked.subscribedCollections.size).toBe(0);
    expect(registry.get(ws as never)).toBe(tracked);
  });

  it("removes connections", () => {
    const registry = createConnectionRegistry();
    const { ws } = createMockWs();
    registry.add(ws as never);
    registry.remove(ws as never);
    expect(registry.get(ws as never)).toBeUndefined();
  });

  it("broadcasts to subscribed connections", () => {
    const registry = createConnectionRegistry();
    const { ws: ws1, sent: sent1 } = createMockWs();
    const { ws: ws2, sent: sent2 } = createMockWs();
    const { ws: ws3, sent: sent3 } = createMockWs();

    const t1 = registry.add(ws1 as never);
    const t2 = registry.add(ws2 as never);
    registry.add(ws3 as never);

    t1.subscribedCollections.add("col-a");
    t2.subscribedCollections.add("col-a");
    // ws3 not subscribed to col-a

    const msg: ServerMessage = { type: "sync-update", collectionId: "col-a", update: "AQID" };
    registry.broadcastToCollection("col-a", msg);

    expect(sent1).toHaveLength(1);
    expect(sent2).toHaveLength(1);
    expect(sent3).toHaveLength(0);
  });

  it("excludes sender from broadcast", () => {
    const registry = createConnectionRegistry();
    const { ws: ws1, sent: sent1 } = createMockWs();
    const { ws: ws2, sent: sent2 } = createMockWs();

    const t1 = registry.add(ws1 as never);
    const t2 = registry.add(ws2 as never);
    t1.subscribedCollections.add("col-b");
    t2.subscribedCollections.add("col-b");

    const msg: ServerMessage = { type: "sync-update", collectionId: "col-b", update: "AQID" };
    registry.broadcastToCollection("col-b", msg, ws1 as never);

    expect(sent1).toHaveLength(0);
    expect(sent2).toHaveLength(1);
  });
});
