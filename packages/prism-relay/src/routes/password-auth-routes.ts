/**
 * Password Authentication Routes — traditional username/password login.
 *
 * Backed by the password-auth module from @prism/core/relay. Issues a Prism
 * capability token on successful login when the capability-tokens module is
 * also installed; otherwise returns a token-less success response.
 *
 * All routes return 404 when the password-auth module is not installed,
 * which is what enables relays to be built with escrow only, password only,
 * both, or neither.
 */

import { Hono } from "hono";
import type { Context as HonoContext } from "hono";
import type { RelayInstance, CapabilityTokenManager } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type {
  PasswordAuthManager,
  PasswordAuthRecord,
} from "@prism/core/trust";
import type { DID } from "@prism/core/identity";

export interface PasswordAuthRoutesOptions {
  /** Session token TTL in milliseconds. Default: 24 hours. */
  sessionTtlMs?: number;
}

export function createPasswordAuthRoutes(
  relay: RelayInstance,
  options: PasswordAuthRoutesOptions = {},
): Hono {
  const app = new Hono();
  const sessionTtlMs = options.sessionTtlMs ?? 24 * 60 * 60 * 1000;

  function manager(): PasswordAuthManager {
    return relay.getCapability<PasswordAuthManager>(
      RELAY_CAPABILITIES.PASSWORD_AUTH,
    ) as PasswordAuthManager;
  }

  app.use("/*", async (c, next) => {
    if (!relay.getCapability(RELAY_CAPABILITIES.PASSWORD_AUTH)) {
      return c.json({ error: "password-auth module not installed" }, 404);
    }
    await next();
  });

  // POST /register — create a new user
  app.post("/register", async (c) => {
    const body = await c.req.json<{
      username?: string;
      password?: string;
      did?: string;
      metadata?: Record<string, string>;
    }>();
    if (!body.username || !body.password) {
      return c.json({ error: "username and password are required" }, 400);
    }
    try {
      const record = await manager().register({
        username: body.username,
        password: body.password,
        ...(body.did !== undefined ? { did: body.did } : {}),
        ...(body.metadata !== undefined ? { metadata: body.metadata } : {}),
      });
      return c.json(redact(record), 201);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      const status = message.includes("already registered") ? 409 : 400;
      return c.json({ error: message }, status);
    }
  });

  // POST /login — verify credentials and (optionally) issue a capability token
  app.post("/login", async (c) => {
    const body = await c.req.json<{ username?: string; password?: string }>();
    if (!body.username || !body.password) {
      return c.json({ error: "username and password are required" }, 400);
    }
    const result = await manager().verify(body.username, body.password);
    if (!result.ok) {
      const status = result.reason === "unknown-user" ? 404 : 401;
      return c.json({ error: result.reason }, status);
    }
    return await issueLoginResponse(c, result.record);
  });

  // POST /change — change password
  app.post("/change", async (c) => {
    const body = await c.req.json<{
      username?: string;
      oldPassword?: string;
      newPassword?: string;
    }>();
    if (!body.username || !body.oldPassword || !body.newPassword) {
      return c.json(
        { error: "username, oldPassword and newPassword are required" },
        400,
      );
    }
    const result = await manager().changePassword(
      body.username,
      body.oldPassword,
      body.newPassword,
    );
    if (!result.ok) {
      const status = result.reason === "unknown-user" ? 404 : 401;
      return c.json({ error: result.reason }, status);
    }
    return c.json({ ok: true, record: redact(result.record) });
  });

  // GET /:username — fetch a user record (no password material)
  app.get("/:username", (c) => {
    const username = c.req.param("username");
    const record = manager().get(username);
    if (!record) return c.json({ error: "unknown-user" }, 404);
    return c.json(redact(record));
  });

  // DELETE /:username — remove a user. Requires the current password to be
  // sent in the body. (Avoids ambient deletion via a leaked admin token.)
  app.delete("/:username", async (c) => {
    const username = c.req.param("username");
    let body: { password?: string } = {};
    try {
      body = await c.req.json<{ password?: string }>();
    } catch {
      return c.json({ error: "password is required" }, 400);
    }
    if (!body.password) {
      return c.json({ error: "password is required" }, 400);
    }
    const verified = await manager().verify(username, body.password);
    if (!verified.ok) {
      const status = verified.reason === "unknown-user" ? 404 : 401;
      return c.json({ error: verified.reason }, status);
    }
    manager().remove(username);
    return c.json({ ok: true });
  });

  async function issueLoginResponse(
    c: HonoContext,
    record: PasswordAuthRecord,
  ) {
    const tokens = relay.getCapability<CapabilityTokenManager>(
      RELAY_CAPABILITIES.TOKENS,
    );
    if (!tokens) {
      // Tokens module not installed — still return a successful login.
      return c.json({
        ok: true,
        did: record.did,
        username: record.username,
        token: null,
        expiresAt: null,
      });
    }
    const token = await tokens.issue({
      subject: record.did as DID,
      permissions: ["read", "write"],
      scope: "*",
      ttlMs: sessionTtlMs,
    });
    const serialized = Buffer.from(
      JSON.stringify({
        ...token,
        signature: Buffer.from(token.signature).toString("base64"),
      }),
    ).toString("base64");
    return c.json({
      ok: true,
      did: record.did,
      username: record.username,
      token: serialized,
      expiresAt: token.expiresAt,
    });
  }

  return app;
}

/** Strip password material from a record before returning over HTTP. */
function redact(record: PasswordAuthRecord): Omit<
  PasswordAuthRecord,
  "passwordHash" | "salt"
> {
  const rest: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(record)) {
    if (k === "passwordHash" || k === "salt") continue;
    rest[k] = v;
  }
  return rest as Omit<PasswordAuthRecord, "passwordHash" | "salt">;
}
