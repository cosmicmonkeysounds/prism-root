import { Hono } from "hono";

export interface LogEntry {
  ts: string;
  level: string;
  msg: string;
  data?: Record<string, unknown>;
}

export interface LogBuffer {
  push(entry: LogEntry): void;
  getAll(): LogEntry[];
  getFiltered(level?: string, limit?: number): LogEntry[];
  clear(): void;
}

export function createLogBuffer(maxSize: number = 1000): LogBuffer {
  const entries: LogEntry[] = [];
  return {
    push(entry) {
      entries.push(entry);
      if (entries.length > maxSize) entries.shift();
    },
    getAll() {
      return [...entries];
    },
    getFiltered(level, limit = 100) {
      const result = level
        ? entries.filter((e) => e.level === level)
        : [...entries];
      return result.slice(-limit);
    },
    clear() {
      entries.length = 0;
    },
  };
}

export function createLogsRoutes(buffer: LogBuffer): Hono {
  const app = new Hono();

  app.get("/", (c) => {
    const level = c.req.query("level");
    const limit = parseInt(c.req.query("limit") ?? "100", 10);
    return c.json(buffer.getFiltered(level ?? undefined, limit));
  });

  app.delete("/", (c) => {
    buffer.clear();
    return c.json({ ok: true });
  });

  return app;
}
