import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  acmeCertificateModule,
} from "@prism/core/relay";
import type { RelayInstance, AcmeCertificateManager } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { createAcmeRoutes, createAcmeManagementRoutes } from "./acme-routes.js";

let relay: RelayInstance;

beforeAll(async () => {
  const identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(acmeCertificateModule())
    .build();
  await relay.start();
});

describe("acme-routes", () => {
  it("responds to ACME HTTP-01 challenge", async () => {
    const mgr = relay.getCapability<AcmeCertificateManager>(RELAY_CAPABILITIES.ACME);
    if (!mgr) throw new Error("ACME capability not available");
    mgr.addChallenge({
      domain: "test.example.com",
      token: "test-token-123",
      keyAuthorization: "test-token-123.thumbprint",
      createdAt: new Date().toISOString(),
      expiresAt: new Date(Date.now() + 300_000).toISOString(),
    });

    const app = createAcmeRoutes(relay);
    const res = await app.request("/test-token-123");
    expect(res.status).toBe(200);
    const text = await res.text();
    expect(text).toBe("test-token-123.thumbprint");
  });

  it("returns 404 for unknown challenge token", async () => {
    const app = createAcmeRoutes(relay);
    const res = await app.request("/nonexistent");
    expect(res.status).toBe(404);
  });

  it("returns 404 for expired challenge", async () => {
    const mgr = relay.getCapability<AcmeCertificateManager>(RELAY_CAPABILITIES.ACME);
    if (!mgr) throw new Error("ACME capability not available");
    mgr.addChallenge({
      domain: "expired.example.com",
      token: "expired-token",
      keyAuthorization: "expired.thumbprint",
      createdAt: new Date(Date.now() - 600_000).toISOString(),
      expiresAt: new Date(Date.now() - 1000).toISOString(),
    });

    const app = createAcmeRoutes(relay);
    const res = await app.request("/expired-token");
    expect(res.status).toBe(404);
  });
});

describe("acme-management-routes", () => {
  it("creates and deletes challenges", async () => {
    const app = createAcmeManagementRoutes(relay);

    const createRes = await app.request("/challenges", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        domain: "mgmt.example.com",
        token: "mgmt-token",
        keyAuthorization: "mgmt-token.thumbprint",
      }),
    });
    expect(createRes.status).toBe(201);

    const deleteRes = await app.request("/challenges/mgmt-token", {
      method: "DELETE",
    });
    expect(deleteRes.status).toBe(200);
  });

  it("manages certificate lifecycle", async () => {
    const app = createAcmeManagementRoutes(relay);

    // Store certificate
    const storeRes = await app.request("/certificates", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        domain: "cert.example.com",
        certificate: "-----BEGIN CERTIFICATE-----\ntest\n-----END CERTIFICATE-----",
        privateKey: "-----BEGIN PRIVATE KEY-----\ntest\n-----END PRIVATE KEY-----",
        expiresAt: new Date(Date.now() + 86_400_000 * 90).toISOString(),
      }),
    });
    expect(storeRes.status).toBe(201);

    // List certificates
    const listRes = await app.request("/certificates");
    expect(listRes.status).toBe(200);
    const certs = await listRes.json() as Array<{ domain: string }>;
    expect(certs.some((c) => c.domain === "cert.example.com")).toBe(true);

    // Get certificate (should NOT expose private key)
    const getRes = await app.request("/certificates/cert.example.com");
    expect(getRes.status).toBe(200);
    const cert = await getRes.json() as Record<string, unknown>;
    expect(cert["domain"]).toBe("cert.example.com");
    expect(cert["certificate"]).toBeDefined();

    // Delete certificate
    const delRes = await app.request("/certificates/cert.example.com", {
      method: "DELETE",
    });
    expect(delRes.status).toBe(200);

    // Verify deleted
    const getRes2 = await app.request("/certificates/cert.example.com");
    expect(getRes2.status).toBe(404);
  });
});
