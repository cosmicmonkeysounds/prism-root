import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import { createRelayBuilder, blindMailboxModule, sovereignPortalModule } from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createPortalRoutes } from "./portal-routes.js";

let relay: RelayInstance;
let relayNoPortals: RelayInstance;

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
});
