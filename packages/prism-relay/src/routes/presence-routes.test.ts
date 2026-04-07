import { describe, it, expect } from "vitest";
import { createPresenceStore } from "../transport/presence-store.js";
import { createPresenceRoutes } from "./presence-routes.js";

describe("presence-routes", () => {
  it("returns empty peers list initially", async () => {
    const store = createPresenceStore();
    const app = createPresenceRoutes(store);

    const res = await app.request("/");
    expect(res.status).toBe(200);
    const body = (await res.json()) as { peers: unknown[]; count: number };
    expect(body.peers).toHaveLength(0);
    expect(body.count).toBe(0);
  });

  it("returns peers after setting presence state", async () => {
    const store = createPresenceStore();
    const app = createPresenceRoutes(store);

    store.set("peer-1", { cursor: { x: 10, y: 20 }, activeView: "editor" });

    const res = await app.request("/");
    expect(res.status).toBe(200);
    const body = (await res.json()) as { peers: Array<Record<string, unknown>>; count: number };
    expect(body.peers).toHaveLength(1);
    expect(body.count).toBe(1);
    expect(body.peers[0]?.["peerId"]).toBe("peer-1");
  });
});
