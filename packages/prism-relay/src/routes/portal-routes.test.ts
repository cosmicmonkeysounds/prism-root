import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  sovereignPortalModule,
  collectionHostModule,
} from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createPortalRoutes } from "./portal-routes.js";

let relay: RelayInstance;
let relayNoPortals: RelayInstance;
let relayWithCollections: RelayInstance;

beforeAll(async () => {
  const identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(sovereignPortalModule())
    .build();
  await relay.start();

  relayNoPortals = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .build();
  await relayNoPortals.start();

  relayWithCollections = createRelayBuilder({ relayDid: identity.did })
    .use(sovereignPortalModule())
    .use(collectionHostModule())
    .build();
  await relayWithCollections.start();
});

describe("portal-routes", () => {
  it("returns 404 when portals module not installed", async () => {
    const app = createPortalRoutes(relayNoPortals);
    const res = await app.request("/");
    expect(res.status).toBe(404);
  });

  it("CRUD lifecycle", async () => {
    const app = createPortalRoutes(relay);

    // List empty
    let res = await app.request("/");
    expect(res.status).toBe(200);
    let list = await res.json() as unknown[];
    expect(list).toHaveLength(0);

    // Register
    res = await app.request("/", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: "My Site",
        level: 1,
        collectionId: "col-1",
        basePath: "/",
        isPublic: true,
      }),
    });
    expect(res.status).toBe(201);
    const created = await res.json() as Record<string, unknown>;
    const portalId = created["portalId"] as string;
    expect(typeof portalId).toBe("string");

    // Get by ID
    res = await app.request(`/${portalId}`);
    expect(res.status).toBe(200);
    const got = await res.json() as Record<string, unknown>;
    expect(got["name"]).toBe("My Site");

    // Get missing
    res = await app.request("/nonexistent");
    expect(res.status).toBe(404);

    // Delete
    res = await app.request(`/${portalId}`, { method: "DELETE" });
    expect(res.status).toBe(200);

    // List empty again
    res = await app.request("/");
    list = await res.json() as unknown[];
    expect(list).toHaveLength(0);
  });

  it("export bundles portal manifest as JSON download", async () => {
    const app = createPortalRoutes(relay);

    const create = await app.request("/", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: "Export Test",
        level: 1,
        collectionId: "col-export",
        basePath: "/",
        isPublic: true,
      }),
    });
    const { portalId } = (await create.json()) as { portalId: string };

    const res = await app.request(`/${portalId}/export`);
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toContain("application/json");
    expect(res.headers.get("content-disposition")).toContain("attachment");

    const bundle = (await res.json()) as Record<string, unknown>;
    expect(bundle["version"]).toBe(1);
    expect(typeof bundle["exportedAt"]).toBe("string");
    expect((bundle["portal"] as Record<string, unknown>)["name"]).toBe("Export Test");
    // No collection host → collection body is null
    expect(bundle["collection"]).toBe(null);
  });

  it("export includes collection snapshot when host module installed", async () => {
    const app = createPortalRoutes(relayWithCollections);

    // Create a backing collection first
    // Collections host is accessed via the relay capability, not routes here.
    // Create the portal, then ensure the collection exists via capability.
    const host = relayWithCollections.getCapability<
      import("@prism/core/relay").CollectionHost
    >("relay:collections");
    host?.create("col-with-snap");

    const create = await app.request("/", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: "With Snap",
        level: 1,
        collectionId: "col-with-snap",
        basePath: "/",
        isPublic: true,
      }),
    });
    const { portalId } = (await create.json()) as { portalId: string };

    const res = await app.request(`/${portalId}/export`);
    expect(res.status).toBe(200);
    const bundle = (await res.json()) as Record<string, unknown>;
    const collection = bundle["collection"] as Record<string, unknown> | null;
    expect(collection).not.toBe(null);
    expect(collection?.["id"]).toBe("col-with-snap");
    expect(typeof collection?.["snapshot"]).toBe("string");
  });

  it("export returns 404 for missing portal", async () => {
    const app = createPortalRoutes(relay);
    const res = await app.request("/no-such-portal/export");
    expect(res.status).toBe(404);
  });
});
