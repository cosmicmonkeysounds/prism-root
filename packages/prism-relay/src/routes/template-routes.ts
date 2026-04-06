import { Hono } from "hono";
import type { RelayInstance, PortalTemplateRegistry } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";

export function createTemplateRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function registry(): PortalTemplateRegistry | undefined {
    return relay.getCapability<PortalTemplateRegistry>(RELAY_CAPABILITIES.TEMPLATES);
  }

  app.use("/*", async (c, next) => {
    if (!registry()) {
      return c.json({ error: "templates module not installed" }, 404);
    }
    await next();
  });

  app.get("/", (c) => {
    const reg = registry();
    if (!reg) return c.json({ error: "templates not available" }, 404);
    return c.json(reg.list());
  });

  app.post("/", async (c) => {
    const reg = registry();
    if (!reg) return c.json({ error: "templates not available" }, 404);
    const body = await c.req.json<{
      name: string;
      description: string;
      css: string;
      headerHtml: string;
      footerHtml: string;
      objectCardHtml: string;
    }>();
    const template = reg.register(body);
    return c.json(template, 201);
  });

  app.get("/:id", (c) => {
    const reg = registry();
    if (!reg) return c.json({ error: "templates not available" }, 404);
    const template = reg.get(c.req.param("id"));
    if (!template) return c.json({ error: "template not found" }, 404);
    return c.json(template);
  });

  app.delete("/:id", (c) => {
    const reg = registry();
    if (!reg) return c.json({ error: "templates not available" }, 404);
    const ok = reg.remove(c.req.param("id"));
    if (!ok) return c.json({ error: "template not found" }, 404);
    return c.json({ ok: true });
  });

  return app;
}
