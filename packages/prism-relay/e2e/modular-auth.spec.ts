/**
 * Modular Auth E2E — proves the four-way matrix:
 *   1. relay built with escrow only
 *   2. relay built with password-auth only
 *   3. relay built with both
 *   4. relay built with neither
 *
 * Each combination starts a real relay server on an ephemeral port and
 * exercises the HTTP API. Endpoints whose backing module is missing must
 * return 404; endpoints whose backing module is installed must work
 * end-to-end.
 */

import { test, expect } from "@playwright/test";
import type { APIRequestContext } from "@playwright/test";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  capabilityTokenModule,
  escrowModule,
  passwordAuthModule,
} from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createRelayServer } from "@prism/relay/server";

interface RelayHandle {
  relay: RelayInstance;
  baseUrl: string;
  close: () => Promise<void>;
}

async function startRelay(
  identity: PrismIdentity,
  build: (b: ReturnType<typeof createRelayBuilder>) => ReturnType<typeof createRelayBuilder>,
): Promise<RelayHandle> {
  const builder = createRelayBuilder({ relayDid: identity.did });
  // capability-tokens is needed by password-auth login to mint a token,
  // but is not strictly required by either module to install.
  builder.use(capabilityTokenModule(identity));
  const relay = build(builder).build();
  await relay.start();
  const server = createRelayServer({
    relay,
    port: 0,
    publicUrl: "http://localhost:0",
    disableCsrf: true,
  });
  const info = await server.start();
  return {
    relay,
    baseUrl: `http://localhost:${info.port}`,
    close: async () => {
      await info.close();
      await relay.stop();
    },
  };
}

let identity: PrismIdentity;
let escrowOnly: RelayHandle;
let passwordOnly: RelayHandle;
let both: RelayHandle;
let neither: RelayHandle;

test.beforeAll(async () => {
  identity = await createIdentity({ method: "key" });
  escrowOnly = await startRelay(identity, (b) => b.use(escrowModule()));
  passwordOnly = await startRelay(identity, (b) =>
    b.use(passwordAuthModule({ iterations: 1000 })),
  );
  both = await startRelay(identity, (b) =>
    b
      .use(escrowModule())
      .use(passwordAuthModule({ iterations: 1000 })),
  );
  // "Neither" still needs *some* module so the builder is non-empty.
  neither = await startRelay(identity, (b) => b.use(blindMailboxModule()));
});

test.afterAll(async () => {
  await escrowOnly.close();
  await passwordOnly.close();
  await both.close();
  await neither.close();
});

// ── Helpers ───────────────────────────────────────────────────────────────

async function depositEscrow(
  request: APIRequestContext,
  baseUrl: string,
  depositorId: string,
  payload: string,
) {
  return request.post(`${baseUrl}/api/escrow/deposit`, {
    data: { depositorId, encryptedPayload: payload },
  });
}

async function registerUser(
  request: APIRequestContext,
  baseUrl: string,
  username: string,
  password: string,
) {
  return request.post(`${baseUrl}/api/auth/password/register`, {
    data: { username, password },
  });
}

async function loginUser(
  request: APIRequestContext,
  baseUrl: string,
  username: string,
  password: string,
) {
  return request.post(`${baseUrl}/api/auth/password/login`, {
    data: { username, password },
  });
}

// ── Matrix tests ──────────────────────────────────────────────────────────

test.describe("Relay built with escrow only", () => {
  test("escrow endpoints work", async ({ request }) => {
    const res = await depositEscrow(request, escrowOnly.baseUrl, "alice", "blob");
    expect(res.status()).toBe(201);
    const deposit = await res.json();
    expect(deposit.depositorId).toBe("alice");
  });

  test("password-auth endpoints return 404", async ({ request }) => {
    const reg = await registerUser(request, escrowOnly.baseUrl, "x", "y");
    expect(reg.status()).toBe(404);
    const login = await loginUser(request, escrowOnly.baseUrl, "x", "y");
    expect(login.status()).toBe(404);
  });
});

test.describe("Relay built with password-auth only", () => {
  test("password-auth endpoints work end-to-end", async ({ request }) => {
    const reg = await registerUser(
      request,
      passwordOnly.baseUrl,
      "alice",
      "passw0rd",
    );
    expect(reg.status()).toBe(201);
    const login = await loginUser(
      request,
      passwordOnly.baseUrl,
      "alice",
      "passw0rd",
    );
    expect(login.status()).toBe(200);
    const body = await login.json();
    expect(body.ok).toBe(true);
    expect(typeof body.token).toBe("string");
  });

  test("escrow endpoints return 404", async ({ request }) => {
    const res = await depositEscrow(
      request,
      passwordOnly.baseUrl,
      "alice",
      "blob",
    );
    expect(res.status()).toBe(404);
  });
});

test.describe("Relay built with escrow + password-auth", () => {
  test("both subsystems work side-by-side", async ({ request }) => {
    const dep = await depositEscrow(request, both.baseUrl, "alice", "blob");
    expect(dep.status()).toBe(201);

    const reg = await registerUser(request, both.baseUrl, "alice", "secret");
    expect(reg.status()).toBe(201);

    const login = await loginUser(request, both.baseUrl, "alice", "secret");
    expect(login.status()).toBe(200);

    // The password user gets a Prism capability token via the same identity
    // as the relay, even though their DID is did:password:*.
    const body = await login.json();
    expect(body.did).toBe("did:password:alice");
    expect(typeof body.token).toBe("string");
  });

  test("DID for the password user is independent of the escrow depositor", async ({
    request,
  }) => {
    // Same logical user, two different identifiers.
    await registerUser(request, both.baseUrl, "carol", "secret");
    await depositEscrow(request, both.baseUrl, "did:password:carol", "blob");
    const list = await request.get(
      `${both.baseUrl}/api/escrow/did:password:carol`,
    );
    const items = await list.json();
    expect(items.length).toBeGreaterThanOrEqual(1);
  });
});

test.describe("Relay built with neither escrow nor password-auth", () => {
  test("both subsystems return 404", async ({ request }) => {
    const dep = await depositEscrow(request, neither.baseUrl, "alice", "blob");
    expect(dep.status()).toBe(404);
    const reg = await registerUser(request, neither.baseUrl, "alice", "secret");
    expect(reg.status()).toBe(404);
  });

  test("relay still serves status routes", async ({ request }) => {
    const res = await request.get(`${neither.baseUrl}/api/status`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.running).toBe(true);
  });
});
