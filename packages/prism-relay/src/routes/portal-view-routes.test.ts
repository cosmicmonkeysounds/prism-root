import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  sovereignPortalModule,
  collectionHostModule,
} from "@prism/core/relay";
import type { RelayInstance, PortalRegistry, CollectionHost } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { createPortalViewRoutes } from "./portal-view-routes.js";

let relay: RelayInstance;

beforeAll(async () => {
  const identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(sovereignPortalModule())
    .use(collectionHostModule())
    .build();
  await relay.start();

  // Register a portal with a backing collection
  const collections = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
  if (!collections) throw new Error("CollectionHost not installed");
  const store = collections.create("col-test");
  store.putObject({
    id: "obj-1",
    type: "task",
    name: "Test Task",
    parentId: null,
    position: 0,
    status: "active",
    tags: ["v1"],
    date: null,
    endDate: null,
    description: "A test task",
    color: null,
    image: null,
    pinned: false,
    data: {},
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  });

  const registry = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS);
  if (!registry) throw new Error("PortalRegistry not installed");
  registry.register({
    name: "Public Portal",
    level: 1,
    collectionId: "col-test",
    basePath: "/",
    isPublic: true,
  });
  registry.register({
    name: "Private Portal",
    level: 3,
    collectionId: "col-test",
    basePath: "/private",
    isPublic: false,
    accessScope: "admin",
  });
  registry.register({
    name: "Live Portal",
    level: 2,
    collectionId: "col-test",
    basePath: "/live",
    isPublic: true,
  });
});

function getPortalId(name: string): string {
  const registry = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS);
  if (!registry) throw new Error("PortalRegistry not installed");
  const found = registry.list().find((p) => p.name === name);
  if (!found) throw new Error(`Portal ${name} not found`);
  return found.portalId;
}

describe("portal-view-routes", () => {
  it("lists public portals on /", async () => {
    const app = createPortalViewRoutes(relay);
    const res = await app.request("/");
    expect(res.status).toBe(200);
    const html = await res.text();
    expect(html).toContain("Sovereign Portals");
    expect(html).toContain("Public Portal");
    expect(html).toContain("Live Portal");
    // Private portal should not be listed
    expect(html).not.toContain("Private Portal");
  });

  it("renders Level 1 portal as static HTML", async () => {
    const id = getPortalId("Public Portal");
    const app = createPortalViewRoutes(relay);
    const res = await app.request(`/${id}`);
    expect(res.status).toBe(200);
    const html = await res.text();
    expect(html).toContain("Public Portal");
    expect(html).toContain("Test Task");
    expect(html).toContain("active");
    expect(html).toContain("v1");
    // Level 1 should not have live update script
    expect(html).not.toContain("WebSocket");
  });

  it("renders Level 2 portal with live update script when wsBaseUrl provided", async () => {
    const id = getPortalId("Live Portal");
    const app = createPortalViewRoutes(relay, "ws://localhost:4444");
    const res = await app.request(`/${id}`);
    expect(res.status).toBe(200);
    const html = await res.text();
    expect(html).toContain("Live Portal");
    expect(html).toContain("ws://localhost:4444/ws/relay");
    expect(html).toContain("sync-request");
  });

  it("returns 403 for non-public portals", async () => {
    const id = getPortalId("Private Portal");
    const app = createPortalViewRoutes(relay);
    const res = await app.request(`/${id}`);
    expect(res.status).toBe(403);
  });

  it("returns 404 for unknown portal ID", async () => {
    const app = createPortalViewRoutes(relay);
    const res = await app.request("/nonexistent");
    expect(res.status).toBe(404);
  });

  it("serves JSON snapshot at /:id/snapshot.json", async () => {
    const id = getPortalId("Public Portal");
    const app = createPortalViewRoutes(relay);
    const res = await app.request(`/${id}/snapshot.json`);
    expect(res.status).toBe(200);
    const json = await res.json() as Record<string, unknown>;
    expect(json["objectCount"]).toBe(1);
    expect(json["portal"]).toBeDefined();
    expect(json["objects"]).toBeDefined();
    expect(json["generatedAt"]).toBeDefined();
  });

  it("returns 403 for JSON snapshot of private portal", async () => {
    const id = getPortalId("Private Portal");
    const app = createPortalViewRoutes(relay);
    const res = await app.request(`/${id}/snapshot.json`);
    expect(res.status).toBe(403);
  });
});
