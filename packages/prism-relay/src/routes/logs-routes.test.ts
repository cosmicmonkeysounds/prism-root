import { describe, it, expect } from "vitest";
import { createLogBuffer, createLogsRoutes } from "./logs-routes.js";

describe("logs-routes", () => {
  it("GET / returns empty array initially", async () => {
    const buffer = createLogBuffer();
    const app = createLogsRoutes(buffer);
    const res = await app.request("/");
    expect(res.status).toBe(200);
    const body = (await res.json()) as unknown[];
    expect(body).toEqual([]);
  });

  it("GET / returns pushed entries", async () => {
    const buffer = createLogBuffer();
    buffer.push({ ts: "2026-01-01T00:00:00Z", level: "info", msg: "hello" });
    buffer.push({ ts: "2026-01-01T00:00:01Z", level: "warn", msg: "watch out" });

    const app = createLogsRoutes(buffer);
    const res = await app.request("/");
    expect(res.status).toBe(200);
    const body = (await res.json()) as Array<Record<string, unknown>>;
    expect(body).toHaveLength(2);
    expect(body[0]?.["msg"]).toBe("hello");
    expect(body[1]?.["msg"]).toBe("watch out");
  });

  it("GET /?level=error filters by level", async () => {
    const buffer = createLogBuffer();
    buffer.push({ ts: "2026-01-01T00:00:00Z", level: "info", msg: "info msg" });
    buffer.push({ ts: "2026-01-01T00:00:01Z", level: "error", msg: "error msg" });
    buffer.push({ ts: "2026-01-01T00:00:02Z", level: "warn", msg: "warn msg" });

    const app = createLogsRoutes(buffer);
    const res = await app.request("/?level=error");
    expect(res.status).toBe(200);
    const body = (await res.json()) as Array<Record<string, unknown>>;
    expect(body).toHaveLength(1);
    expect(body[0]?.["level"]).toBe("error");
    expect(body[0]?.["msg"]).toBe("error msg");
  });

  it("GET /?limit=2 respects limit", async () => {
    const buffer = createLogBuffer();
    buffer.push({ ts: "t1", level: "info", msg: "a" });
    buffer.push({ ts: "t2", level: "info", msg: "b" });
    buffer.push({ ts: "t3", level: "info", msg: "c" });

    const app = createLogsRoutes(buffer);
    const res = await app.request("/?limit=2");
    expect(res.status).toBe(200);
    const body = (await res.json()) as Array<Record<string, unknown>>;
    expect(body).toHaveLength(2);
    // Should return the last 2 entries (slice(-limit))
    expect(body[0]?.["msg"]).toBe("b");
    expect(body[1]?.["msg"]).toBe("c");
  });

  it("DELETE / clears the buffer", async () => {
    const buffer = createLogBuffer();
    buffer.push({ ts: "t1", level: "info", msg: "entry" });

    const app = createLogsRoutes(buffer);
    const delRes = await app.request("/", { method: "DELETE" });
    expect(delRes.status).toBe(200);
    const delBody = (await delRes.json()) as Record<string, unknown>;
    expect(delBody["ok"]).toBe(true);

    const getRes = await app.request("/");
    const getBody = (await getRes.json()) as unknown[];
    expect(getBody).toHaveLength(0);
  });
});

describe("createLogBuffer", () => {
  it("respects maxSize by dropping oldest entries", () => {
    const buffer = createLogBuffer(3);
    buffer.push({ ts: "t1", level: "info", msg: "a" });
    buffer.push({ ts: "t2", level: "info", msg: "b" });
    buffer.push({ ts: "t3", level: "info", msg: "c" });
    buffer.push({ ts: "t4", level: "info", msg: "d" });

    const all = buffer.getAll();
    expect(all).toHaveLength(3);
    // Oldest ("a") should have been dropped
    expect(all[0]?.msg).toBe("b");
    expect(all[1]?.msg).toBe("c");
    expect(all[2]?.msg).toBe("d");
  });
});
