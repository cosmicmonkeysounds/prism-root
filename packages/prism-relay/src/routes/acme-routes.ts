import { Hono } from "hono";
import type { RelayInstance, AcmeCertificateManager } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";

export function createAcmeRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function manager(): AcmeCertificateManager | undefined {
    return relay.getCapability<AcmeCertificateManager>(RELAY_CAPABILITIES.ACME);
  }

  // ACME HTTP-01 challenge response (Let's Encrypt calls this)
  app.get("/:token", (c) => {
    const mgr = manager();
    if (!mgr) return c.text("ACME not available", 404);

    const challenge = mgr.getChallenge(c.req.param("token"));
    if (!challenge) return c.text("Challenge not found", 404);

    // Check expiry
    if (new Date(challenge.expiresAt).getTime() < Date.now()) {
      mgr.removeChallenge(challenge.token);
      return c.text("Challenge expired", 404);
    }

    // Return key authorization as plain text (ACME HTTP-01 spec)
    return c.text(challenge.keyAuthorization);
  });

  return app;
}

export function createAcmeManagementRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function manager(): AcmeCertificateManager | undefined {
    return relay.getCapability<AcmeCertificateManager>(RELAY_CAPABILITIES.ACME);
  }

  app.use("/*", async (c, next) => {
    if (!manager()) {
      return c.json({ error: "acme module not installed" }, 404);
    }
    await next();
  });

  // Register an ACME challenge
  app.post("/challenges", async (c) => {
    const mgr = manager();
    if (!mgr) return c.json({ error: "acme not available" }, 404);
    const body = await c.req.json<{
      domain: string;
      token: string;
      keyAuthorization: string;
      expiresInMs?: number;
    }>();

    const now = new Date();
    const expiresAt = new Date(now.getTime() + (body.expiresInMs ?? 300_000)).toISOString();

    mgr.addChallenge({
      domain: body.domain,
      token: body.token,
      keyAuthorization: body.keyAuthorization,
      createdAt: now.toISOString(),
      expiresAt,
    });

    return c.json({ ok: true, token: body.token, expiresAt }, 201);
  });

  // Remove a challenge
  app.delete("/challenges/:token", (c) => {
    const mgr = manager();
    if (!mgr) return c.json({ error: "acme not available" }, 404);
    const ok = mgr.removeChallenge(c.req.param("token"));
    if (!ok) return c.json({ error: "challenge not found" }, 404);
    return c.json({ ok: true });
  });

  // List certificates
  app.get("/certificates", (c) => {
    const mgr = manager();
    if (!mgr) return c.json({ error: "acme not available" }, 404);
    // Don't expose private keys in listing
    const certs = mgr.listCertificates().map((cert) => ({
      domain: cert.domain,
      issuedAt: cert.issuedAt,
      expiresAt: cert.expiresAt,
      active: cert.active,
    }));
    return c.json(certs);
  });

  // Store a certificate
  app.post("/certificates", async (c) => {
    const mgr = manager();
    if (!mgr) return c.json({ error: "acme not available" }, 404);
    const body = await c.req.json<{
      domain: string;
      certificate: string;
      privateKey: string;
      expiresAt: string;
    }>();

    mgr.setCertificate({
      domain: body.domain,
      certificate: body.certificate,
      privateKey: body.privateKey,
      issuedAt: new Date().toISOString(),
      expiresAt: body.expiresAt,
      active: true,
    });

    return c.json({ ok: true, domain: body.domain }, 201);
  });

  // Get a certificate for a domain
  app.get("/certificates/:domain", (c) => {
    const mgr = manager();
    if (!mgr) return c.json({ error: "acme not available" }, 404);
    const cert = mgr.getCertificate(c.req.param("domain"));
    if (!cert) return c.json({ error: "certificate not found" }, 404);
    return c.json({
      domain: cert.domain,
      certificate: cert.certificate,
      issuedAt: cert.issuedAt,
      expiresAt: cert.expiresAt,
      active: cert.active,
    });
  });

  // Remove a certificate
  app.delete("/certificates/:domain", (c) => {
    const mgr = manager();
    if (!mgr) return c.json({ error: "acme not available" }, 404);
    const ok = mgr.removeCertificate(c.req.param("domain"));
    if (!ok) return c.json({ error: "certificate not found" }, 404);
    return c.json({ ok: true });
  });

  return app;
}
