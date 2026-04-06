import { Hono } from "hono";
import type { RelayInstance, HashcashGate } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";

export function createHashcashRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function gate(): HashcashGate {
    return relay.getCapability<HashcashGate>(RELAY_CAPABILITIES.HASHCASH) as HashcashGate;
  }

  app.use("/*", async (c, next) => {
    if (!relay.getCapability(RELAY_CAPABILITIES.HASHCASH)) {
      return c.json({ error: "hashcash module not installed" }, 404);
    }
    await next();
  });

  app.post("/challenge", async (c) => {
    const body = await c.req.json<{ resource: string }>();
    const challenge = gate().createChallenge(body.resource);
    return c.json(challenge);
  });

  app.post("/verify", async (c) => {
    const body = await c.req.json<{
      challenge: { resource: string; bits: number; issuedAt: string; salt: string };
      counter: number;
      hash: string;
    }>();
    const valid = await gate().verifyProof(body);
    return c.json({ valid });
  });

  return app;
}
