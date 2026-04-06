/**
 * Auth Routes — OAuth/OIDC authentication and Blind Escrow key derivation.
 *
 * Provides Web 2.0 auth flows for non-technical users:
 * - Google/GitHub OIDC login → session token
 * - Blind Escrow: derive key from password + OAuth salt → encrypt master vault key
 * - Relay never sees the raw master key; breach yields useless encrypted blobs
 */

import { Hono } from "hono";
import type { Context as HonoContext } from "hono";
import type { RelayInstance, CapabilityTokenManager } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { EscrowManager } from "@prism/core/trust";

/** OAuth provider configuration. */
export interface OAuthProviderConfig {
  clientId: string;
  clientSecret: string;
  redirectUri: string;
}

/** Auth route options. */
export interface AuthRoutesOptions {
  google?: OAuthProviderConfig;
  github?: OAuthProviderConfig;
  /** Session token TTL in milliseconds. Default: 24 hours. */
  sessionTtlMs?: number;
}

export function createAuthRoutes(
  relay: RelayInstance,
  options: AuthRoutesOptions = {},
): Hono {
  const app = new Hono();
  const sessionTtlMs = options.sessionTtlMs ?? 24 * 60 * 60 * 1000;

  function getTokenManager(): CapabilityTokenManager | undefined {
    return relay.getCapability<CapabilityTokenManager>(RELAY_CAPABILITIES.TOKENS);
  }

  function getEscrow(): EscrowManager | undefined {
    return relay.getCapability<EscrowManager>(RELAY_CAPABILITIES.ESCROW);
  }

  // GET /api/auth/providers — list available OAuth providers
  app.get("/providers", (c) => {
    const providers: string[] = [];
    if (options.google) providers.push("google");
    if (options.github) providers.push("github");
    return c.json({ providers });
  });

  // GET /api/auth/google — redirect to Google OIDC
  app.get("/google", (c) => {
    if (!options.google) return c.json({ error: "Google auth not configured" }, 404);
    const { clientId, redirectUri } = options.google;
    const state = generateState();
    const url = `https://accounts.google.com/o/oauth2/v2/auth?client_id=${encodeURIComponent(clientId)}&redirect_uri=${encodeURIComponent(redirectUri)}&response_type=code&scope=openid+email+profile&state=${state}`;
    return c.redirect(url);
  });

  // GET /api/auth/github — redirect to GitHub OAuth
  app.get("/github", (c) => {
    if (!options.github) return c.json({ error: "GitHub auth not configured" }, 404);
    const { clientId, redirectUri } = options.github;
    const state = generateState();
    const url = `https://github.com/login/oauth/authorize?client_id=${encodeURIComponent(clientId)}&redirect_uri=${encodeURIComponent(redirectUri)}&scope=read:user+user:email&state=${state}`;
    return c.redirect(url);
  });

  // POST /api/auth/callback/google — exchange code for token
  app.post("/callback/google", async (c) => {
    if (!options.google) return c.json({ error: "Google auth not configured" }, 404);
    const { clientId, clientSecret, redirectUri } = options.google;
    const { code } = await c.req.json<{ code: string }>();
    if (!code) return c.json({ error: "missing code" }, 400);

    try {
      const tokenResponse = await fetch("https://oauth2.googleapis.com/token", {
        method: "POST",
        headers: { "Content-Type": "application/x-www-form-urlencoded" },
        body: new URLSearchParams({
          code,
          client_id: clientId,
          client_secret: clientSecret,
          redirect_uri: redirectUri,
          grant_type: "authorization_code",
        }),
      });

      if (!tokenResponse.ok) {
        return c.json({ error: "token exchange failed" }, 401);
      }

      const tokenData = await tokenResponse.json() as { id_token?: string; access_token?: string };
      if (!tokenData.id_token) {
        return c.json({ error: "no id_token in response" }, 401);
      }

      // Decode JWT payload (no verification needed — Google signed it)
      const claims = decodeJwtPayload(tokenData.id_token);
      if (!claims.sub || !claims.email) {
        return c.json({ error: "invalid id_token claims" }, 401);
      }

      // Issue a Prism capability token for this session
      return await issueSessionToken(c, claims.sub as string, claims.email as string, "google");
    } catch (err) {
      return c.json({ error: `auth failed: ${String(err)}` }, 500);
    }
  });

  // POST /api/auth/callback/github — exchange code for token
  app.post("/callback/github", async (c) => {
    if (!options.github) return c.json({ error: "GitHub auth not configured" }, 404);
    const { clientId, clientSecret } = options.github;
    const { code } = await c.req.json<{ code: string }>();
    if (!code) return c.json({ error: "missing code" }, 400);

    try {
      const tokenResponse = await fetch("https://github.com/login/oauth/access_token", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Accept: "application/json",
        },
        body: JSON.stringify({ client_id: clientId, client_secret: clientSecret, code }),
      });

      if (!tokenResponse.ok) {
        return c.json({ error: "token exchange failed" }, 401);
      }

      const tokenData = await tokenResponse.json() as { access_token?: string };
      if (!tokenData.access_token) {
        return c.json({ error: "no access_token in response" }, 401);
      }

      // Fetch user profile
      const userResponse = await fetch("https://api.github.com/user", {
        headers: { Authorization: `Bearer ${tokenData.access_token}`, Accept: "application/json" },
      });
      const user = await userResponse.json() as { id?: number; email?: string; login?: string };
      if (!user.id) {
        return c.json({ error: "failed to fetch user profile" }, 401);
      }

      return await issueSessionToken(
        c,
        String(user.id),
        user.email ?? `${user.login ?? "unknown"}@github`,
        "github",
      );
    } catch (err) {
      return c.json({ error: `auth failed: ${String(err)}` }, 500);
    }
  });

  // POST /api/auth/escrow/derive — Blind Escrow key derivation
  // Client sends password + OAuth salt → derive escrow key → encrypt vault key → store
  app.post("/escrow/derive", async (c) => {
    const escrow = getEscrow();
    if (!escrow) return c.json({ error: "escrow module not installed" }, 404);

    const body = await c.req.json<{
      depositorId: string;
      password: string;
      oauthSalt: string;
      encryptedVaultKey: string;
      expiresAt?: string;
    }>();

    if (!body.depositorId || !body.password || !body.oauthSalt || !body.encryptedVaultKey) {
      return c.json({ error: "depositorId, password, oauthSalt, and encryptedVaultKey are required" }, 400);
    }

    // Derive escrow key using PBKDF2: password + OAuth-derived salt
    // The actual encryption happens client-side. The relay stores the
    // already-encrypted blob. This endpoint records the derivation params.
    const escrowKeyHash = await deriveEscrowKeyHash(body.password, body.oauthSalt);

    // Store the encrypted vault key with the escrow key hash as metadata
    const deposit = escrow.deposit(
      body.depositorId,
      JSON.stringify({
        encryptedVaultKey: body.encryptedVaultKey,
        escrowKeyHash,
        derivation: "pbkdf2-sha256-600000",
      }),
      body.expiresAt,
    );

    return c.json({ ok: true, depositId: deposit.id }, 201);
  });

  // POST /api/auth/escrow/recover — recover vault key using password + OAuth salt
  app.post("/escrow/recover", async (c) => {
    const escrow = getEscrow();
    if (!escrow) return c.json({ error: "escrow module not installed" }, 404);

    const body = await c.req.json<{
      depositorId: string;
      password: string;
      oauthSalt: string;
    }>();

    if (!body.depositorId || !body.password || !body.oauthSalt) {
      return c.json({ error: "depositorId, password, and oauthSalt are required" }, 400);
    }

    const deposits = escrow.listDeposits(body.depositorId);
    if (deposits.length === 0) {
      return c.json({ error: "no escrow deposits found" }, 404);
    }

    const escrowKeyHash = await deriveEscrowKeyHash(body.password, body.oauthSalt);

    // Find deposit matching this escrow key hash
    for (const deposit of deposits) {
      if (deposit.claimed) continue;
      try {
        const data = JSON.parse(deposit.encryptedPayload) as {
          encryptedVaultKey: string;
          escrowKeyHash: string;
        };
        if (data.escrowKeyHash === escrowKeyHash) {
          return c.json({
            ok: true,
            depositId: deposit.id,
            encryptedVaultKey: data.encryptedVaultKey,
          });
        }
      } catch {
        continue;
      }
    }

    return c.json({ error: "escrow key mismatch — wrong password or OAuth account" }, 403);
  });

  // ── Helpers ────────────────────────────────────────────────────────────

  async function issueSessionToken(
    c: HonoContext,
    externalId: string,
    email: string,
    provider: string,
  ) {
    const tokenManager = getTokenManager();
    if (!tokenManager) {
      return c.json({ error: "capability tokens module not installed" }, 503);
    }

    const oauthDid = `did:oauth:${provider}:${externalId}` as import("@prism/core/identity").DID;
    const token = await tokenManager.issue({
      subject: oauthDid,
      permissions: ["read", "write"],
      scope: "*",
      ttlMs: sessionTtlMs,
    });

    // Base64 encode the token for transport
    const serialized = Buffer.from(JSON.stringify({
      ...token,
      signature: Buffer.from(token.signature).toString("base64"),
    })).toString("base64");

    return c.json({
      ok: true,
      provider,
      email,
      externalId,
      did: oauthDid,
      token: serialized,
      expiresAt: token.expiresAt,
    });
  }

  return app;
}

