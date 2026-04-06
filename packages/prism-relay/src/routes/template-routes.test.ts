import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  portalTemplateModule,
} from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createTemplateRoutes } from "./template-routes.js";

let relay: RelayInstance;

beforeAll(async () => {
  const identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(portalTemplateModule())
    .build();
  await relay.start();
});

describe("template-routes", () => {
  let templateId: string;

  it("registers a template", async () => {
    const app = createTemplateRoutes(relay);
    const res = await app.request("/", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: "Dark Theme",
        description: "A dark-themed portal layout",
        css: ":root { --bg: #000; --fg: #fff; }",
        headerHtml: "<h1>{{portalName}}</h1>",
        footerHtml: "<footer>{{objectCount}} objects</footer>",
        objectCardHtml: "<div class='card'>{{name}}</div>",
      }),
    });
    expect(res.status).toBe(201);
    const body = await res.json() as { templateId: string; name: string };
    expect(body.templateId).toBeDefined();
    expect(body.name).toBe("Dark Theme");
    templateId = body.templateId;
  });

  it("lists templates", async () => {
    const app = createTemplateRoutes(relay);
    const res = await app.request("/");
    expect(res.status).toBe(200);
    const body = await res.json() as Array<{ name: string }>;
    expect(body.some((t) => t.name === "Dark Theme")).toBe(true);
  });

  it("gets a template by ID", async () => {
    const app = createTemplateRoutes(relay);
    const res = await app.request(`/${templateId}`);
    expect(res.status).toBe(200);
    const body = await res.json() as { name: string; css: string };
    expect(body.name).toBe("Dark Theme");
    expect(body.css).toContain("--bg: #000");
  });

  it("returns 404 for unknown template", async () => {
    const app = createTemplateRoutes(relay);
    const res = await app.request("/nonexistent");
    expect(res.status).toBe(404);
  });

  it("deletes a template", async () => {
    const app = createTemplateRoutes(relay);
    const res = await app.request(`/${templateId}`, { method: "DELETE" });
    expect(res.status).toBe(200);

    const getRes = await app.request(`/${templateId}`);
    expect(getRes.status).toBe(404);
  });
});
