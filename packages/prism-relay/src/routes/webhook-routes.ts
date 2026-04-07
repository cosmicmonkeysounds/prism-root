import { Hono } from "hono";
import type { RelayInstance, WebhookEmitter } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";

export function createWebhookRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function emitter(): WebhookEmitter {
    return relay.getCapability<WebhookEmitter>(RELAY_CAPABILITIES.WEBHOOKS) as WebhookEmitter;
  }

  app.use("/*", async (c, next) => {
    if (!relay.getCapability(RELAY_CAPABILITIES.WEBHOOKS)) {
      return c.json({ error: "webhooks module not installed" }, 404);
    }
    await next();
  });

  app.get("/", (c) => {
    return c.json(emitter().list());
  });

  app.post("/", async (c) => {
    const body = await c.req.json<{ url: string; events: string[]; secret?: string; active?: boolean }>();
    const registration: Omit<import("@prism/core/relay").WebhookConfig, "id"> = {
      url: body.url,
      events: body.events,
      active: body.active ?? true,
    };
    if (body.secret !== undefined) registration.secret = body.secret;
    const config = emitter().register(registration);
    return c.json(config, 201);
  });

  app.delete("/:id", (c) => {
    const ok = emitter().unregister(c.req.param("id"));
    if (!ok) return c.json({ error: "webhook not found" }, 404);
    return c.json({ ok: true });
  });

  app.post("/:id/test", async (c) => {
    const id = c.req.param("id");
    const webhook = emitter().list().find((w) => w.id === id);
    if (!webhook) return c.json({ error: "webhook not found" }, 404);
    await emitter().emit("webhook.test", {
      webhookId: id,
      timestamp: new Date().toISOString(),
    });
    return c.json({ ok: true, deliveredTo: webhook.url });
  });

  app.get("/:id/deliveries", (c) => {
    return c.json(emitter().deliveries(c.req.param("id")));
  });

  return app;
}
