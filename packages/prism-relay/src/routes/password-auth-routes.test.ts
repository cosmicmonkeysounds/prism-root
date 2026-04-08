import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  passwordAuthModule,
  capabilityTokenModule,
} from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createPasswordAuthRoutes } from "./password-auth-routes.js";

let identity: PrismIdentity;
let relayWithPassword: RelayInstance;
let relayWithPasswordAndTokens: RelayInstance;
let relayWithoutPassword: RelayInstance;

beforeAll(async () => {
  identity = await createIdentity({ method: "key" });

  relayWithPassword = createRelayBuilder({ relayDid: identity.did })
    .use(passwordAuthModule({ iterations: 1000 }))
    .build();
  await relayWithPassword.start();

  relayWithPasswordAndTokens = createRelayBuilder({ relayDid: identity.did })
    .use(capabilityTokenModule(identity))
    .use(passwordAuthModule({ iterations: 1000 }))
    .build();
  await relayWithPasswordAndTokens.start();

  relayWithoutPassword = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .build();
  await relayWithoutPassword.start();
});

describe("password-auth-routes", () => {
  it("returns 404 when password-auth module is not installed", async () => {
    const app = createPasswordAuthRoutes(relayWithoutPassword);
    const res = await app.request("/register", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "x", password: "y" }),
    });
    expect(res.status).toBe(404);
  });

  it("registers a new user and rejects duplicates", async () => {
    const app = createPasswordAuthRoutes(relayWithPassword);

    const first = await app.request("/register", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "alice", password: "secret-1" }),
    });
    expect(first.status).toBe(201);
    const body = (await first.json()) as Record<string, unknown>;
    expect(body["username"]).toBe("alice");
    expect(body["did"]).toBe("did:password:alice");
    expect(body).not.toHaveProperty("passwordHash");
    expect(body).not.toHaveProperty("salt");

    const dup = await app.request("/register", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "alice", password: "other" }),
    });
    expect(dup.status).toBe(409);
  });

  it("validates required fields on register", async () => {
    const app = createPasswordAuthRoutes(relayWithPassword);
    const res = await app.request("/register", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "missing-password" }),
    });
    expect(res.status).toBe(400);
  });

  it("login succeeds with correct password (no token when tokens module absent)", async () => {
    const app = createPasswordAuthRoutes(relayWithPassword);
    await app.request("/register", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "bob", password: "hunter2" }),
    });

    const res = await app.request("/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "bob", password: "hunter2" }),
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as Record<string, unknown>;
    expect(body["ok"]).toBe(true);
    expect(body["did"]).toBe("did:password:bob");
    expect(body["token"]).toBeNull();
  });

  it("login issues a capability token when tokens module installed", async () => {
    const app = createPasswordAuthRoutes(relayWithPasswordAndTokens);
    await app.request("/register", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "carol", password: "p@ssw0rd" }),
    });

    const res = await app.request("/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "carol", password: "p@ssw0rd" }),
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as Record<string, unknown>;
    expect(body["ok"]).toBe(true);
    expect(typeof body["token"]).toBe("string");
    expect((body["token"] as string).length).toBeGreaterThan(0);
    expect(body["expiresAt"]).toBeTruthy();
  });

  it("login rejects wrong password with 401", async () => {
    const app = createPasswordAuthRoutes(relayWithPassword);
    await app.request("/register", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "dave", password: "real-pass" }),
    });
    const res = await app.request("/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "dave", password: "wrong" }),
    });
    expect(res.status).toBe(401);
  });

  it("login returns 404 for unknown user", async () => {
    const app = createPasswordAuthRoutes(relayWithPassword);
    const res = await app.request("/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "ghost", password: "anything" }),
    });
    expect(res.status).toBe(404);
  });

  it("change password rotates credentials", async () => {
    const app = createPasswordAuthRoutes(relayWithPassword);
    await app.request("/register", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "eve", password: "old" }),
    });
    const change = await app.request("/change", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "eve", oldPassword: "old", newPassword: "new" }),
    });
    expect(change.status).toBe(200);

    const tryOld = await app.request("/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "eve", password: "old" }),
    });
    expect(tryOld.status).toBe(401);

    const tryNew = await app.request("/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "eve", password: "new" }),
    });
    expect(tryNew.status).toBe(200);
  });

  it("DELETE /:username requires the current password", async () => {
    const app = createPasswordAuthRoutes(relayWithPassword);
    await app.request("/register", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "frank", password: "delete-me" }),
    });

    const wrong = await app.request("/frank", {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ password: "nope" }),
    });
    expect(wrong.status).toBe(401);

    const right = await app.request("/frank", {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ password: "delete-me" }),
    });
    expect(right.status).toBe(200);

    const gone = await app.request("/frank");
    expect(gone.status).toBe(404);
  });

  it("GET /:username returns the redacted record", async () => {
    const app = createPasswordAuthRoutes(relayWithPassword);
    await app.request("/register", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: "grace", password: "secret" }),
    });
    const res = await app.request("/grace");
    expect(res.status).toBe(200);
    const body = (await res.json()) as Record<string, unknown>;
    expect(body["username"]).toBe("grace");
    expect(body).not.toHaveProperty("passwordHash");
    expect(body).not.toHaveProperty("salt");
  });
});
