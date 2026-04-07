import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import { createRelayBuilder, blindMailboxModule, hashcashModule } from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createHashcashMinter } from "@prism/core/trust";
import { createHashcashRoutes } from "./hashcash-routes.js";

let relay: RelayInstance;
let relayNoHashcash: RelayInstance;
let identity: PrismIdentity;

beforeAll(async () => {
  identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(hashcashModule({ bits: 4 }))
    .build();
  await relay.start();

  relayNoHashcash = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .build();
  await relayNoHashcash.start();
});

describe("hashcash-routes", () => {
  it("returns 404 when hashcash module not installed", async () => {
    const app = createHashcashRoutes(relayNoHashcash);
    const res = await app.request("/challenge", { method: "POST" });
    expect(res.status).toBe(404);
  });

  it("challenge returns a challenge object with resource, bits, salt, issuedAt", async () => {
    const app = createHashcashRoutes(relay);
    const res = await app.request("/challenge", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ resource: "test-resource" }),
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as Record<string, unknown>;
    expect(body["resource"]).toBe("test-resource");
    expect(body["bits"]).toBe(4);
    expect(typeof body["salt"]).toBe("string");
    expect(typeof body["issuedAt"]).toBe("string");
  });

  it("verify with valid proof returns valid=true", async () => {
    const app = createHashcashRoutes(relay);

    // Get a challenge
    const challengeRes = await app.request("/challenge", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ resource: "verify-test" }),
    });
    const challenge = (await challengeRes.json()) as {
      resource: string;
      bits: number;
      issuedAt: string;
      salt: string;
    };

    // Solve the challenge
    const minter = createHashcashMinter();
    const proof = await minter.mint(challenge);

    // Verify
    const res = await app.request("/verify", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(proof),
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as { valid: boolean };
    expect(body.valid).toBe(true);
  });

  it("verify with invalid proof returns valid=false", async () => {
    const app = createHashcashRoutes(relay);

    // Get a challenge
    const challengeRes = await app.request("/challenge", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ resource: "invalid-test" }),
    });
    const challenge = (await challengeRes.json()) as {
      resource: string;
      bits: number;
      issuedAt: string;
      salt: string;
    };

    // Submit a fake proof
    const res = await app.request("/verify", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        challenge,
        counter: 0,
        hash: "0000000000000000000000000000000000000000000000000000000000000000",
      }),
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as { valid: boolean };
    expect(body.valid).toBe(false);
  });
});
