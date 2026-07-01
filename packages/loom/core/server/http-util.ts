//! Tiny request/response helpers shared by the top-level router and every
//! per-event `EventRuntime`. Kept dependency-free (just `node:http`) so the
//! same JSON / SSE / body-parsing conventions apply everywhere and neither
//! the router nor a runtime has to reimplement them.

import type { IncomingMessage, ServerResponse } from "node:http";

/** Write a JSON body with permissive CORS (the app is same-origin, but the
 * Vite dev proxy and QR-linked phones cross origins during development). */
export function sendJson(res: ServerResponse, status: number, body: unknown): void {
  const json = JSON.stringify(body);
  res.writeHead(status, { "content-type": "application/json", "access-control-allow-origin": "*" });
  res.end(json);
}

/** Read + JSON-parse a request body, tolerating an empty or malformed one. */
export async function readBody(req: IncomingMessage): Promise<Record<string, unknown>> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) chunks.push(chunk as Buffer);
  if (chunks.length === 0) return {};
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8")) as Record<string, unknown>;
  } catch {
    return {};
  }
}

/** Read a string field from a parsed body (`""` when absent / non-string). */
export function str(body: Record<string, unknown>, key: string): string {
  const v = body[key];
  return typeof v === "string" ? v : "";
}

/** The caller's session token, from the header or the body (`""` → undefined). */
export function tokenOf(req: IncomingMessage, body: Record<string, unknown>): string | undefined {
  return (req.headers["x-loom-token"] as string | undefined) || str(body, "token") || undefined;
}

/** Emit one Server-Sent Event frame. */
export function sseSend(res: ServerResponse, event: string, data: unknown): void {
  res.write(`event: ${event}\n`);
  res.write(`data: ${JSON.stringify(data)}\n\n`);
}
