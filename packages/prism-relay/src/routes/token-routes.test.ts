import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import { createRelayBuilder, blindMailboxModule, capabilityTokenModule } from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createTokenRoutes } from "./token-routes.js";

let relay: RelayInstance;
let relayNoTokens: RelayInstance;
let identity: PrismIdentity;

beforeAll(async () => {
  identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(capabilityTokenModule(identity))
    .build();
  await relay.start();

  relayNoTokens = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .build();
  await relayNoTokens.start();
});

describe("token-routes", () => {
  it("returns 404 when tokens module not installed", async () => {
    const app = createTokenRoutes(relayNoTokens);
    const res = await app.request("/issue", { method: "POST" });
    expect(res.status).toBe(404);
  });

  it("issue/verify/revoke lifecycle", async () => {
    const app = createTokenRoutes(relay);

    // Issue
    let res = await app.request("/issue", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        subject: "*",
        permissions: ["read"],
        scope: "collection-1",
      }),
    });
    expect(res.status).toBe(201);
    const issued = await res.json() as Record<string, unknown>;
    expect(typeof issued["tokenId"]).toBe("string");
    expect(typeof issued["signature"]).toBe("string"); // base64

    // Verify
    res = await app.request("/verify", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(issued),
    });
    expect(res.status).toBe(200);
    const verified = await res.json() as Record<string, unknown>;
    expect(verified["valid"]).toBe(true);

    // Revoke
    res = await app.request("/revoke", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ tokenId: issued["tokenId"] }),
    });
    expect(res.status).toBe(200);

    // Verify after revoke → invalid
    res = await app.request("/verify", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(issued),
    });
    const result = await res.json() as Record<string, unknown>;
    expect(result["valid"]).toBe(false);
  });
});
