import { describe, it, expect } from "vitest";
import {
  createEmailRoutes,
  createMemoryEmailTransport,
  interpolate,
} from "./email-routes.js";

describe("interpolate", () => {
  it("replaces {{key}} placeholders", () => {
    expect(interpolate("Hi {{name}}", { name: "Alice" })).toBe("Hi Alice");
  });

  it("leaves unknown placeholders untouched", () => {
    expect(interpolate("{{missing}}", { other: "x" })).toBe("{{missing}}");
  });

  it("handles whitespace around keys", () => {
    expect(interpolate("{{ name }}", { name: "Bob" })).toBe("Bob");
  });

  it("returns input unchanged when no variables given", () => {
    expect(interpolate("hello")).toBe("hello");
  });

  it("handles multiple replacements", () => {
    expect(interpolate("{{a}} + {{b}} = {{c}}", { a: "1", b: "2", c: "3" })).toBe(
      "1 + 2 = 3",
    );
  });
});

describe("email-routes", () => {
  it("GET /status returns unconfigured when no transport", async () => {
    const app = createEmailRoutes();
    const res = await app.request("/status");
    expect(res.status).toBe(200);
    const body = (await res.json()) as Record<string, unknown>;
    expect(body["configured"]).toBe(false);
  });

  it("GET /status returns configured with provider name", async () => {
    const app = createEmailRoutes({ transport: createMemoryEmailTransport("test") });
    const res = await app.request("/status");
    const body = (await res.json()) as Record<string, unknown>;
    expect(body["configured"]).toBe(true);
    expect(body["provider"]).toBe("test");
  });

  it("POST /send returns 503 when no transport configured", async () => {
    const app = createEmailRoutes();
    const res = await app.request("/send", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ to: "a@b.com", subject: "s", body: "b" }),
    });
    expect(res.status).toBe(503);
  });

  it("POST /send rejects missing fields", async () => {
    const app = createEmailRoutes({ transport: createMemoryEmailTransport() });
    const res = await app.request("/send", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ to: "a@b.com" }),
    });
    expect(res.status).toBe(400);
  });

  it("POST /send rejects invalid email address", async () => {
    const app = createEmailRoutes({ transport: createMemoryEmailTransport() });
    const res = await app.request("/send", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ to: "not-an-email", subject: "s", body: "b" }),
    });
    expect(res.status).toBe(400);
  });

  it("POST /send delivers via transport and interpolates variables", async () => {
    const transport = createMemoryEmailTransport();
    const app = createEmailRoutes({ transport });

    const res = await app.request("/send", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        to: "alice@example.com",
        subject: "Hello {{name}}",
        body: "Welcome, {{name}}!",
        variables: { name: "Alice" },
      }),
    });

    expect(res.status).toBe(200);
    const reply = (await res.json()) as Record<string, unknown>;
    expect(reply["ok"]).toBe(true);

    expect(transport.sent).toHaveLength(1);
    expect(transport.sent[0]?.subject).toBe("Hello Alice");
    expect(transport.sent[0]?.body).toBe("Welcome, Alice!");
  });

  it("POST /send surfaces transport errors as 502", async () => {
    const app = createEmailRoutes({
      transport: {
        provider: "failing",
        async send() {
          throw new Error("boom");
        },
      },
    });
    const res = await app.request("/send", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ to: "a@b.com", subject: "s", body: "b" }),
    });
    expect(res.status).toBe(502);
    const body = (await res.json()) as Record<string, unknown>;
    expect(String(body["error"])).toContain("boom");
  });
});