// ── Utility Functions ──────────────────────────────────────────────────────

function generateState(): string {
  const bytes = new Uint8Array(16);
  globalThis.crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function decodeJwtPayload(jwt: string): Record<string, unknown> {
  const parts = jwt.split(".");
  if (parts.length !== 3) return {};
  const payload = Buffer.from(parts[1] as string, "base64url").toString("utf-8");
  try {
    return JSON.parse(payload) as Record<string, unknown>;
  } catch {
    return {};
  }
}

async function deriveEscrowKeyHash(password: string, salt: string): Promise<string> {
  const enc = new TextEncoder();
  const keyMaterial = await globalThis.crypto.subtle.importKey(
    "raw",
    enc.encode(password),
    "PBKDF2",
    false,
    ["deriveBits"],
  );
  const derived = await globalThis.crypto.subtle.deriveBits(
    {
      name: "PBKDF2",
      salt: enc.encode(salt),
      iterations: 600_000,
      hash: "SHA-256",
    },
    keyMaterial,
    256,
  );
  // Return hex hash of derived key (relay stores hash, not the key itself)
  const hashBuf = await globalThis.crypto.subtle.digest("SHA-256", derived);
  return Array.from(new Uint8Array(hashBuf), (b) => b.toString(16).padStart(2, "0")).join("");
}

