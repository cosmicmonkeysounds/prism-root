import { Hono } from "hono";
import type { RelayInstance, CapabilityTokenManager, CapabilityToken } from "@prism/core/relay";
import type { DID } from "@prism/core/identity";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { encodeBase64, decodeBase64 } from "../protocol/relay-protocol.js";

export function createTokenRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function manager(): CapabilityTokenManager {
    return relay.getCapability<CapabilityTokenManager>(RELAY_CAPABILITIES.TOKENS) as CapabilityTokenManager;
  }

  app.use("/*", async (c, next) => {
    if (!relay.getCapability(RELAY_CAPABILITIES.TOKENS)) {
      return c.json({ error: "tokens module not installed" }, 404);
    }
    await next();
  });

  app.post("/issue", async (c) => {
    const body = await c.req.json<{
      subject: DID | "*";
      permissions: string[];
      scope: string;
      ttlMs?: number;
    }>();
    const token = await manager().issue(body);
    return c.json(serializeToken(token), 201);
  });

  app.post("/verify", async (c) => {
    const body = await c.req.json<SerializedCapabilityToken>();
    const token = deserializeToken(body);
    const result = await manager().verify(token);
    return c.json(result);
  });

  app.post("/revoke", async (c) => {
    const body = await c.req.json<{ tokenId: string }>();
    manager().revoke(body.tokenId);
    return c.json({ ok: true });
  });

  return app;
}

// Serialized token type for the wire (signature as base64)
interface SerializedCapabilityToken {
  tokenId: string;
  issuer: DID;
  subject: DID | "*";
  permissions: string[];
  scope: string;
  issuedAt: string;
  expiresAt: string | null;
  signature: string;
}

export function serializeToken(t: CapabilityToken): SerializedCapabilityToken {
  return {
    tokenId: t.tokenId,
    issuer: t.issuer,
    subject: t.subject,
    permissions: t.permissions,
    scope: t.scope,
    issuedAt: t.issuedAt,
    expiresAt: t.expiresAt,
    signature: encodeBase64(t.signature),
  };
}

export function deserializeToken(s: SerializedCapabilityToken): CapabilityToken {
  return {
    tokenId: s.tokenId,
    issuer: s.issuer,
    subject: s.subject,
    permissions: s.permissions,
    scope: s.scope,
    issuedAt: s.issuedAt,
    expiresAt: s.expiresAt,
    signature: decodeBase64(s.signature),
  };
}
