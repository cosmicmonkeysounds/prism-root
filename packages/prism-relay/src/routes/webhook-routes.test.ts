import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import { createRelayBuilder, blindMailboxModule, webhookModule } from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createWebhookRoutes } from "./webhook-routes.js";

let relay: RelayInstance;
let relayNoWebhooks: RelayInstance;

beforeAll(async () => {
  const identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(webhookModule())
    .build();
  await relay.start();

  relayNoWebhooks = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .build();
  await relayNoWebhooks.start();
});

describe("webhook-routes", () => {
  it("returns 404 when webhooks module not installed", async () => {
    const app = createWebhookRoutes(relayNoWebhooks);
    const res = await app.request("/");
    expect(res.status).toBe(404);
    const body = await res.json() as Record<string, unknown>;
    expect(body["error"]).toContain("not installed");
  });

  it("CRUD lifecycle", async () => {
    const app = createWebhookRoutes(relay);

    // List empty
    let res = await app.request("/");
    expect(res.status).toBe(200);
    let list = await res.json() as unknown[];
    expect(list).toHaveLength(0);

    // Register
    res = await app.request("/", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url: "https://example.com/hook", events: ["crdt.update"], active: true }),
    });
    expect(res.status).toBe(201);
    const created = await res.json() as Record<string, unknown>;
    expect(typeof created["id"]).toBe("string");
    const webhookId = created["id"] as string;

    // List with one
    res = await app.request("/");
    list = await res.json() as unknown[];
    expect(list).toHaveLength(1);

    // Deliveries (empty)
    res = await app.request(`/${webhookId}/deliveries`);
    expect(res.status).toBe(200);
    const deliveries = await res.json() as unknown[];
    expect(deliveries).toHaveLength(0);

    // Unregister
    res = await app.request(`/${webhookId}`, { method: "DELETE" });
    expect(res.status).toBe(200);

    // Unregister again → 404
    res = await app.request(`/${webhookId}`, { method: "DELETE" });
    expect(res.status).toBe(404);
  });
});
