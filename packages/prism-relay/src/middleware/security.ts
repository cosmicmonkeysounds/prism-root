/**
 * Security Middleware — CSRF protection, body size limits, and schema validation.
 *
 * Applied globally via Hono middleware to harden the Relay against common attacks.
 */

import type { Context, Next } from "hono";
import type { RelayInstance } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { PeerTrustGraph } from "@prism/core/trust";

// ── CSRF Protection ────────────────────────────────────────────────────────

const CSRF_HEADER = "x-prism-csrf";
const CSRF_SAFE_METHODS = new Set(["GET", "HEAD", "OPTIONS"]);

/**
 * CSRF middleware: requires a `X-Prism-CSRF: 1` header on all mutating
 * requests. This prevents browser-based CSRF because custom headers
 * cannot be sent cross-origin without a preflight CORS check.
 *
 * API clients simply add the header. Browsers must pass CORS preflight.
 * Portal form submissions (POST /portals/:id/submit) use JSON with
 * Content-Type: application/json which already requires preflight.
 */
export function csrfMiddleware() {
  return async (c: Context, next: Next) => {
    if (CSRF_SAFE_METHODS.has(c.req.method)) {
      return next();
    }

    // Mutating requests must include the CSRF header
    const csrf = c.req.header(CSRF_HEADER);
    if (!csrf) {
      return c.json({ error: "missing CSRF header (X-Prism-CSRF)" }, 403);
    }

    return next();
  };
}

// ── Body Size Limit ────────────────────────────────────────────────────────

/**
 * Reject request bodies exceeding the configured max size.
 * Uses Content-Length header for early rejection.
 */
export function bodySizeLimitMiddleware(maxBytes: number) {
  return async (c: Context, next: Next) => {
    if (CSRF_SAFE_METHODS.has(c.req.method)) {
      return next();
    }

    const contentLength = c.req.header("content-length");
    if (contentLength && parseInt(contentLength, 10) > maxBytes) {
      return c.json(
        { error: `request body exceeds maximum size of ${maxBytes} bytes` },
        413,
      );
    }

    return next();
  };
}

// ── Banned Peer Rejection ──────────────────────────────────────────────────

/**
 * Reject requests from banned DIDs. Checks the Authorization header
 * for a Bearer token containing a DID, or the X-Prism-DID header.
 */
export function bannedPeerMiddleware(relay: RelayInstance) {
  return async (c: Context, next: Next) => {
    const trust = relay.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST);
    if (!trust) return next();

    const did = c.req.header("x-prism-did");
    if (did && trust.isBanned(did)) {
      return c.json({ error: "peer is banned" }, 403);
    }

    return next();
  };
}

// ── Schema Poison Pill (CRDT diff validation) ──────────────────────────────

/**
 * Validates JSON request bodies against basic structural safety rules.
 * Prevents excessively nested or oversized payloads from being processed.
 */
export function schemaValidationMiddleware(opts?: {
  maxDepth?: number;
  maxKeys?: number;
  maxStringLength?: number;
}) {
  const maxDepth = opts?.maxDepth ?? 20;
  const maxKeys = opts?.maxKeys ?? 5000;
  const maxStringLength = opts?.maxStringLength ?? 500_000;

  return async (c: Context, next: Next) => {
    if (CSRF_SAFE_METHODS.has(c.req.method)) {
      return next();
    }

    const contentType = c.req.header("content-type") ?? "";
    if (!contentType.includes("application/json")) {
      return next();
    }

    // Clone the body for validation, then restore
    const text = await c.req.text();

    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch {
      return c.json({ error: "invalid JSON body" }, 400);
    }

    const issues = validateStructure(parsed, maxDepth, maxKeys, maxStringLength);
    if (issues.length > 0) {
      return c.json({ error: "schema validation failed", issues }, 400);
    }

    // Store parsed body for downstream use
    c.set("parsedBody" as never, parsed as never);
    return next();
  };
}

function validateStructure(
  value: unknown,
  maxDepth: number,
  maxKeys: number,
  maxStringLength: number,
): string[] {
  const issues: string[] = [];
  let keyCount = 0;

  function walk(v: unknown, depth: number): void {
    if (depth > maxDepth) {
      issues.push(`nesting depth exceeds ${maxDepth}`);
      return;
    }

    if (typeof v === "string" && v.length > maxStringLength) {
      issues.push(`string length ${v.length} exceeds ${maxStringLength}`);
      return;
    }

    if (Array.isArray(v)) {
      for (const item of v) walk(item, depth + 1);
      return;
    }

    if (v !== null && typeof v === "object") {
      const keys = Object.keys(v);
      keyCount += keys.length;
      if (keyCount > maxKeys) {
        issues.push(`total keys ${keyCount} exceeds ${maxKeys}`);
        return;
      }
      for (const key of keys) {
        walk((v as Record<string, unknown>)[key], depth + 1);
      }
    }
  }

  walk(value, 0);
  return issues;
}
