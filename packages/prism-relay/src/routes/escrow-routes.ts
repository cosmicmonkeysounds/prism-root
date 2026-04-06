import { Hono } from "hono";
import type { RelayInstance } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { EscrowManager } from "@prism/core/trust";

export function createEscrowRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function mgr(): EscrowManager {
    return relay.getCapability<EscrowManager>(RELAY_CAPABILITIES.ESCROW) as EscrowManager;
  }

  app.use("/*", async (c, next) => {
    if (!relay.getCapability(RELAY_CAPABILITIES.ESCROW)) {
      return c.json({ error: "escrow module not installed" }, 404);
    }
    await next();
  });

  app.post("/deposit", async (c) => {
    const body = await c.req.json<{
      depositorId: string;
      encryptedPayload: string;
      expiresAt?: string;
    }>();
    const deposit = mgr().deposit(body.depositorId, body.encryptedPayload, body.expiresAt);
    return c.json(deposit, 201);
  });

  app.post("/claim", async (c) => {
    const body = await c.req.json<{ depositId: string }>();
    const deposit = mgr().claim(body.depositId);
    if (!deposit) return c.json({ error: "deposit not found or already claimed" }, 404);
    return c.json(deposit);
  });

  app.get("/:depositorId", (c) => {
    return c.json(mgr().listDeposits(c.req.param("depositorId")));
  });

  return app;
}
