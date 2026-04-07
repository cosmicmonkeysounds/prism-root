import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import { createRelayBuilder, blindMailboxModule, collectionHostModule } from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createCollectionRoutes } from "./collection-routes.js";

let relay: RelayInstance;
let relayNoCollections: RelayInstance;
let identity: PrismIdentity;

beforeAll(async () => {
  identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(collectionHostModule())
    .build();
  await relay.start();

  relayNoCollections = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .build();
  await relayNoCollections.start();
});

describe("collection-routes", () => {
  it("returns 404 when collections module not installed", async () => {
    const app = createCollectionRoutes(relayNoCollections);
    const res = await app.request("/");
    expect(res.status).toBe(404);
  });

  it("list returns empty array initially", async () => {
    const app = createCollectionRoutes(relay);
    const res = await app.request("/");
    expect(res.status).toBe(200);
    const body = (await res.json()) as string[];
    expect(body).toEqual([]);
  });

  it("create adds a collection", async () => {
    const app = createCollectionRoutes(relay);
    const res = await app.request("/", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id: "col-1" }),
    });
    expect(res.status).toBe(201);
    const body = (await res.json()) as { id: string };
    expect(body.id).toBe("col-1");

    // Verify it appears in the list
    const listRes = await app.request("/");
    const list = (await listRes.json()) as string[];
    expect(list).toContain("col-1");
  });

  it("snapshot returns base64 data", async () => {
    const app = createCollectionRoutes(relay);
    const res = await app.request("/col-1/snapshot");
    expect(res.status).toBe(200);
    const body = (await res.json()) as { snapshot: string };
    expect(typeof body.snapshot).toBe("string");
    expect(body.snapshot.length).toBeGreaterThan(0);
  });

  it("import accepts base64 data", async () => {
    const app = createCollectionRoutes(relay);

    // Get current snapshot to use as import data
    const snapRes = await app.request("/col-1/snapshot");
    const snapBody = (await snapRes.json()) as { snapshot: string };

    const res = await app.request("/col-1/import", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ data: snapBody.snapshot }),
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as { ok: boolean };
    expect(body.ok).toBe(true);
  });

  it("delete removes collection", async () => {
    const app = createCollectionRoutes(relay);

    // Create a collection to delete
    await app.request("/", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id: "col-delete" }),
    });

    const res = await app.request("/col-delete", { method: "DELETE" });
    expect(res.status).toBe(200);
    const body = (await res.json()) as { ok: boolean };
    expect(body.ok).toBe(true);

    // Verify it no longer appears in the list
    const listRes = await app.request("/");
    const list = (await listRes.json()) as string[];
    expect(list).not.toContain("col-delete");
  });

  it("delete returns 404 for unknown collection", async () => {
    const app = createCollectionRoutes(relay);
    const res = await app.request("/nonexistent", { method: "DELETE" });
    expect(res.status).toBe(404);
  });

  it("snapshot returns 404 for unknown collection", async () => {
    const app = createCollectionRoutes(relay);
    const res = await app.request("/nonexistent/snapshot");
    expect(res.status).toBe(404);
  });
});
