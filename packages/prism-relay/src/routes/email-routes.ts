/**
 * Email routes — HTTP surface for dispatching outgoing mail.
 *
 * Email delivery itself is not a Prism Layer 1 primitive: different
 * deployments use different providers (SendGrid, Mailgun, SMTP, etc.).
 * This route accepts a pluggable EmailTransport and delegates to it,
 * interpolating {{field}} placeholders in the subject/body.
 *
 *   POST /api/email/send
 *     body: { to, subject, body, templateId?, variables? }
 *     reply: { ok: true, id? } | { error }
 *
 *   GET  /api/email/status
 *     reply: { configured: boolean, provider?: string }
 *
 * If no transport is provided, the routes still mount but return 503
 * on /send so clients can probe for availability without crashing.
 */

import { Hono } from "hono";

// ── Transport interface ─────────────────────────────────────────────────────

export interface EmailSendRequest {
  to: string;
  subject: string;
  body: string;
  templateId?: string | undefined;
  variables?: Record<string, string> | undefined;
}

export interface EmailSendResult {
  ok: true;
  id?: string;
}

export interface EmailTransport {
  /** Human-readable provider name for /status. */
  readonly provider: string;
  /** Deliver a single email. Throws on unrecoverable errors. */
  send(request: EmailSendRequest): Promise<EmailSendResult>;
}

// ── Template interpolation ──────────────────────────────────────────────────

/**
 * Replace {{key}} placeholders in the input with matching `variables`
 * values. Missing keys are left untouched so the caller can see them.
 */
export function interpolate(input: string, variables?: Record<string, string>): string {
  if (!variables) return input;
  return input.replace(/\{\{\s*(\w+)\s*\}\}/g, (match, key: string) => {
    const val = variables[key];
    return val === undefined ? match : val;
  });
}

// ── Routes ──────────────────────────────────────────────────────────────────

export interface EmailRoutesOptions {
  transport?: EmailTransport;
}

export function createEmailRoutes(options: EmailRoutesOptions = {}): Hono {
  const app = new Hono();
  const { transport } = options;

  app.get("/status", (c) => {
    if (!transport) return c.json({ configured: false });
    return c.json({ configured: true, provider: transport.provider });
  });

  app.post("/send", async (c) => {
    if (!transport) {
      return c.json({ error: "email transport not configured" }, 503);
    }

    let body: EmailSendRequest;
    try {
      body = await c.req.json<EmailSendRequest>();
    } catch {
      return c.json({ error: "invalid JSON body" }, 400);
    }

    if (!body.to || !body.subject || !body.body) {
      return c.json({ error: "to, subject, and body are required" }, 400);
    }

    // Basic email format check — not RFC-perfect, just a smoke test.
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(body.to)) {
      return c.json({ error: "invalid 'to' address" }, 400);
    }

    const prepared: EmailSendRequest = {
      to: body.to,
      subject: interpolate(body.subject, body.variables),
      body: interpolate(body.body, body.variables),
      templateId: body.templateId,
      variables: body.variables,
    };

    try {
      const result = await transport.send(prepared);
      return c.json(result);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return c.json({ error: `delivery failed: ${message}` }, 502);
    }
  });

  return app;
}

// ── Test helper ─────────────────────────────────────────────────────────────

/**
 * Create an in-memory transport that records every outgoing message.
 * Useful for tests and for local development without a real provider.
 */
export function createMemoryEmailTransport(name = "memory"): EmailTransport & {
  sent: EmailSendRequest[];
} {
  const sent: EmailSendRequest[] = [];
  return {
    provider: name,
    sent,
    async send(request) {
      sent.push(request);
      return { ok: true as const, id: `mem-${sent.length}` };
    },
  };
}
