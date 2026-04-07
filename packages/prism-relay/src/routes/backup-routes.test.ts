import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  sovereignPortalModule,
  webhookModule,
  federationModule,
  peerTrustModule,
} from "@prism/core/relay";
import type {
  RelayInstance,
  PortalRegistry,
} from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { createBackupRoutes } from "./backup-routes.js";

let relay: RelayInstance;

beforeAll(async () => {
  const identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(sovereignPortalModule())
    .use(webhookModule())
    .use(federationModule())
    .use(peerTrustModule())
    .build();
  await relay.start();
});

describe("backup-routes", () => {
  it("GET / returns state with expected arrays", async () => {
    const app = createBackupRoutes(relay);
    const res = await app.request("/");
    expect(res.status).toBe(200);
    const body = (await res.json()) as Record<string, unknown>;
    expect(Array.isArray(body["portals"])).toBe(true);
    expect(Array.isArray(body["webhooks"])).toBe(true);
    expect(Array.isArray(body["federationPeers"])).toBe(true);
    expect(Array.isArray(body["flaggedHashes"])).toBe(true);
  });

  it("GET / includes registered portals", async () => {
    const portals = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS);
    portals?.register({
      name: "backup-test-portal",
      level: "level-1" as const,
      collectionId: "col-1",
      basePath: "/test",
      isPublic: true,
    });

    const app = createBackupRoutes(relay);
    const res = await app.request("/");
    expect(res.status).toBe(200);
    const body = (await res.json()) as Record<string, unknown>;
    const portalList = body["portals"] as Array<Record<string, unknown>>;
    expect(portalList.length).toBeGreaterThanOrEqual(1);
    expect(portalList.some((p) => p["name"] === "backup-test-portal")).toBe(true);
  });

  it("POST / restores portals from backup and returns counts", async () => {
    const app = createBackupRoutes(relay);
    const res = await app.request("/", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        portals: [
          {
            name: "restored-portal",
            level: "level-1",
            collectionId: "col-r",
            basePath: "/restored",
            isPublic: false,
          },
        ],
        webhooks: [
          { url: "https://example.com/hook", events: ["crdt.update"], active: true },
        ],
      }),
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as Record<string, unknown>;
    expect(body["ok"]).toBe(true);
    const restored = body["restored"] as Record<string, number>;
    expect(restored["portals"]).toBe(1);
    expect(restored["webhooks"]).toBe(1);
  });

  it("POST / with empty body returns zeros", async () => {
    const app = createBackupRoutes(relay);
    const res = await app.request("/", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as Record<string, unknown>;
    expect(body["ok"]).toBe(true);
    const restored = body["restored"] as Record<string, number>;
    expect(restored["portals"]).toBe(0);
    expect(restored["webhooks"]).toBe(0);
    expect(restored["templates"]).toBe(0);
    expect(restored["certificates"]).toBe(0);
    expect(restored["federationPeers"]).toBe(0);
    expect(restored["flaggedHashes"]).toBe(0);
    expect(restored["collections"]).toBe(0);
  });
});
