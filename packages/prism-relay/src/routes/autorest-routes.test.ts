import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  collectionHostModule,
  webhookModule,
} from "@prism/core/relay";
import type { RelayInstance, CollectionHost } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { objectId } from "@prism/core/object-model";
import { createRelayServer } from "../server/relay-server.js";

let relay: RelayInstance;
let identity: PrismIdentity;
let port: number;
let close: () => Promise<void>;

beforeAll(async () => {
  identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(collectionHostModule())
    .use(webhookModule())
    .build();
  await relay.start();

  // Create a test collection with an object
  const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
  if (!host) throw new Error("collection host not available");
  const store = host.create("test-rest");
  const now = new Date().toISOString();
  store.putObject({
    id: objectId("obj-1"),
    type: "task",
    name: "Test Task",
    parentId: null,
    position: 0,
    status: "open",
    tags: ["urgent"],
    date: now,
    endDate: null,
    description: "A test object",
    color: null,
    image: null,
    pinned: false,
    data: {},
    createdAt: now,
    updatedAt: now,
  });

  const server = createRelayServer({ relay, port: 0, disableCsrf: true });
  const info = await server.start();
  port = info.port;
  close = info.close;
});

afterAll(async () => {
  await close();
  await relay.stop();
});

function url(path: string): string {
  return `http://127.0.0.1:${port}${path}`;
}

describe("autorest-routes", () => {
  it("GET /api/rest/:collectionId lists objects", async () => {
    const res = await fetch(url("/api/rest/test-rest"));
    expect(res.status).toBe(200);
    const body = await res.json() as { objects: unknown[]; total: number };
    expect(body.total).toBe(1);
    expect(body.objects).toHaveLength(1);
  });

  it("GET /api/rest/:collectionId supports type filter", async () => {
    const res = await fetch(url("/api/rest/test-rest?type=task"));
    expect(res.status).toBe(200);
    const body = await res.json() as { total: number };
    expect(body.total).toBe(1);

    const res2 = await fetch(url("/api/rest/test-rest?type=contact"));
    const body2 = await res2.json() as { total: number };
    expect(body2.total).toBe(0);
  });

  it("GET /api/rest/:collectionId/:objectId returns single object", async () => {
    const res = await fetch(url("/api/rest/test-rest/obj-1"));
    expect(res.status).toBe(200);
    const body = await res.json() as { name: string; type: string };
    expect(body.name).toBe("Test Task");
    expect(body.type).toBe("task");
  });

  it("POST /api/rest/:collectionId creates an object", async () => {
    const res = await fetch(url("/api/rest/test-rest"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name: "New Task", type: "task", status: "open" }),
    });
    expect(res.status).toBe(201);
    const body = await res.json() as { ok: boolean; objectId: string };
    expect(body.ok).toBe(true);
    expect(body.objectId).toBeTruthy();
  });

  it("PUT /api/rest/:collectionId/:objectId updates an object", async () => {
    const res = await fetch(url("/api/rest/test-rest/obj-1"), {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name: "Updated Task" }),
    });
    expect(res.status).toBe(200);

    // Verify update
    const getRes = await fetch(url("/api/rest/test-rest/obj-1"));
    const body = await getRes.json() as { name: string };
    expect(body.name).toBe("Updated Task");
  });

  it("DELETE /api/rest/:collectionId/:objectId soft-deletes", async () => {
    const res = await fetch(url("/api/rest/test-rest/obj-1"), {
      method: "DELETE",
    });
    expect(res.status).toBe(200);

    // Should no longer appear in list
    const listRes = await fetch(url("/api/rest/test-rest?type=task"));
    const body = await listRes.json() as { objects: Array<{ id: string }> };
    const found = body.objects.find((o) => o.id === "obj-1");
    expect(found).toBeUndefined();
  });

  it("returns 404 for non-existent collection", async () => {
    const res = await fetch(url("/api/rest/nonexistent"));
    expect(res.status).toBe(404);
  });

  it("validates name on create", async () => {
    const res = await fetch(url("/api/rest/test-rest"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ type: "task" }),
    });
    expect(res.status).toBe(400);
  });
});
