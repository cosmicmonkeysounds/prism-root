import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  webhookModule,
  sovereignPortalModule,
} from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createStatusRoutes } from "./status-routes.js";

let relay: RelayInstance;

beforeAll(async () => {
  const identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(webhookModule())
    .use(sovereignPortalModule())
    .build();
  await relay.start();
});

describe("status-routes", () => {
  it("GET /status returns relay state", async () => {
    const app = createStatusRoutes(relay);
    const res = await app.request("/status");
    expect(res.status).toBe(200);
    const body = await res.json() as Record<string, unknown>;
    expect(body["running"]).toBe(true);
    expect(typeof body["did"]).toBe("string");
    expect(Array.isArray(body["modules"])).toBe(true);
    expect(Array.isArray(body["peers"])).toBe(true);
  });

  it("GET /modules returns module list", async () => {
    const app = createStatusRoutes(relay);
    const res = await app.request("/modules");
    expect(res.status).toBe(200);
    const body = await res.json() as Array<{ name: string }>;
    expect(body.length).toBeGreaterThanOrEqual(4);
    expect(body.some((m) => m.name === "blind-mailbox")).toBe(true);
  });
});
